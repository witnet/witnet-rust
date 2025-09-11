use std::{
    convert::{TryFrom, TryInto},
    fmt,
    str::FromStr,
};

use serde_cbor::value::Value;

use crate::{
    error::RadError,
    operators::{Operable, RadonOpCodes, identity, integer as integer_operators},
    script::RadonCall,
    types::{RadonType, RadonTypes, string::RadonString},
};
use witnet_data_structures::{proto::versioning::ProtocolVersion::*, radon_report::ReportContext};

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
        let original_type = item.radon_type_name();
        if let RadonTypes::Integer(rad_integer) = item {
            Ok(rad_integer)
        } else {
            Value::try_from(item)?
                .try_into()
                .map_err(|_| Self::Error::Decode {
                    from: original_type,
                    to: RadonInteger::radon_type_name(),
                })
        }
    }
}

impl TryFrom<RadonString> for RadonInteger {
    type Error = RadError;

    fn try_from(value: RadonString) -> Result<Self, Self::Error> {
        Self::try_from(value.value().as_str())
    }
}

impl TryInto<Value> for RadonInteger {
    type Error = RadError;

    fn try_into(self) -> Result<Value, Self::Error> {
        Ok(Value::Integer(self.value()))
    }
}

impl From<i128> for RadonInteger {
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

impl Operable for RadonInteger {
    fn operate(&self, call: &RadonCall) -> Result<RadonTypes, RadError> {
        self.operate_in_context(call, &mut ReportContext::default())
    }

    fn operate_in_context(
        &self,
        call: &RadonCall,
        context: &mut ReportContext<RadonTypes>,
    ) -> Result<RadonTypes, RadError> {
        let protocol_version = context.protocol_version.unwrap_or_default();

        match call {
            // Identity
            (RadonOpCodes::Identity, None) => identity(RadonTypes::from(self.clone())),
            (RadonOpCodes::IntegerAbsolute, None) => {
                integer_operators::absolute(self).map(RadonTypes::from)
            }
            (RadonOpCodes::IntegerGreaterThan, Some(args)) => {
                integer_operators::greater_than(self, args).map(Into::into)
            }
            (RadonOpCodes::IntegerLessThan, Some(args)) => {
                integer_operators::less_than(self, args).map(Into::into)
            }
            (RadonOpCodes::IntegerModulo, Some(args)) => {
                integer_operators::modulo(self, args.as_slice()).map(RadonTypes::from)
            }
            (RadonOpCodes::IntegerMultiply, Some(args)) => {
                integer_operators::multiply(self, args.as_slice()).map(RadonTypes::from)
            }
            (RadonOpCodes::IntegerNegate, None) => {
                integer_operators::negate(self).map(RadonTypes::from)
            }
            (RadonOpCodes::IntegerPower, Some(args)) => {
                integer_operators::power(self, args.as_slice()).map(RadonTypes::from)
            }
            (RadonOpCodes::IntegerToBytes, args) if protocol_version >= V2_1 => {
                integer_operators::to_bytes(self, args).map(RadonTypes::from)
            }
            (RadonOpCodes::IntegerToFloat, None) => {
                integer_operators::to_float(self).map(RadonTypes::from)
            }
            (RadonOpCodes::IntegerToString, None) => {
                integer_operators::to_string(self).map(RadonTypes::from)
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

    assert!(result.is_err());
}

#[test]
fn test_from_vector() {
    let input: &[u8] = &[27, 0, 11, 41, 67, 10, 37, 109, 33]; // 3141592653589793

    let expected = RadonTypes::from(RadonInteger::from(3_141_592_653_589_793));
    let result = RadonTypes::try_from(input).unwrap();

    assert_eq!(result, expected);
}
