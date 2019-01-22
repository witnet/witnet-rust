use crate::error::*;
use crate::operators::{identity, Operable, RadonOpCodes};
use crate::script::RadonCall;
use crate::types::{RadonType, RadonTypes};

use rmpv::{decode, encode, Value};
use std::{fmt, io::Cursor};
use witnet_data_structures::serializers::decoders::{TryFrom, TryInto};

#[derive(Debug, PartialEq)]
pub struct RadonFloat {
    value: f64,
}

impl<'a> RadonType<'a, f64> for RadonFloat {
    fn value(&self) -> f64 {
        self.value
    }
}

impl<'a> From<f64> for RadonFloat {
    fn from(value: f64) -> Self {
        RadonFloat { value }
    }
}

impl<'a> TryFrom<&'a [u8]> for RadonFloat {
    type Error = RadError;

    fn try_from(slice: &'a [u8]) -> Result<Self, Self::Error> {
        let mut cursor = Cursor::new(slice);
        let buffer = cursor.get_mut();
        let result = decode::read_value(buffer);

        match result {
            Ok(value) => Ok(RadonFloat {
                value: value.as_f64().unwrap(),
            }),
            Err(_) => Err(RadError::new(
                RadErrorKind::EncodeDecode,
                String::from("Failed to encode a RadonFloat from bytes"),
            )),
        }
    }
}

impl<'a> TryInto<&'a [u8]> for RadonFloat {
    type Error = RadError;

    fn try_into(self) -> Result<&'a [u8], Self::Error> {
        let mut buffer: &mut [u8] = &mut [];
        let result = encode::write_value(&mut buffer, &Value::F64(self.value));

        match result {
            Ok(()) => Ok(buffer),
            Err(_) => Err(RadError::new(
                RadErrorKind::EncodeDecode,
                String::from("Failed to decode a RadonFloat from bytes"),
            )),
        }
    }
}

impl<'a> Operable<'a> for RadonFloat {
    fn operate(self, call: &RadonCall) -> RadResult<RadonTypes<'a>> {
        match call {
            // Identity
            (RadonOpCodes::Identity, None) => identity(RadonTypes::Float(self)),
            // Unsupported / unimplemented  
            (op_code, args) => Err(WitnetError::from(RadError::new(
                RadErrorKind::UnsupportedOperator,
                format!(
                    "Call to {:?} with args {:?} is not supported on type RadonString",
                    op_code, args
                ),
            ))),
        }
    }
}

impl fmt::Display for RadonFloat {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "RadonFloat")
    }
}
