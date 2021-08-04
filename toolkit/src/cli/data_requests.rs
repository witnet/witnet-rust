//! Implementations of CLI methods related to Witnet data requests.

use witnet_data_structures::chain::DataRequestOutput;
use witnet_rad::RADRequestExecutionReport;

use crate::{errors::Error, lib};

use super::arguments;

/// Decode a data request from a `DecodeDataRequest` structure.
pub(crate) fn decode_from_args(
    args: arguments::DecodeDataRequest,
) -> Result<DataRequestOutput, Error> {
    if let Some(hex) = args.hex {
        lib::data_requests::decode_from_hex(hex)
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
    let request = decode_from_args(arguments::DecodeDataRequest { hex: args.hex })?.data_request;
    let full_trace = args.full_trace.unwrap_or(true);

    lib::data_requests::try_data_request(&request, full_trace)
}
