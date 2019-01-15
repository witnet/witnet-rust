use crate::error::*;
use crate::operators::{identity, Operable, RadonOpCodes};
use crate::script::RadonCall;
use crate::types::{RadonType, RadonTypes};

use rmpv::{decode, encode, Value};
use std::{fmt, io::Cursor};
use witnet_data_structures::serializers::decoders::{TryFrom, TryInto};

#[derive(Debug, PartialEq)]
pub struct RadonMixed {
    value: Value,
}

impl<'a> RadonType<'a, Value> for RadonMixed {
    fn value(&self) -> Value {
        self.value.clone()
    }
}

impl From<Value> for RadonMixed {
    fn from(value: Value) -> Self {
        RadonMixed { value }
    }
}

impl<'a> TryFrom<&'a [u8]> for RadonMixed {
    type Error = RadError;

    fn try_from(slice: &'a [u8]) -> Result<Self, Self::Error> {
        let mut cursor = Cursor::new(slice);
        let buffer = cursor.get_mut();
        let result = decode::read_value(buffer);

        match result {
            Ok(value) => Ok(Self::from(value)),
            Err(_) => Err(RadError::new(
                RadErrorKind::EncodeDecode,
                String::from("Failed to encode a RadonMixed from bytes"),
            )),
        }
    }
}

impl<'a> TryInto<&'a [u8]> for RadonMixed {
    type Error = RadError;

    fn try_into(self) -> Result<&'a [u8], Self::Error> {
        let mut buffer: &mut [u8] = &mut [];
        let result = encode::write_value(&mut buffer, &self.value);

        match result {
            Ok(()) => Ok(buffer),
            Err(_) => Err(RadError::new(
                RadErrorKind::EncodeDecode,
                String::from("Failed to decode a RadonMixed from bytes"),
            )),
        }
    }
}

impl<'a> Operable<'a> for RadonMixed {
    fn operate(self, call: &RadonCall) -> RadResult<RadonTypes<'a>> {
        match call {
            // Identity
            (RadonOpCodes::Identity, None) => identity(RadonTypes::Mixed(self)),
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

impl fmt::Display for RadonMixed {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "RadonMixed")
    }
}

#[test]
fn test_operate_identity() {
    let value = rmpv::Value::from(0);
    let input = RadonMixed::from(value.clone());
    let expected = RadonTypes::Mixed(RadonMixed::from(value));

    let call = (RadonOpCodes::Identity, None);
    let output = input.operate(&call).unwrap();

    assert_eq!(output, expected);
}

#[test]
fn test_operate_unimplemented() {
    let input = RadonMixed::from(rmpv::Value::from(0));

    let call = (RadonOpCodes::Fail, None);
    let result = input.operate(&call);

    assert!(if let Err(_error) = result {
        true
    } else {
        false
    });
}
