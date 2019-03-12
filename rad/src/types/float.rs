use std::fmt;

use rmpv::Value;

use witnet_data_structures::serializers::decoders::{TryFrom, TryInto};

use crate::error::RadError;
use crate::operators::{identity, Operable, RadonOpCodes};
use crate::script::RadonCall;
use crate::types::{RadonType, RadonTypes};

#[derive(Clone, Debug, PartialEq)]
pub struct RadonFloat {
    value: f64,
}

impl RadonType<f64> for RadonFloat {
    fn value(&self) -> f64 {
        self.value
    }
}

impl TryFrom<Value> for RadonFloat {
    type Error = RadError;

    fn try_from(value: Value) -> Result<Self, Self::Error> {
        match value {
            Value::F64(f64_value) => Some(Self::from(f64_value)),
            Value::F32(f32_value) => Some(Self::from(f64::from(f32_value))),
            Value::Integer(integer_value) => integer_value.as_f64().map(Self::from),
            _ => None,
        }
        .ok_or_else(|| RadError::Decode {
            from: "rmpv::Value",
            to: "RadonFloat",
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
    fn operate(self, call: &RadonCall) -> Result<RadonTypes, RadError> {
        match call {
            // Identity
            (RadonOpCodes::Identity, None) => identity(RadonTypes::Float(self)),
            // Unsupported / unimplemented
            (op_code, args) => Err(RadError::UnsupportedOperator {
                input_type: "RadonFloat".to_string(),
                operator: op_code.to_string(),
                args: args.to_owned(),
            }),
        }
    }
}

impl fmt::Display for RadonFloat {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "RadonFloat({})", self.value)
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
