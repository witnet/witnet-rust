use crate::error::*;
use crate::operators::{identity, Operable, RadonOpCodes};
use crate::script::RadonCall;
use crate::types::{RadonType, RadonTypes};

use rmpv::{decode, encode, Value};
use std::{fmt, io::Cursor};
use witnet_data_structures::serializers::decoders::{TryFrom, TryInto};

#[derive(Clone, Debug, PartialEq)]
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

impl<'a> TryFrom<Value> for RadonMixed {
    type Error = RadError;

    fn try_from(value: Value) -> Result<Self, Self::Error> {
        Ok(Self::from(value))
    }
}

impl<'a> TryInto<Value> for RadonMixed {
    type Error = RadError;

    fn try_into(self) -> Result<Value, Self::Error> {
        Ok(self.value())
    }
}

impl<'a> TryFrom<&'a [u8]> for RadonMixed {
    type Error = RadError;

    fn try_from(vector: &'a [u8]) -> Result<Self, Self::Error> {
        let mut cursor = Cursor::new(vector);
        let result = decode::read_value(&mut cursor);

        match result {
            Ok(value) => Ok(Self::from(value)),
            Err(_) => Err(RadError::new(
                RadErrorKind::EncodeDecode,
                String::from("Failed to encode a RadonMixed from bytes"),
            )),
        }
    }
}

impl<'a> TryInto<Vec<u8>> for RadonMixed {
    type Error = RadError;

    fn try_into(self) -> Result<Vec<u8>, Self::Error> {
        let mut cursor = Cursor::new(Vec::new());
        let result = encode::write_value(&mut cursor, &self.value);
        let vector = cursor.into_inner();

        match result {
            Ok(()) => Ok(vector),
            Err(_) => Err(RadError::new(
                RadErrorKind::EncodeDecode,
                String::from("Failed to decode a RadonMixed from bytes"),
            )),
        }
    }
}

impl Operable for RadonMixed {
    fn operate(self, call: &RadonCall) -> RadResult<RadonTypes> {
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
