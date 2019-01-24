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
        match value {
            Value::F64(f64_value) => Some(Self::from(f64_value)),
            Value::F32(f32_value) => Some(Self::from(f32_value as f64)),
            Value::Integer(integer_value) => integer_value.as_f64().map(Self::from),
            _ => None,
        }
        .ok_or(RadError::new(
            RadErrorKind::EncodeDecode,
            format!(
                "Error creating a RadonFloat from MessagePack value \"{:?}\"",
                value
            ),
        ))
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

    let expected = RadonTypes::from(RadonFloat::from(std::f64::consts::PI));
    let result = RadonTypes::try_from(input).unwrap();

    assert_eq!(result, expected);
}
