use crate::error::*;
use crate::operators::{identity, string as string_operators, Operable, RadonOpCodes};
use crate::script::RadonCall;
use crate::types::{mixed::RadonMixed, RadonType, RadonTypes};

use rmpv::Value;
use std::fmt;
use witnet_data_structures::serializers::decoders::{TryFrom, TryInto};

#[derive(Clone, Debug, PartialEq)]
pub struct RadonString {
    value: String,
}

impl<'a> RadonType<'a, String> for RadonString {
    fn value(&self) -> String {
        self.value.clone()
    }
}

impl TryFrom<Value> for RadonString {
    type Error = RadError;

    fn try_from(value: Value) -> Result<Self, Self::Error> {
        value.as_str().map(Self::from).ok_or_else(|| {
            RadError::new(
                RadErrorKind::EncodeDecode,
                String::from("Error creating a RadonString from a MessagePack value"),
            )
        })
    }
}

impl TryInto<Value> for RadonString {
    type Error = RadError;

    fn try_into(self) -> Result<Value, Self::Error> {
        Ok(Value::from(self.value()))
    }
}

impl From<String> for RadonString {
    fn from(value: String) -> Self {
        RadonString { value }
    }
}

impl<'a> From<&'a str> for RadonString {
    fn from(value: &'a str) -> Self {
        Self::from(String::from(value))
    }
}

impl<'a> TryFrom<&'a [u8]> for RadonString {
    type Error = RadError;

    fn try_from(vector: &'a [u8]) -> Result<Self, Self::Error> {
        let mixed = RadonMixed::try_from(vector)?;
        let value: Value = RadonMixed::try_into(mixed)?;

        Self::try_from(value)
    }
}

impl<'a> TryInto<Vec<u8>> for RadonString {
    type Error = RadError;

    fn try_into(self) -> Result<Vec<u8>, Self::Error> {
        let value: Value = Self::try_into(self)?;
        let mixed = RadonMixed::try_from(value)?;

        RadonMixed::try_into(mixed)
    }
}

impl Operable for RadonString {
    fn operate(self, call: &RadonCall) -> RadResult<RadonTypes> {
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

impl<'a> fmt::Display for RadonString {
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
