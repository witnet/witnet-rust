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

    decode_dro_from_pb_bytes(&pb_bytes).or(decode_rad_from_pb_bytes(&pb_bytes))
}

/// Decode a data request output from its Protocol Buffers bytecode.
pub fn decode_dro_from_pb_bytes(pb_bytes: &[u8]) -> Result<DataRequestOutput, Error> {
    let request =
        DataRequestOutput::from_pb_bytes(pb_bytes).map_err(Error::DataRequestProtoBufNotValid)?;

    Ok(request)
}

/// Decode a RAD request from its Protocol Buffers bytecode.
pub fn decode_rad_from_pb_bytes(pb_bytes: &[u8]) -> Result<DataRequestOutput, Error> {
    let rad = RADRequest::from_pb_bytes(pb_bytes).map_err(Error::DataRequestProtoBufNotValid)?;
    let request = DataRequestOutput {
        data_request: rad,
        witness_reward: 0,
        witnesses: 0,
        commit_and_reveal_fee: 0,
        min_consensus_percentage: 0,
        collateral: 0,
    };

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decode_dro_from_hex_string() {
        let hex = "0aab0412520801123268747470733a2f2f6170692e62696e616e63652e55532f6170692f76332f7469636b65723f73796d626f6c3d4554485553441a1a841877821864696c61737450726963658218571a000f4240185b124d0801122c68747470733a2f2f6170692e62697466696e65782e636f6d2f76312f7075627469636b65722f4554485553441a1b8418778218646a6c6173745f70726963658218571a000f4240185b12480801122d68747470733a2f2f7777772e6269747374616d702e6e65742f6170692f76322f7469636b65722f6574687573641a15841877821864646c6173748218571a000f4240185b12550801123168747470733a2f2f6170692e626974747265782e636f6d2f76332f6d61726b6574732f4554482d5553442f7469636b65721a1e8418778218646d6c6173745472616465526174658218571a000f4240185b12620801123768747470733a2f2f6170692e636f696e626173652e636f6d2f76322f65786368616e67652d72617465733f63757272656e63793d4554481a258618778218666464617461821866657261746573821864635553448218571a000f4240185b12630801123268747470733a2f2f6170692e6b72616b656e2e636f6d2f302f7075626c69632f5469636b65723f706169723d4554485553441a2b87187782186666726573756c7482186668584554485a55534482186161618216008218571a000f4240185b1a0d0a0908051205fa3fc000001003220d0a0908051205fa4020000010031080a3c347180a2080ade20428333080acc7f037";
        let request = decode_from_hex_string(hex);

        assert!(request.is_ok())
    }

    #[test]
    fn decode_rad_from_hex_string() {
        let hex = "0aab0412520801123268747470733a2f2f6170692e62696e616e63652e55532f6170692f76332f7469636b65723f73796d626f6c3d4554485553441a1a841877821864696c61737450726963658218571a000f4240185b124d0801122c68747470733a2f2f6170692e62697466696e65782e636f6d2f76312f7075627469636b65722f4554485553441a1b8418778218646a6c6173745f70726963658218571a000f4240185b12480801122d68747470733a2f2f7777772e6269747374616d702e6e65742f6170692f76322f7469636b65722f6574687573641a15841877821864646c6173748218571a000f4240185b12550801123168747470733a2f2f6170692e626974747265782e636f6d2f76332f6d61726b6574732f4554482d5553442f7469636b65721a1e8418778218646d6c6173745472616465526174658218571a000f4240185b12620801123768747470733a2f2f6170692e636f696e626173652e636f6d2f76322f65786368616e67652d72617465733f63757272656e63793d4554481a258618778218666464617461821866657261746573821864635553448218571a000f4240185b12630801123268747470733a2f2f6170692e6b72616b656e2e636f6d2f302f7075626c69632f5469636b65723f706169723d4554485553441a2b87187782186666726573756c7482186668584554485a55534482186161618216008218571a000f4240185b1a0d0a0908051205fa3fc000001003220d0a0908051205fa402000001003";
        let request = decode_from_hex_string(hex);

        assert!(request.is_ok())
    }
}
