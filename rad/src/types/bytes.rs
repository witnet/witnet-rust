use crate::{
    error::RadError,
    operators::{bytes as bytes_operators, identity, Operable, RadonOpCodes},
    script::RadonCall,
    types::{RadonType, RadonTypes},
};
use serde_cbor::value::Value;
use std::{
    convert::{TryFrom, TryInto},
    fmt,
};
use witnet_data_structures::radon_report::ReportContext;

const RADON_BYTES_TYPE_NAME: &str = "RadonBytes";

#[derive(Clone, Debug, Default, Eq, Ord, PartialEq, PartialOrd)]
pub struct RadonBytes {
    value: Vec<u8>,
}

impl RadonType<Vec<u8>> for RadonBytes {
    fn value(&self) -> Vec<u8> {
        self.value.clone()
    }

    #[inline]
    fn radon_type_name() -> &'static str {
        RADON_BYTES_TYPE_NAME
    }
}

impl TryFrom<Value> for RadonBytes {
    type Error = RadError;

    fn try_from(value: Value) -> Result<Self, Self::Error> {
        let error = || RadError::Decode {
            from: "cbor::value::Value",
            to: RadonBytes::radon_type_name(),
        };

        match value {
            Value::Bytes(bytes_value) => Ok(Self::from(bytes_value)),
            _ => Err(error()),
        }
    }
}

impl TryFrom<RadonTypes> for RadonBytes {
    type Error = RadError;

    fn try_from(item: RadonTypes) -> Result<Self, Self::Error> {
        if let RadonTypes::Bytes(rad_bytes) = item {
            Ok(rad_bytes)
        } else {
            let value = Value::try_from(item)?;
            value.try_into()
        }
    }
}

impl TryInto<Value> for RadonBytes {
    type Error = RadError;

    fn try_into(self) -> Result<Value, Self::Error> {
        Ok(Value::from(self.value()))
    }
}

impl From<Vec<u8>> for RadonBytes {
    fn from(value: Vec<u8>) -> Self {
        RadonBytes { value }
    }
}

impl fmt::Display for RadonBytes {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let hex_value = hex::encode(&self.value);
        write!(f, "{}({:?})", RADON_BYTES_TYPE_NAME, hex_value)
    }
}

impl Operable for RadonBytes {
    fn operate(&self, call: &RadonCall) -> Result<RadonTypes, RadError> {
        match call {
            // Identity
            (RadonOpCodes::Identity, None) => identity(RadonTypes::from(self.clone())),
            (RadonOpCodes::BytesAsString, None) => bytes_operators::to_string(self)
                .map(RadonTypes::from)
                .map_err(Into::into),
            (RadonOpCodes::BytesHash, Some(args)) => bytes_operators::hash(self, args.as_slice())
                .map(RadonTypes::from)
                .map_err(Into::into),
            // Unsupported / unimplemented
            (op_code, args) => Err(RadError::UnsupportedOperator {
                input_type: RADON_BYTES_TYPE_NAME.to_string(),
                operator: op_code.to_string(),
                args: args.to_owned(),
            }),
        }
    }

    fn operate_in_context(
        &self,
        call: &RadonCall,
        _context: &mut ReportContext<RadonTypes>,
    ) -> Result<RadonTypes, RadError> {
        self.operate(call)
    }
}
