use std::{
    convert::{TryFrom, TryInto},
    fmt,
    str::FromStr,
};

use serde_cbor::value::Value;

use crate::{
    error::RadError,
    operators::{float as float_operators, identity, Operable, RadonOpCodes},
    script::RadonCall,
    types::{RadonType, RadonTypes},
};
use witnet_data_structures::radon_report::ReportContext;

const RADON_FLOAT_TYPE_NAME: &str = "RadonFloat";

#[derive(Clone, Debug, Default, PartialEq)]
pub struct RadonFloat {
    value: f64,
}

impl RadonType<f64> for RadonFloat {
    fn value(&self) -> f64 {
        self.value
    }

    #[inline]
    fn radon_type_name() -> &'static str {
        RADON_FLOAT_TYPE_NAME
    }
}

impl TryFrom<Value> for RadonFloat {
    type Error = RadError;

    // FIXME: Allow for now, since there is no safe cast function from an i128 to float yet
    #[allow(clippy::cast_precision_loss)]
    fn try_from(value: Value) -> Result<Self, Self::Error> {
        let error = || RadError::Decode {
            from: "cbor::value::Value",
            to: RadonFloat::radon_type_name(),
        };

        match value {
            Value::Float(f64_value) => Ok(Self::from(f64_value)),
            Value::Integer(i128_value) => Ok(Self::from(i128_value as f64)),
            Value::Text(string_value) => Self::try_from(string_value.as_str()),
            _ => Err(error()),
        }
    }
}

impl TryFrom<RadonTypes> for RadonFloat {
    type Error = RadError;

    fn try_from(item: RadonTypes) -> Result<Self, Self::Error> {
        if let RadonTypes::Float(rad_float) = item {
            Ok(rad_float)
        } else {
            let value = Value::try_from(item)?;
            value.try_into()
        }
    }
}

impl TryInto<Value> for RadonFloat {
    type Error = RadError;

    fn try_into(self) -> Result<Value, Self::Error> {
        Ok(Value::from(self.value()))
    }
}

impl From<f64> for RadonFloat {
    fn from(value: f64) -> Self {
        RadonFloat { value }
    }
}

impl From<i32> for RadonFloat {
    fn from(value: i32) -> Self {
        RadonFloat {
            value: f64::from(value),
        }
    }
}

impl TryFrom<&str> for RadonFloat {
    type Error = RadError;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        f64::from_str(value).map(Into::into).map_err(Into::into)
    }
}

impl Operable for RadonFloat {
    fn operate(&self, call: &RadonCall) -> Result<RadonTypes, RadError> {
        match call {
            // Identity
            (RadonOpCodes::Identity, None) => identity(RadonTypes::from(self.clone())),
            (RadonOpCodes::FloatAbsolute, None) => {
                Ok(RadonTypes::from(float_operators::absolute(self)))
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
        _context: &mut ReportContext<RadonTypes>,
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

    assert!(result.is_err());
}

#[test]
fn test_from_vector() {
    let input: &[u8] = &[251, 64, 9, 33, 251, 84, 68, 45, 24]; // 3.141592653589793

    let expected = RadonTypes::from(RadonFloat::from(std::f64::consts::PI));
    let result = RadonTypes::try_from(input).unwrap();

    assert_eq!(result, expected);
}
