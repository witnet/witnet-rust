use json;
use serde_cbor::value::{from_value, Value};
use std::{convert::TryFrom, error::Error, str::FromStr};

use crate::{
    error::RadError,
    hash_functions::{self, RadonHashFunctions},
    types::{
        array::RadonArray, boolean::RadonBoolean, bytes::RadonBytes, float::RadonFloat,
        integer::RadonInteger, map::RadonMap, mixed::RadonMixed, result::RadonResult,
        string::RadonString, RadonType, RadonTypes,
    },
};

pub fn to_mixed(input: RadonString) -> RadonMixed {
    RadonMixed::from(Value::Text(input.value()))
}

pub fn parse_json(input: &RadonString) -> Result<RadonMixed, RadError> {
    match json::parse(&input.value()) {
        Ok(json_value) => {
            let value = json_to_cbor(&json_value);
            Ok(RadonMixed::from(value))
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

pub fn to_int(input: &RadonString) -> Result<RadonInteger, RadError> {
    i128::from_str(&input.value())
        .map(RadonInteger::from)
        .map_err(Into::into)
}

pub fn to_bool(input: &RadonString) -> Result<RadonBoolean, RadError> {
    bool::from_str(&input.value())
        .map(RadonBoolean::from)
        .map_err(Into::into)
}

pub fn length(input: &RadonString) -> RadonInteger {
    RadonInteger::from(input.value().len() as i128)
}

pub fn to_lowercase(input: &RadonString) -> RadonString {
    RadonString::from(input.value().as_str().to_lowercase())
}

pub fn to_uppercase(input: &RadonString) -> RadonString {
    RadonString::from(input.value().as_str().to_uppercase())
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
    let hash_function_integer = from_value::<u8>(arg).map_err(|_| wrong_args())?;
    let hash_function_code =
        RadonHashFunctions::try_from(hash_function_integer).map_err(|_| wrong_args())?;

    let digest = hash_functions::hash(input_bytes, hash_function_code)?;
    let hex_string = hex::encode(digest);

    Ok(RadonString::from(hex_string))
}

pub fn string_match(input: &RadonString, args: &[Value]) -> Result<RadonTypes, RadError> {
    let wrong_args = || RadError::WrongArguments {
        input_type: "RadonString".to_string(),
        operator: "String match".to_string(),
        args: args.to_vec(),
    };

    let first_arg = args.first().ok_or_else(wrong_args)?.to_owned();
    let map = RadonMap::try_from(first_arg)?;
    let second_arg = args.get(1).ok_or_else(wrong_args)?.to_owned();
    let default = RadonTypes::try_from(second_arg)?;
    let temp_def = default.clone();
    let map_value = map.value();

    map_value
        .get(&input.value())
        .map(|res| match default {
            RadonTypes::Array(_) => Ok(RadonTypes::from(RadonArray::try_from(res.value())?)),
            RadonTypes::Boolean(_) => Ok(RadonTypes::from(RadonBoolean::try_from(res.value())?)),
            RadonTypes::Bytes(_) => Ok(RadonTypes::from(RadonBytes::try_from(res.value())?)),
            RadonTypes::Float(_) => Ok(RadonTypes::from(RadonFloat::try_from(res.value())?)),
            RadonTypes::Integer(_) => Ok(RadonTypes::from(RadonInteger::try_from(res.value())?)),
            RadonTypes::Map(_) => Ok(RadonTypes::from(RadonMap::try_from(res.value())?)),
            RadonTypes::Mixed(_) => Ok(RadonTypes::from(res.clone())),
            RadonTypes::Result(_) => Ok(RadonTypes::from(RadonResult::try_from(res.value())?)),
            RadonTypes::String(_) => Ok(RadonTypes::from(RadonString::try_from(res.value())?)),
        })
        .unwrap_or(Ok(temp_def))
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

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use super::*;

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
        let wrong_args = [Value::from(0xFE)]; // 0xFF is not a member of RadonHashFunctions
        let unsupported_args = [Value::from(0xFF)]; // -1 is RadonHashFunctions::Fail (unsupported)

        let valid_output = hash(&input, &valid_args).unwrap();
        let wrong_output = hash(&input, &wrong_args);
        let unsupported_output = hash(&input, &unsupported_args);

        let valid_expected =
            RadonString::from("dffd6021bb2bd5b0af676290809ec3a53191dd81c7f70a4b28688a362182986f");

        assert_eq!(valid_output, valid_expected);
        assert_eq!(
            &wrong_output.unwrap_err().to_string(),
            "Wrong `RadonString::Hash()` arguments: `[Integer(254)]`"
        );
        assert_eq!(
            &unsupported_output.unwrap_err().to_string(),
            "Hash function `RadonHashFunctions::Fail` is not implemented"
        );
    }

    #[test]
    fn test_string_to_integer() {
        let rad_int = RadonInteger::from(10);
        let rad_string: RadonString = RadonString::from("10");

        assert_eq!(to_int(&rad_string).unwrap(), rad_int);
    }

    #[test]
    fn test_string_to_float() {
        let rad_float = RadonFloat::from(10.2);
        let rad_string: RadonString = RadonString::from("10.2");

        assert_eq!(to_float(&rad_string).unwrap(), rad_float);
    }

    #[test]
    fn test_string_to_bool() {
        let rad_float = RadonBoolean::from(false);
        let rad_string: RadonString = RadonString::from("false");

        assert_eq!(to_bool(&rad_string).unwrap(), rad_float);
    }

    #[test]
    fn test_string_length() {
        let rad_string: RadonString = RadonString::from("Hello");

        assert_eq!(length(&rad_string), RadonInteger::from(5));
    }

    #[test]
    fn test_string_to_lowercase() {
        let rad_string: RadonString = RadonString::from("HeLlO");

        assert_eq!(to_lowercase(&rad_string), RadonString::from("hello"));
    }

    #[test]
    fn test_string_to_uppercase() {
        let rad_string: RadonString = RadonString::from("HeLlO");

        assert_eq!(to_uppercase(&rad_string), RadonString::from("HELLO"));
    }

    #[test]
    fn test_string_match_booleans() {
        let mut map: BTreeMap<Value, Value> = BTreeMap::new();
        map.insert(Value::Text("key1".to_string()), Value::Bool(true));
        map.insert(Value::Text("key2".to_string()), Value::Bool(false));

        let mut input_key = RadonString::from("key1");

        let args = vec![Value::Map(map), Value::Bool(false)];

        let result = string_match(&input_key, &args);
        assert_eq!(result.unwrap(), RadonTypes::from(RadonBoolean::from(true)));

        input_key = RadonString::from("key2");

        let result = string_match(&input_key, &args);
        assert_eq!(result.unwrap(), RadonTypes::from(RadonBoolean::from(false)));

        input_key = RadonString::from("key3");

        let result = string_match(&input_key, &args);
        assert_eq!(result.unwrap(), RadonTypes::from(RadonBoolean::from(false)));
    }

    #[test]
    fn test_string_match_integers() {
        let mut map: BTreeMap<Value, Value> = BTreeMap::new();
        map.insert(Value::Text("key1".to_string()), Value::Integer(1));
        map.insert(Value::Text("key2".to_string()), Value::Integer(2));

        let mut input_key = RadonString::from("key1");

        let args = vec![Value::Map(map), Value::Integer(0)];

        let result = string_match(&input_key, &args);
        assert_eq!(result.unwrap(), RadonTypes::from(RadonInteger::from(1i128)));

        input_key = RadonString::from("key2");

        let result = string_match(&input_key, &args);
        assert_eq!(result.unwrap(), RadonTypes::from(RadonInteger::from(2i128)));

        input_key = RadonString::from("key3");

        let result = string_match(&input_key, &args);
        assert_eq!(result.unwrap(), RadonTypes::from(RadonInteger::from(0i128)));
    }

    #[test]
    fn test_string_match_strings() {
        let mut map: BTreeMap<Value, Value> = BTreeMap::new();
        map.insert(
            Value::Text("key1".to_string()),
            Value::Text("value1".to_string()),
        );
        map.insert(
            Value::Text("key2".to_string()),
            Value::Text("value2".to_string()),
        );

        let mut input_key = RadonString::from("key1");

        let args = vec![Value::Map(map), Value::Text("default".to_string())];

        let result = string_match(&input_key, &args);
        assert_eq!(
            result.unwrap(),
            RadonTypes::from(RadonString::from("value1".to_string()))
        );

        input_key = RadonString::from("key2");

        let result = string_match(&input_key, &args);
        assert_eq!(
            result.unwrap(),
            RadonTypes::from(RadonString::from("value2".to_string()))
        );

        input_key = RadonString::from("key3");

        let result = string_match(&input_key, &args);
        assert_eq!(
            result.unwrap(),
            RadonTypes::from(RadonString::from("default".to_string()))
        );
    }

    #[test]
    fn test_string_match_floats() {
        let mut map: BTreeMap<Value, Value> = BTreeMap::new();
        map.insert(Value::Text("key1".to_string()), Value::Float(1.0));
        map.insert(Value::Text("key2".to_string()), Value::Float(2.0));

        let mut input_key = RadonString::from("key1");

        let args = vec![Value::Map(map), Value::Float(0.5f64)];

        let result = string_match(&input_key, &args);
        assert_eq!(result.unwrap(), RadonTypes::from(RadonFloat::from(1f64)));

        input_key = RadonString::from("key2");

        let result = string_match(&input_key, &args);
        assert_eq!(result.unwrap(), RadonTypes::from(RadonFloat::from(2f64)));

        input_key = RadonString::from("key3");

        let result = string_match(&input_key, &args);
        assert_eq!(result.unwrap(), RadonTypes::from(RadonFloat::from(0.5f64)));
    }

    #[test]
    fn test_string_match_bytes() {
        let mut map: BTreeMap<Value, Value> = BTreeMap::new();
        map.insert(Value::Text("key1".to_string()), Value::Bytes(vec![1]));
        map.insert(Value::Text("key2".to_string()), Value::Bytes(vec![2]));

        let mut input_key = RadonString::from("key1");

        let args = vec![Value::Map(map), Value::Bytes(vec![0])];

        let result = string_match(&input_key, &args);
        assert_eq!(
            result.unwrap(),
            RadonTypes::from(RadonMixed::from(Value::Bytes(vec![1])))
        );

        input_key = RadonString::from("key2");

        let result = string_match(&input_key, &args);
        assert_eq!(
            result.unwrap(),
            RadonTypes::from(RadonMixed::from(Value::Bytes(vec![2])))
        );

        input_key = RadonString::from("key3");

        let result = string_match(&input_key, &args);
        assert_eq!(
            result.unwrap(),
            RadonTypes::from(RadonMixed::from(Value::Bytes(vec![0])))
        );
    }

    #[test]
    fn test_string_match_array() {
        let mut map: BTreeMap<Value, Value> = BTreeMap::new();

        map.insert(
            Value::Text("key1".to_string()),
            Value::Array(vec![Value::Integer(4), Value::Integer(4)]),
        );

        map.insert(
            Value::Text("key2".to_string()),
            Value::Array(vec![Value::Integer(5), Value::Integer(5)]),
        );

        let mut input_key = RadonString::from("key1");

        let args = vec![Value::Map(map), Value::Array(vec![])];

        let result = string_match(&input_key, &args);
        let expected1 = RadonTypes::from(RadonArray::from(vec![
            RadonInteger::from(4i128).into(),
            RadonInteger::from(4i128).into(),
        ]));
        assert_eq!(result.unwrap(), expected1);

        let expected2 = RadonTypes::from(RadonArray::from(vec![
            RadonInteger::from(5i128).into(),
            RadonInteger::from(5i128).into(),
        ]));
        input_key = RadonString::from("key2");

        let result = string_match(&input_key, &args);
        assert_eq!(result.unwrap(), expected2);

        input_key = RadonString::from("key3");
        let expected3 = RadonTypes::from(RadonArray::from(vec![]));

        let result = string_match(&input_key, &args);
        assert_eq!(result.unwrap(), expected3);
    }

    #[test]
    fn test_string_match_map() {
        use std::convert::TryInto;
        let mut map: BTreeMap<Value, Value> = BTreeMap::new();

        let mut value_map_1: BTreeMap<String, RadonMixed> = BTreeMap::new();
        value_map_1.insert(
            "subkey1".to_string(),
            RadonMixed::from(Value::Text("value1".to_string())),
        );

        let mut value_map_2: BTreeMap<String, RadonMixed> = BTreeMap::new();
        value_map_2.insert(
            "subkey2".to_string(),
            RadonMixed::from(Value::Text("value2".to_string())),
        );

        let default_map: BTreeMap<String, RadonMixed> = BTreeMap::new();

        map.insert(
            Value::Text("key1".to_string()),
            RadonMap::from(value_map_1.clone()).try_into().unwrap(),
        );
        map.insert(
            Value::Text("key2".to_string()),
            RadonMap::from(value_map_2.clone()).try_into().unwrap(),
        );

        let mut input_key = RadonString::from("key1");

        let args = vec![
            Value::Map(map),
            RadonMap::from(default_map.clone()).try_into().unwrap(),
        ];

        let result = string_match(&input_key, &args);
        let expected1 = RadonTypes::from(RadonMap::from(value_map_1));
        assert_eq!(result.unwrap(), expected1);

        let expected2 = RadonTypes::from(RadonMap::from(value_map_2));

        input_key = RadonString::from("key2");

        let result = string_match(&input_key, &args);
        assert_eq!(result.unwrap(), expected2);

        input_key = RadonString::from("key3");
        let expected3 = RadonTypes::from(RadonMap::from(default_map));

        let result = string_match(&input_key, &args);
        assert_eq!(result.unwrap(), expected3);
    }

    #[test]
    fn test_string_match_wrong_arguments() {
        let mut map: BTreeMap<Value, Value> = BTreeMap::new();
        map.insert(Value::Text("key1".to_string()), Value::Float(1.0));
        map.insert(Value::Text("key2".to_string()), Value::Float(2.0));

        let input_key = RadonString::from("key1");

        let args = vec![Value::Float(0.5f64), Value::Float(0.5f64)];

        let result = string_match(&input_key, &args);
        assert_eq!(
            result.unwrap_err().to_string(),
            "Failed to decode RadonMap from cbor::value::Value"
        );
    }

    #[test]
    fn test_string_match_mismatched_types() {
        let mut map: BTreeMap<Value, Value> = BTreeMap::new();
        map.insert(Value::Text("key1".to_string()), Value::Float(1.0));
        map.insert(Value::Text("key2".to_string()), Value::Bool(true));

        let input_key = RadonString::from("key1");

        let args = vec![Value::Map(map), Value::Bool(false)];

        let result = string_match(&input_key, &args);
        assert_eq!(
            result.unwrap_err().to_string(),
            "Failed to decode RadonBoolean from cbor::value::Value"
        );
    }

    #[test]
    fn test_string_match_default_insufficient_arguments() {
        let mut map: BTreeMap<Value, Value> = BTreeMap::new();
        map.insert(Value::Text("key1".to_string()), Value::Float(1.0));
        map.insert(Value::Text("key2".to_string()), Value::Float(2.0));

        let input_key = RadonString::from("any");

        let args = vec![Value::Map(map)];

        let result = string_match(&input_key, &args);
        assert_eq!(result.unwrap_err().to_string(), "Wrong `RadonString::String match()` arguments: `[Map({Text(\"key1\"): Float(1.0), Text(\"key2\"): Float(2.0)})]`");
    }

    #[test]
    fn test_string_match_empty_map() {
        let map: BTreeMap<Value, Value> = BTreeMap::new();

        let args = vec![Value::Map(map), Value::Text("default".to_string())];

        let input_key = RadonString::from("any");

        let result = string_match(&input_key, &args);
        assert_eq!(
            result.unwrap(),
            RadonTypes::from(RadonString::from("default".to_string()))
        );
    }
}
