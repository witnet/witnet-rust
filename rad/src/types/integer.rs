use std::{
    convert::{TryFrom, TryInto},
    fmt,
    str::FromStr,
};

use serde::{Deserialize, Serialize};
use serde_cbor::value::Value;

use crate::{
    operators::{identity, integer as integer_operators, Operable, RadonOpCodes},
    rad_error::RadError,
    script::RadonCall,
    types::{RadonType, RadonTypes},
};

pub const RADON_INTEGER_TYPE_NAME: &str = "RadonInteger";

#[derive(Clone, Debug, PartialEq, PartialOrd, Ord, Eq, Serialize, Deserialize, Default)]
pub struct RadonInteger {
    value: i128,
}

impl RadonType<i128> for RadonInteger {
    fn value(&self) -> i128 {
        self.value
    }

    fn radon_type_name() -> String {
        RADON_INTEGER_TYPE_NAME.to_string()
    }
}

impl TryFrom<Value> for RadonInteger {
    type Error = RadError;

    fn try_from(value: Value) -> Result<Self, Self::Error> {
        let error = || RadError::Decode {
            from: "cbor::value::Value".to_string(),
            to: RADON_INTEGER_TYPE_NAME.to_string(),
        };

        match value {
            Value::Integer(i128_value) => Ok(Self::from(i128_value)),
            Value::Text(string_value) => Self::try_from(string_value.as_str()),
            _ => Err(error()),
        }
    }
}

impl TryInto<Value> for RadonInteger {
    type Error = RadError;

    fn try_into(self) -> Result<Value, Self::Error> {
        Ok(Value::Integer(self.value()))
    }
}

impl<'a> From<i128> for RadonInteger {
    fn from(value: i128) -> Self {
        RadonInteger { value }
    }
}

impl TryFrom<&str> for RadonInteger {
    type Error = RadError;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        i128::from_str(value).map(Into::into).map_err(Into::into)
    }
}

impl<'a> Operable for RadonInteger {
    fn operate(self, call: &RadonCall) -> Result<RadonTypes, RadError> {
        match call {
            // Identity
            (RadonOpCodes::Identity, None) => identity(RadonTypes::Integer(self)),
            (RadonOpCodes::IntegerAbsolute, None) => integer_operators::absolute(&self)
                .map(RadonTypes::from)
                .map_err(Into::into),
            (RadonOpCodes::IntegerAsBytes, None) => {
                Ok(RadonTypes::from(integer_operators::to_bytes(self)))
            }
            (RadonOpCodes::IntegerAsFloat, None) => integer_operators::to_float(self)
                .map(RadonTypes::from)
                .map_err(Into::into),
            (RadonOpCodes::IntegerAsString, None) => integer_operators::to_string(self)
                .map(RadonTypes::from)
                .map_err(Into::into),
            (RadonOpCodes::IntegerGreaterThan, Some(args)) => {
                integer_operators::greater_than(&self, args).map(Into::into)
            }
            (RadonOpCodes::IntegerLessThan, Some(args)) => {
                integer_operators::less_than(&self, args).map(Into::into)
            }
            (RadonOpCodes::IntegerModulo, Some(args)) => {
                integer_operators::modulo(&self, args.as_slice()).map(Into::into)
            }
            (RadonOpCodes::IntegerMultiply, Some(args)) => {
                integer_operators::multiply(&self, args.as_slice()).map(Into::into)
            }
            (RadonOpCodes::IntegerNegate, None) => integer_operators::negate(&self)
                .map(RadonTypes::from)
                .map_err(Into::into),
            (RadonOpCodes::IntegerPower, Some(args)) => {
                integer_operators::power(&self, args.as_slice()).map(Into::into)
            }
            // Unsupported / unimplemented
            (op_code, args) => Err(RadError::UnsupportedOperator {
                input_type: RADON_INTEGER_TYPE_NAME.to_string(),
                operator: op_code.to_string(),
                args: args.to_owned(),
            }),
        }
    }
}

impl fmt::Display for RadonInteger {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}({})", RADON_INTEGER_TYPE_NAME, self.value)
    }
}

#[test]
fn test_operate_unimplemented() {
    let input = RadonInteger::from(1);

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
    let input: &[u8] = &[27, 0, 11, 41, 67, 10, 37, 109, 33]; // 3141592653589793

    let expected = RadonTypes::from(RadonInteger::from(3_141_592_653_589_793));
    let result = RadonTypes::try_from(input).unwrap();

    assert_eq!(result, expected);
}
