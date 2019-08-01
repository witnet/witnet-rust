use crate::error::RadError;
use crate::hash_functions::{self, RadonHashFunctions};
use crate::types::{bytes::RadonBytes, float::RadonFloat, string::RadonString, RadonType};

use json;
use num_traits::FromPrimitive;
use rmpv::{self, Value};
use std::{error::Error, str::FromStr};

pub fn parse_json(input: &RadonString) -> Result<RadonBytes, RadError> {
    match json::parse(&input.value()) {
        Ok(json_value) => {
            let value = json_to_rmp(&json_value);
            Ok(RadonBytes::from(value.to_owned()))
        }
        Err(json_error) => Err(RadError::JsonParse {
            description: json_error.description().to_owned(),
        }),
    }
}
pub fn to_float(input: &RadonString) -> Result<RadonFloat, RadError> {
    f64::from_str(&input.value())
        .map(RadonFloat::from)
        .map_err(Into::into)
}

pub fn hash(input: &RadonString, args: &[Value]) -> Result<RadonString, RadError> {
    let error = || RadError::WrongArguments {
        input_type: "RadonString".to_string(),
        operator: "Hash".to_string(),
        args: args.to_vec(),
    };

    let string = input.value();
    let bytes = string.as_bytes();
    let hash_function_integer = args.first().ok_or_else(error)?.as_i64().ok_or_else(error)?;
    let hash_function_code =
        RadonHashFunctions::from_i64(hash_function_integer).ok_or_else(error)?;

    let digest = hash_functions::hash(bytes, hash_function_code)?;
    let hex_string = hex::encode(digest);

    Ok(RadonString::from(hex_string))
}

fn json_to_rmp(value: &json::JsonValue) -> rmpv::ValueRef {
    match value {
        json::JsonValue::Array(value) => {
            rmpv::ValueRef::Array(value.iter().map(json_to_rmp).collect())
        }
        json::JsonValue::Object(value) => {
            let entries = value
                .iter()
                .map(|(key, value)| (rmpv::ValueRef::from(key), json_to_rmp(value)))
                .collect();
            rmpv::ValueRef::Map(entries)
        }
        json::JsonValue::Short(value) => {
            rmpv::ValueRef::String(rmpv::Utf8StringRef::from(value.as_str()))
        }
        json::JsonValue::String(value) => {
            rmpv::ValueRef::String(rmpv::Utf8StringRef::from(value.as_str()))
        }
        json::JsonValue::Number(value) => rmpv::ValueRef::F64((*value).into()),
        _ => rmpv::ValueRef::Nil,
    }
}

#[test]
fn test_parse_json() {
    let valid_string = RadonString::from(r#"{ "Hello": "world" }"#);
    let invalid_string = RadonString::from(r#"{ "Hello": }"#);

    let valid_object = parse_json(&valid_string).unwrap();
    let invalid_object = parse_json(&invalid_string);

    assert!(if let rmpv::Value::Map(vector) = valid_object.value() {
        if let Some((rmpv::Value::String(key), rmpv::Value::String(val))) = vector.first() {
            key.as_str() == Some("Hello") && val.as_str() == Some("world")
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
fn test_hash() {
    let input = RadonString::from("Hello, World!");
    let valid_args = [Value::from(0x0A)]; // 0x0A is RadonHashFunctions::SHA_256
    let wrong_args = [Value::from(0xFF)]; // 0xFF is not a member of RadonHashFunctions
    let unsupported_args = [Value::from(-1)]; // -1 is RadonHashFunctions::Fail (unsupported)

    let valid_output = hash(&input, &valid_args).unwrap();
    let wrong_output = hash(&input, &wrong_args);
    let unsupported_output = hash(&input, &unsupported_args);

    let valid_expected =
        RadonString::from("dffd6021bb2bd5b0af676290809ec3a53191dd81c7f70a4b28688a362182986f");

    assert_eq!(valid_output, valid_expected);
    assert_eq!(
        &wrong_output.unwrap_err().to_string(),
        "Wrong `RadonString::Hash()` arguments: `[Integer(PosInt(255))]`"
    );
    assert_eq!(
        &unsupported_output.unwrap_err().to_string(),
        "Hash function `RadonHashFunctions::Fail` is not implemented"
    );
}
