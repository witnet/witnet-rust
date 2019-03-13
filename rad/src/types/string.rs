use crate::error::RadError;
use crate::operators::{identity, string as string_operators, Operable, RadonOpCodes};
use crate::script::RadonCall;
use crate::types::{RadonType, RadonTypes};

use rmpv::Value;
use std::fmt;
use witnet_data_structures::serializers::decoders::{TryFrom, TryInto};

pub const RADON_STRING_TYPE_NAME: &str = "RadonString";

#[derive(Clone, Debug, PartialEq)]
pub struct RadonString {
    value: String,
}

impl RadonType<String> for RadonString {
    fn value(&self) -> String {
        self.value.clone()
    }

    fn radon_type_name() -> String {
        RADON_STRING_TYPE_NAME.to_string()
    }
}

impl TryFrom<Value> for RadonString {
    type Error = RadError;

    fn try_from(value: Value) -> Result<Self, Self::Error> {
        value
            .as_str()
            .map(Self::from)
            .ok_or_else(|| RadError::Decode {
                from: "rmpv::Value".to_string(),
                to: RADON_STRING_TYPE_NAME.to_string(),
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

impl Operable for RadonString {
    fn operate(self, call: &RadonCall) -> Result<RadonTypes, RadError> {
        match call {
            (RadonOpCodes::Identity, None) => identity(RadonTypes::String(self)),
            (RadonOpCodes::ParseJson, None) => {
                string_operators::parse_json(&self).map(RadonTypes::Mixed)
            }
            (op_code, args) => Err(RadError::UnsupportedOperator {
                input_type: RADON_STRING_TYPE_NAME.to_string(),
                operator: op_code.to_string(),
                args: args.to_owned(),
            }),
        }
    }
}

impl fmt::Display for RadonString {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, r#"{}("{}")"#, RADON_STRING_TYPE_NAME, self.value)
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

#[test]
fn test_serialize_radon_string() {
    let input = RadonTypes::from(RadonString::from("Hello world!"));
    let expected: Vec<u8> = vec![172, 72, 101, 108, 108, 111, 32, 119, 111, 114, 108, 100, 33];

    let output: Vec<u8> = RadonTypes::try_into(input).unwrap();

    assert_eq!(output, expected);
}
