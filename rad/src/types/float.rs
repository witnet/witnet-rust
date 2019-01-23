use crate::error::*;
use crate::operators::{identity, Operable, RadonOpCodes};
use crate::script::RadonCall;
use crate::types::{RadonMixed, RadonType, RadonTypes};

use rmpv::{decode, encode, Value};
use std::{fmt, io::Cursor};
use witnet_data_structures::serializers::decoders::{TryFrom, TryInto};

#[derive(Debug, PartialEq, Clone)]
pub struct RadonFloat {
    value: f64,
}

impl<'a> RadonType<'a, f64> for RadonFloat {
    fn value(&self) -> f64 {
        self.value
    }
}

impl<'a> TryFrom<Value> for RadonFloat {
    type Error = RadError;

    fn try_from(value: Value) -> Result<Self, Self::Error> {
        value.as_f64().map(Self::from).ok_or_else(|| {
            RadError::new(
                RadErrorKind::EncodeDecode,
                String::from("Error creating a RadonFloat from MessagePack value"),
            )
        })
    }
}

impl TryInto<Value> for RadonFloat {
    type Error = RadError;

    fn try_into(self) -> Result<Value, Self::Error> {
        Ok(Value::from(self.value()))
    }
}

impl<'a> From<f64> for RadonFloat {
    fn from(value: f64) -> Self {
        RadonFloat { value }
    }
}

impl<'a> TryFrom<&'a [u8]> for RadonFloat {
    type Error = RadError;

    fn try_from(vector: &'a [u8]) -> Result<Self, Self::Error> {
        let mixed = RadonMixed::try_from(vector)?;
        let value: Value = RadonMixed::try_into(mixed)?;

        Self::try_from(value)
    }
}

impl<'a> TryInto<Vec<u8>> for RadonFloat {
    type Error = RadError;

    fn try_into(self) -> Result<Vec<u8>, Self::Error> {
        let value: Value = Self::try_into(self)?;
        let mixed = RadonMixed::try_from(value)?;

        RadonMixed::try_into(mixed)
    }
}

impl<'a> Operable for RadonFloat {
    fn operate(self, call: &RadonCall) -> RadResult<RadonTypes> {
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

#[test]
fn test_operate_unimplemented() {
    let input = RadonFloat::from(3.141592);

    let call = (RadonOpCodes::Fail, None);
    let result = input.operate(&call);

    assert!(if let Err(_error) = result {
        true
    } else {
        false
    });
}

#[test]
fn test_from_vector() {
    let input: &[u8] = &[203, 64, 9, 33, 250, 252, 139, 0, 122]; // 3.141592

    let expected = RadonFloat::from(3.141592);
    let expected_wrong = RadonFloat::from(3.141593);
    let result = RadonFloat::try_from(input);
    let wronw_result = RadonFloat::try_from(input);

    assert_eq!(expected, result.unwrap());
    assert_ne!(expected_wrong, wronw_result.unwrap());
}
