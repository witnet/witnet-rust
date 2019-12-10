use std::{
    convert::{TryFrom, TryInto},
    fmt,
    str::FromStr,
};

use serde::{Deserialize, Serialize};
use serde_cbor::value::Value;

use crate::{
    error::RadError,
    operators::{float as float_operators, identity, Operable, RadonOpCodes},
    script::RadonCall,
    types::{RadonType, RadonTypes},
};
use witnet_data_structures::radon_report::ReportContext;

pub const RADON_FLOAT_TYPE_NAME: &str = "RadonFloat";

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, Default)]
pub struct RadonFloat {
    value: f64,
}

impl RadonType<f64> for RadonFloat {
    fn value(&self) -> f64 {
        self.value
    }

    fn radon_type_name() -> String {
        RADON_FLOAT_TYPE_NAME.to_string()
    }
}

impl TryFrom<Value> for RadonFloat {
    type Error = RadError;

    fn try_from(value: Value) -> Result<Self, Self::Error> {
        let error = || RadError::Decode {
            from: "cbor::value::Value".to_string(),
            to: RADON_FLOAT_TYPE_NAME.to_string(),
        };

        match value {
            Value::Float(f64_value) => Ok(Self::from(f64_value)),
            Value::Integer(i128_value) => Ok(Self::from(i128_value as f64)),
            Value::Text(string_value) => Self::try_from(string_value.as_str()),
            _ => Err(error()),
        }
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

impl TryFrom<&str> for RadonFloat {
    type Error = RadError;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        f64::from_str(value).map(Into::into).map_err(Into::into)
    }
}

impl<'a> Operable for RadonFloat {
    fn operate(&self, call: &RadonCall) -> Result<RadonTypes, RadError> {
        match call {
            // Identity
            (RadonOpCodes::Identity, None) => identity(RadonTypes::from(self.clone())),
            (RadonOpCodes::FloatAbsolute, None) => {
                Ok(RadonTypes::from(float_operators::absolute(self)))
            }
            (RadonOpCodes::FloatAsMixed, None) => {
                Ok(RadonTypes::from(float_operators::to_mixed(self.clone())))
            }
            (RadonOpCodes::FloatAsString, None) => float_operators::to_string(self.clone())
                .map(RadonTypes::from)
                .map_err(Into::into),
            (RadonOpCodes::FloatCeiling, None) => {
                Ok(RadonTypes::from(float_operators::ceiling(self)))
            }
            (RadonOpCodes::FloatGreaterThan, Some(args)) => {
                float_operators::greater_than(self, args).map(Into::into)
            }
            (RadonOpCodes::FloatLessThan, Some(args)) => {
                float_operators::less_than(self, args).map(Into::into)
            }
            (RadonOpCodes::FloatMultiply, Some(args)) => {
                float_operators::multiply(self, args.as_slice()).map(Into::into)
            }
            (RadonOpCodes::FloatModulo, Some(args)) => {
                float_operators::modulo(self, args.as_slice()).map(Into::into)
            }
            (RadonOpCodes::FloatFloor, None) => Ok(RadonTypes::from(float_operators::floor(self))),

            (RadonOpCodes::FloatNegate, None) => {
                Ok(RadonTypes::from(float_operators::negate(self)))
            }
            (RadonOpCodes::FloatPower, Some(args)) => {
                float_operators::power(self, args.as_slice()).map(Into::into)
            }
            (RadonOpCodes::FloatRound, None) => Ok(RadonTypes::from(float_operators::round(self))),
            (RadonOpCodes::FloatTruncate, None) => {
                Ok(RadonTypes::from(float_operators::truncate(self)))
            }
            // Unsupported / unimplemented
            (op_code, args) => Err(RadError::UnsupportedOperator {
                input_type: RADON_FLOAT_TYPE_NAME.to_string(),
                operator: op_code.to_string(),
                args: args.to_owned(),
            }),
        }
    }

    fn operate_in_context(
        &self,
        call: &RadonCall,
        _context: &mut ReportContext,
    ) -> Result<RadonTypes, RadError> {
        self.operate(call)
    }
}

impl fmt::Display for RadonFloat {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}({})", RADON_FLOAT_TYPE_NAME, self.value)
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
    let input: &[u8] = &[251, 64, 9, 33, 251, 84, 68, 45, 24]; // 3.141592653589793

    let expected = RadonTypes::from(RadonFloat::from(std::f64::consts::PI));
    let result = RadonTypes::try_from(input).unwrap();

    assert_eq!(result, expected);
}
