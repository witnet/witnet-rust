//! Implementations of CLI methods related to Witnet data requests.

use std::{fs::File, io::Read, path::Path};

use regex::Regex;

use witnet_data_structures::chain::DataRequestOutput;
use witnet_rad::RADRequestExecutionReport;

use crate::{errors::Error, lib};

use super::arguments;

/// Decode a data request from a `DecodeDataRequest` structure.
pub(crate) fn decode_from_args(
    args: arguments::DecodeDataRequest,
) -> Result<DataRequestOutput, Error> {
    if let Some(hex_string) = &args.hex {
        lib::data_requests::decode_from_hex_string(hex_string)
    } else if let Some(path_string) = &args.from_solidity {
        let path = Path::new(path_string);
        let pb_bytes = extract_pb_bytes_from_solidity_file(path)?;

        lib::data_requests::decode_from_pb_bytes(&pb_bytes)
    } else {
        Err(Error::DataRequestNoBytes)
    }
}

/// Try a data request from a `DecodeDataRequest` structure.
///
/// By default, a full trace is provided, i.e.: all execution details including the partial results
/// after each operator.
///
/// Full trace mode can be disabled by setting `--full_trace` option to `false`.  
pub(crate) fn try_from_args(
    args: arguments::TryDataRequest,
) -> Result<RADRequestExecutionReport, Error> {
    let full_trace = args.full_trace.unwrap_or(true);
    let request = decode_from_args(args.into())?.data_request;

    lib::data_requests::try_data_request(&request, full_trace)
}

/// Extract the Protocol Buffers representation of a data request from a Solidity smart contract
/// that is an instance of the `WitnetRequest.sol` contract, or at least implements the same
/// interface.
fn extract_pb_bytes_from_solidity_file(path: &Path) -> Result<Vec<u8>, Error> {
    let mut file = File::open(path).map_err(Error::SolidityFileCantOpen)?;
    let mut contents = String::new();
    file.read_to_string(&mut contents)
        .map_err(Error::SolidityFileCantRead)?;

    let hex_reg_ex: Regex = Regex::new(r#"\s*constructor.*Request\s*\(\s*hex"([\da-f]+)"#)?;

    let hex_string = hex_reg_ex
        .captures(&contents)
        .and_then(|captures| captures.get(1))
        .ok_or_else(Error::SolidityFileNoHexMatch)?
        .as_str();

    hex::decode(hex_string).map_err(Error::DataRequestHexNotValid)
}
