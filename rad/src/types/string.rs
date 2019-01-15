use crate::error::*;
use crate::operators::{identity, string as string_operators, Operable, RadonOpCodes};
use crate::script::RadonCall;
use crate::types::{RadonType, RadonTypes};

use std::fmt;
use witnet_data_structures::serializers::decoders::{TryFrom, TryInto};

#[derive(Debug, PartialEq)]
pub struct RadonString<'a> {
    value: &'a [u8],
}

impl<'a> RadonType<'a, &'a [u8]> for RadonString<'a> {
    fn value(&self) -> &'a [u8] {
        self.value
    }
}

impl<'a> From<&'a [u8]> for RadonString<'a> {
    fn from(value: &'a [u8]) -> Self {
        RadonString { value }
    }
}

impl<'a> From<&'a str> for RadonString<'a> {
    fn from(value: &'a str) -> Self {
        Self::from(value.as_bytes())
    }
}

impl<'a> TryFrom<&'a [u8]> for RadonString<'a> {
    type Error = RadError;

    fn try_from(slice: &'a [u8]) -> Result<Self, Self::Error> {
        Ok(Self::from(slice))
    }
}

impl<'a> TryInto<&'a [u8]> for RadonString<'a> {
    type Error = RadError;

    fn try_into(self) -> Result<&'a [u8], Self::Error> {
        Ok(self.value)
    }
}

impl<'a> Operable<'a> for RadonString<'a> {
    fn operate(self, call: &RadonCall) -> RadResult<RadonTypes<'a>> {
        match call {
            (RadonOpCodes::Identity, None) => identity(RadonTypes::String(self)),
            (RadonOpCodes::ParseJson, None) => {
                string_operators::parse_json(&self).map(RadonTypes::Mixed)
            }
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

impl<'a> fmt::Display for RadonString<'a> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "RadonString")
    }
}

#[test]
fn test_operate_identity() {
    let input = RadonString::from("Hello world!");
    let expected = RadonString::from("Hello world!").into();

    let call = (RadonOpCodes::Identity, None);
    let output = input.operate(&call).unwrap();

    assert_eq!(output, expected);
}

#[test]
fn test_operate_parsejson() {
    let valid_string = RadonString::from(r#"{ "Hello": "world" }"#);
    let invalid_string = RadonString::from(r#"{ "Not a JSON": }"#);

    let call = (RadonOpCodes::ParseJson, None);
    let valid_object = valid_string.operate(&call).unwrap();
    let invalid_object = invalid_string.operate(&call);

    assert!(if let RadonTypes::Mixed(mixed) = valid_object {
        if let rmpv::Value::Map(vector) = mixed.value() {
            if let Some((rmpv::Value::String(key), rmpv::Value::String(val))) = vector.first() {
                key.as_str() == Some("Hello") && val.as_str() == Some("world")
            } else {
                false
            }
        } else {
            false
        }
    } else {
        false
    });

    assert!(if let Err(_error) = invalid_object {
        true
    } else {
        false
    });
}

#[test]
fn test_operate_unimplemented() {
    let input = RadonString::from("Hello world!");

    let call = (RadonOpCodes::Fail, None);
    let result = input.operate(&call);

    assert!(if let Err(_error) = result {
        true
    } else {
        false
    });
}
