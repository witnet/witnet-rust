use std::fmt;

use rmpv::Value;

use witnet_data_structures::serializers::decoders::{TryFrom, TryInto};

use crate::error::*;
use crate::operators::{identity, Operable, RadonOpCodes};
use crate::script::RadonCall;
use crate::types::{RadonType, RadonTypes};

#[derive(Clone, Debug, PartialEq)]
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

impl<'a> Operable for RadonFloat {
    fn operate(self, call: &RadonCall) -> RadResult<RadonTypes> {
        match call {
            // Identity
            (RadonOpCodes::Identity, None) => identity(RadonTypes::Float(self)),
            // Unsupported / unimplemented
            (op_code, args) => Err(WitnetError::from(RadError::new(
                RadErrorKind::UnsupportedOperator,
                format!(
                    "Call to {:?} with args {:?} is not supported on type RadonFloat",
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
    let input = RadonFloat::from(std::f64::consts::PI);

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
    let input: &[u8] = &[203, 64, 9, 33, 251, 84, 68, 45, 24]; // 3.141592653589793

    let expected = RadonFloat::from(std::f64::consts::PI);
    let expected_wrong = RadonFloat::from(std::f64::consts::PI + 1f64);
    let result = RadonFloat::decode(input);
    let wrong_result = RadonFloat::decode(input);

    assert_eq!(expected, result.unwrap());
    assert_ne!(expected_wrong, wrong_result.unwrap());
}
