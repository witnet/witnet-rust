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

        // Try to read file contents
        let mut file = File::open(path).map_err(Error::SolidityFileCantOpen)?;
        let mut contents = String::new();
        file.read_to_string(&mut contents)
            .map_err(Error::SolidityFileCantRead)?;

        let pb_bytes = extract_pb_bytes_from_solidity(&contents)?;
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
fn extract_pb_bytes_from_solidity(contents: &str) -> Result<Vec<u8>, Error> {
    // Regex to capture old and new Witnet request syntax
    let hex_reg_ex: Regex = Regex::new(
        r#"\s*(constructor|WitnetRequestInitializableBase).*(Request|initialize)\s*\(\s*hex"(?P<bytes>[\da-f]+)"#,
    )?;

    let hex_string = hex_reg_ex
        .captures(contents)
        .and_then(|captures| captures.name("bytes"))
        .ok_or_else(Error::SolidityFileNoHexMatch)?
        .as_str();

    hex::decode(hex_string).map_err(Error::DataRequestHexNotValid)
}

#[test]
fn test_extract_pb_bytes_from_constructor() {
    let contents = r#"
pragma solidity >=0.7.0 <0.9.0;

import "witnet-ethereum-bridge/contracts/requests/WitnetRequest.sol";

// The bytecode of the BitcoinPrice request that will be sent to Witnet
contract BitcoinPriceRequest is WitnetRequest {
  constructor () WitnetRequest(hex"0abf0108f3b5988906123b122468747470733a2f2f7777772e6269747374616d702e6e65742f6170692f7469636b65722f1a13841877821864646c6173748218571903e8185b125c123168747470733a2f2f6170692e636f696e6465736b2e636f6d2f76312f6270692f63757272656e7470726963652e6a736f6e1a2786187782186663627069821866635553448218646a726174655f666c6f61748218571903e8185b1a0d0a0908051205fa3fc000001003220d0a0908051205fa3fc000001003100a186420012846308094ebdc03") { }
}
"#;

    let result = extract_pb_bytes_from_solidity(contents).unwrap();
    assert_eq!(result,
            hex::decode("0abf0108f3b5988906123b122468747470733a2f2f7777772e6269747374616d702e6e65742f6170692f7469636b65722f1a13841877821864646c6173748218571903e8185b125c123168747470733a2f2f6170692e636f696e6465736b2e636f6d2f76312f6270692f63757272656e7470726963652e6a736f6e1a2786187782186663627069821866635553448218646a726174655f666c6f61748218571903e8185b1a0d0a0908051205fa3fc000001003220d0a0908051205fa3fc000001003100a186420012846308094ebdc03").unwrap()
    );
}

#[test]
fn test_extract_pb_bytes_from_initialize() {
    let contents = r#"
pragma solidity >=0.7.0 <0.9.0;

import "witnet-ethereum-bridge/contracts/requests/WitnetRequestInitializableBase.sol";

// The bytecode of the BitcoinPrice request that will be sent to Witnet
contract BitcoinPriceRequest is WitnetRequestInitializableBase {
  function initialize() public {
    WitnetRequestInitializableBase.initialize(hex"0abf0108f3b5988906123b122468747470733a2f2f7777772e6269747374616d702e6e65742f6170692f7469636b65722f1a13841877821864646c6173748218571903e8185b125c123168747470733a2f2f6170692e636f696e6465736b2e636f6d2f76312f6270692f63757272656e7470726963652e6a736f6e1a2786187782186663627069821866635553448218646a726174655f666c6f61748218571903e8185b1a0d0a0908051205fa3fc000001003220d0a0908051205fa3fc000001003100a186420012846308094ebdc03");
  }
}
"#;

    let result = extract_pb_bytes_from_solidity(contents).unwrap();
    assert_eq!(result,
               hex::decode("0abf0108f3b5988906123b122468747470733a2f2f7777772e6269747374616d702e6e65742f6170692f7469636b65722f1a13841877821864646c6173748218571903e8185b125c123168747470733a2f2f6170692e636f696e6465736b2e636f6d2f76312f6270692f63757272656e7470726963652e6a736f6e1a2786187782186663627069821866635553448218646a726174655f666c6f61748218571903e8185b1a0d0a0908051205fa3fc000001003220d0a0908051205fa3fc000001003100a186420012846308094ebdc03").unwrap()
    );
}
