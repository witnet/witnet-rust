//! Functions providing convenient utilities for working with Witnet data requests.
use witnet_data_structures::{
    chain::{DataRequestOutput, RADRequest},
    proto::ProtobufConvert,
};
use witnet_rad::{script::RadonScriptExecutionSettings, RADRequestExecutionReport};

use crate::errors::Error;

/// Decode a data request from its Protocol Buffers hexadecimal string representation.
pub fn decode_from_hex_string(hex: &str) -> Result<DataRequestOutput, Error> {
    let pb_bytes = hex::decode(hex).map_err(Error::DataRequestHexNotValid)?;

    decode_from_pb_bytes(&pb_bytes)
}

/// Decode a data request from its Protocol Buffers bytecode.
pub fn decode_from_pb_bytes(pb_bytes: &[u8]) -> Result<DataRequestOutput, Error> {
    let request =
        DataRequestOutput::from_pb_bytes(pb_bytes).map_err(Error::DataRequestProtoBufNotValid)?;

    Ok(request)
}

/// Locally try a data request.
///
/// By default, a full trace is provided, i.e.: all execution details including the partial results
/// after each operator.
///
/// Full trace mode can be disabled by setting `full_trace` to `false`.
pub fn try_data_request(
    request: &RADRequest,
    full_trace: bool,
) -> Result<RADRequestExecutionReport, Error> {
    let settings = if full_trace {
        RadonScriptExecutionSettings::enable_all()
    } else {
        RadonScriptExecutionSettings::disable_all()
    };
    let report = witnet_rad::try_data_request(request, settings, None, None);

    Ok(report)
}
