use crate::{
    error::RadError,
    operators::{Operable, RadonOpCodes, bytes as bytes_operators, identity},
    script::RadonCall,
    types::{RadonType, RadonTypes},
};
use num_enum::TryFromPrimitive;
use serde::Serialize;
use serde_cbor::value::Value;
use std::{
    convert::{TryFrom, TryInto},
    fmt,
};
use witnet_data_structures::{proto::versioning::ProtocolVersion::*, radon_report::ReportContext};

const RADON_BYTES_TYPE_NAME: &str = "RadonBytes";

/// List of support string-encoding algorithms for buffers
#[derive(Debug, Default, PartialEq, Eq, Serialize, TryFromPrimitive)]
#[repr(u8)]
pub enum RadonBytesEncoding {
    #[default]
    Hex = 0x00,
    Base58 = 0x10,
    Base64 = 0x11,
    Utf8 = 0x80,
}

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
        write!(f, "{RADON_BYTES_TYPE_NAME}({hex_value:?})")
    }
}

impl Operable for RadonBytes {
    fn operate(&self, call: &RadonCall) -> Result<RadonTypes, RadError> {
        self.operate_in_context(call, &mut ReportContext::default())
    }

    fn operate_in_context(
        &self,
        call: &RadonCall,
        context: &mut ReportContext<RadonTypes>,
    ) -> Result<RadonTypes, RadError> {
        let protocol_version = context.protocol_version.unwrap_or_default();

        match call {
            // Identity
            (RadonOpCodes::Identity, None) => identity(RadonTypes::from(self.clone())),
            (RadonOpCodes::BytesToInteger, args) if protocol_version >= V2_1 => {
                bytes_operators::to_integer(self, args).map(RadonTypes::from)
            }
            (RadonOpCodes::BytesLength, None) if protocol_version >= V2_1 => {
                Ok(RadonTypes::from(bytes_operators::length(self)))
            }
            (RadonOpCodes::BytesHash, Some(args)) => {
                bytes_operators::hash(self, args.as_slice()).map(RadonTypes::from)
            }
            (RadonOpCodes::BytesSlice, Some(args)) if protocol_version >= V2_1 => {
                bytes_operators::slice(self, args.as_slice()).map(RadonTypes::from)
            }
            (RadonOpCodes::BytesToString, args) => {
                if protocol_version >= V2_1 {
                    bytes_operators::to_string(self, args).map(RadonTypes::from)
                } else {
                    bytes_operators::to_string_legacy(self).map(RadonTypes::from)
                }
            }

            // Unsupported / unimplemented
            (op_code, args) => Err(RadError::UnsupportedOperator {
                input_type: RADON_BYTES_TYPE_NAME.to_string(),
                operator: op_code.to_string(),
                args: args.to_owned(),
            }),
        }
    }
}

/// List of supported string encoding algorithms for byte buffers
#[derive(Debug, Default, PartialEq, Eq, Serialize, TryFromPrimitive)]
#[repr(u8)]
pub enum RadonBytesEncoding {
    /// Hexadecimal
    #[default]
    Hex = 0,
    /// Base64
    Base64 = 1,
    /// UTF-8
    Utf8 = 2,
}

/// A simple, convenient and unified marker for endianness.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq)]
pub enum RadonBytesEndianness {
    /// Big endian
    #[default]
    Big,
    /// Little endian
    Little,
}

impl From<u8> for RadonBytesEndianness {
    fn from(value: u8) -> Self {
        if value == 1 {
            RadonBytesEndianness::Little
        } else {
            RadonBytesEndianness::Big
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_endianness_from_u8() {
        assert_eq!(RadonBytesEndianness::from(0), RadonBytesEndianness::Big);
        assert_eq!(RadonBytesEndianness::from(1), RadonBytesEndianness::Little);
        assert_eq!(RadonBytesEndianness::from(2), RadonBytesEndianness::Big);
        assert_eq!(
            RadonBytesEndianness::from(u8::MAX),
            RadonBytesEndianness::Big
        );
    }
}
