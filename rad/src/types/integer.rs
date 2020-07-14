use std::{
    convert::{TryFrom, TryInto},
    fmt,
    str::FromStr,
};

use serde_cbor::value::Value;

use crate::{
    error::RadError,
    operators::{identity, integer as integer_operators, Operable, RadonOpCodes},
    script::RadonCall,
    types::{RadonType, RadonTypes},
};
use witnet_data_structures::radon_report::ReportContext;

const RADON_INTEGER_TYPE_NAME: &str = "RadonInteger";

#[derive(Clone, Debug, Default, Eq, Ord, PartialEq, PartialOrd)]
pub struct RadonInteger {
    value: i128,
}

impl RadonType<i128> for RadonInteger {
    fn value(&self) -> i128 {
        self.value
    }

    #[inline]
    fn radon_type_name() -> &'static str {
        RADON_INTEGER_TYPE_NAME
    }
}

impl TryFrom<Value> for RadonInteger {
    type Error = RadError;

    fn try_from(value: Value) -> Result<Self, Self::Error> {
        let error = || RadError::Decode {
            from: "cbor::value::Value",
            to: RadonInteger::radon_type_name(),
        };

        match value {
            Value::Integer(i128_value) => Ok(Self::from(i128_value)),
            Value::Text(string_value) => Self::try_from(string_value.as_str()),
            _ => Err(error()),
        }
    }
}

impl TryFrom<RadonTypes> for RadonInteger {
    type Error = RadError;

    fn try_from(item: RadonTypes) -> Result<Self, Self::Error> {
        if let RadonTypes::Integer(rad_integer) = item {
            Ok(rad_integer)
        } else {
            let value = Value::try_from(item)?;
            value.try_into()
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
    fn operate(&self, call: &RadonCall) -> Result<RadonTypes, RadError> {
        match call {
            // Identity
            (RadonOpCodes::Identity, None) => identity(RadonTypes::from(self.clone())),
            (RadonOpCodes::IntegerAbsolute, None) => integer_operators::absolute(self)
                .map(RadonTypes::from)
                .map_err(Into::into),
            (RadonOpCodes::IntegerAsFloat, None) => integer_operators::to_float(self.clone())
                .map(RadonTypes::from)
                .map_err(Into::into),
            (RadonOpCodes::IntegerAsString, None) => integer_operators::to_string(self.clone())
                .map(RadonTypes::from)
                .map_err(Into::into),
            (RadonOpCodes::IntegerGreaterThan, Some(args)) => {
                integer_operators::greater_than(self, args).map(Into::into)
            }
            (RadonOpCodes::IntegerLessThan, Some(args)) => {
                integer_operators::less_than(self, args).map(Into::into)
            }
            (RadonOpCodes::IntegerModulo, Some(args)) => {
                integer_operators::modulo(self, args.as_slice()).map(Into::into)
            }
            (RadonOpCodes::IntegerMultiply, Some(args)) => {
                integer_operators::multiply(self, args.as_slice()).map(Into::into)
            }
            (RadonOpCodes::IntegerNegate, None) => integer_operators::negate(self)
                .map(RadonTypes::from)
                .map_err(Into::into),
            (RadonOpCodes::IntegerPower, Some(args)) => {
                integer_operators::power(self, args.as_slice()).map(Into::into)
            }
            // Unsupported / unimplemented
            (op_code, args) => Err(RadError::UnsupportedOperator {
                input_type: RADON_INTEGER_TYPE_NAME.to_string(),
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
