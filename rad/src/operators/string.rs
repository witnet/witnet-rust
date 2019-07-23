use std::{error::Error, str::FromStr};

use json;
use num_traits::FromPrimitive;
use serde_cbor::value::{from_value, Value};

use crate::error::RadError;
use crate::hash_functions::{self, RadonHashFunctions};
use crate::types::{bytes::RadonBytes, float::RadonFloat, string::RadonString, RadonType};

pub fn parse_json(input: &RadonString) -> Result<RadonBytes, RadError> {
    match json::parse(&input.value()) {
        Ok(json_value) => {
            let value = json_to_cbor(&json_value);
            Ok(RadonBytes::from(value))
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
    let wrong_args = || RadError::WrongArguments {
        input_type: "RadonString".to_string(),
        operator: "Hash".to_string(),
        args: args.to_vec(),
    };

    let input_string = input.value();
    let input_bytes = input_string.as_bytes();

    let arg = args.first().ok_or_else(wrong_args)?.to_owned();
    let hash_function_integer = from_value::<i64>(arg).map_err(|_| wrong_args())?;
    let hash_function_code =
        RadonHashFunctions::from_i64(hash_function_integer).ok_or_else(wrong_args)?;

    let digest = hash_functions::hash(input_bytes, hash_function_code)?;
    let hex_string = hex::encode(digest);

    Ok(RadonString::from(hex_string))
}

fn json_to_cbor(value: &json::JsonValue) -> Value {
    match value {
        json::JsonValue::Array(value) => Value::Array(value.iter().map(json_to_cbor).collect()),
        json::JsonValue::Object(value) => {
            let entries = value
                .iter()
                .map(|(key, value)| (Value::Text(String::from(key)), json_to_cbor(value)))
                .collect();
            Value::Map(entries)
        }
        json::JsonValue::Short(value) => Value::Text(String::from(value.as_str())),
        json::JsonValue::String(value) => Value::Text(String::from(value.as_str())),
        json::JsonValue::Number(value) => Value::Float((*value).into()),
        _ => Value::Null,
    }
}

#[test]
fn test_parse_json() {
    let valid_string = RadonString::from(r#"{ "Hello": "world" }"#);
    let invalid_string = RadonString::from(r#"{ "Hello": }"#);

    let valid_object = parse_json(&valid_string).unwrap();
    let invalid_object = parse_json(&invalid_string);

    assert!(if let Value::Map(map) = valid_object.value() {
        if let Some((Value::Text(key), Value::Text(val))) = map.iter().next() {
            key == "Hello" && val == "world"
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
        "Wrong `RadonString::Hash()` arguments: `[Integer(255)]`"
    );
    assert_eq!(
        &unsupported_output.unwrap_err().to_string(),
        "Hash function `RadonHashFunctions::Fail` is not implemented"
    );
}
