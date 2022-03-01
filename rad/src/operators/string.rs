use serde_cbor::value::{from_value, Value};
use std::{
    collections::BTreeMap,
    convert::{TryFrom, TryInto},
    str::FromStr,
};

use crate::{
    error::RadError,
    hash_functions::{self, RadonHashFunctions},
    types::{
        array::RadonArray, boolean::RadonBoolean, bytes::RadonBytes, float::RadonFloat,
        integer::RadonInteger, map::RadonMap, string::RadonString, RadonType, RadonTypes,
    },
};

const MAX_DEPTH: u8 = 20;

pub fn parse_json(input: &RadonString) -> Result<RadonTypes, RadError> {
    match json::parse(&input.value()) {
        Ok(json_value) => {
            let value = json_to_cbor(&json_value);
            RadonTypes::try_from(value)
        }
        Err(json_error) => Err(RadError::JsonParse {
            description: json_error.to_string(),
        }),
    }
}

pub fn parse_json_map(input: &RadonString) -> Result<RadonMap, RadError> {
    let item = parse_json(input)?;
    item.try_into()
}

pub fn parse_json_array(input: &RadonString) -> Result<RadonArray, RadError> {
    let item = parse_json(input)?;
    item.try_into()
}

fn add_children(
    map: &mut BTreeMap<String, RadonTypes>,
    text_children: Vec<RadonTypes>,
    element_children: Vec<(String, RadonTypes)>,
) {
    match text_children.len() {
        0 => {}
        1 => {
            let text_value = text_children.into_iter().next().unwrap();
            map.insert("_text".to_string(), text_value);
        }
        _ => {
            let text_value = RadonArray::from(text_children).into();
            map.insert("_text".to_string(), text_value);
        }
    }

    for (key, value) in element_children {
        match map.get_mut(&key) {
            Some(old_value) => match old_value {
                RadonTypes::Array(rad_array) => {
                    let mut new_array = rad_array.value();
                    new_array.push(value);

                    *rad_array = RadonArray::from(new_array);
                }
                x => {
                    let new_array = vec![x.clone(), value];
                    *x = RadonArray::from(new_array).into();
                }
            },
            None => {
                map.insert(key, value);
            }
        }
    }
}

fn parse_element_map(input: &minidom::Element, depth: u8) -> Result<RadonTypes, RadError> {
    if depth > MAX_DEPTH {
        return Err(RadError::XmlParseOverflow);
    }

    let mut map: BTreeMap<String, RadonTypes> = BTreeMap::new();
    for (k, v) in input.attrs() {
        map.insert(format!("@{}", k), RadonString::from(v).into());
    }

    let mut element_children = vec![];
    let mut text_children: Vec<RadonTypes> = vec![];
    for child in input.nodes() {
        match child {
            minidom::Node::Element(elem) => {
                let key = elem.name().to_string();
                let value = parse_element_map(elem, depth + 1)?;

                element_children.push((key, value));
            }
            minidom::Node::Text(text) => {
                // This check is to avoid blank spaces in xml would be included in the RadonMap
                let text_var = text.trim().to_string();
                if !text_var.is_empty() {
                    text_children.push(RadonString::from(text_var).into());
                }
            }
        }
    }

    let only_text_children = element_children.is_empty() && map.is_empty();

    if only_text_children {
        let text_value = match text_children.len() {
            0 => RadonString::from("").into(),
            1 => text_children.into_iter().next().unwrap(),
            _ => RadonArray::from(text_children).into(),
        };

        Ok(text_value)
    } else {
        add_children(&mut map, text_children, element_children);
        Ok(RadonMap::from(map).into())
    }
}

// Parse a XML `RadonString` to a `RadonMap` according to WIP0021 [https://github.com/witnet/WIPs/blob/master/wip-0021.md]
pub fn parse_xml_map(input: &RadonString) -> Result<RadonMap, RadError> {
    let minidom_element: Result<minidom::Element, minidom::Error> = input.value().parse();

    match minidom_element {
        Ok(element) => {
            let value = parse_element_map(&element, 0)?;
            let mut main_map: BTreeMap<String, RadonTypes> = BTreeMap::new();
            main_map.insert(element.name().to_string(), value);

            Ok(RadonMap::from(main_map))
        }
        Err(minidom_error) => Err(RadError::XmlParse {
            description: minidom_error.to_string(),
        }),
    }
}

pub fn radon_trim(input: &RadonString) -> String {
    if input.value().ends_with('\n') {
        input.value()[..input.value().len() - 1].to_string()
    } else {
        input.value()
    }
}

pub fn to_float(input: &RadonString) -> Result<RadonFloat, RadError> {
    let str_value = radon_trim(input);
    f64::from_str(&str_value)
        .map(RadonFloat::from)
        .map_err(Into::into)
}

pub fn to_int(input: &RadonString) -> Result<RadonInteger, RadError> {
    let str_value = radon_trim(input);
    i128::from_str(&str_value)
        .map(RadonInteger::from)
        .map_err(Into::into)
}

pub fn to_bool(input: &RadonString) -> Result<RadonBoolean, RadError> {
    let str_value = radon_trim(input);
    bool::from_str(&str_value)
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
        input_type: RadonString::radon_type_name(),
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
        input_type: RadonString::radon_type_name(),
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
            RadonTypes::Array(_) => Ok(RadonTypes::from(RadonArray::try_from(res.clone())?)),
            RadonTypes::Boolean(_) => Ok(RadonTypes::from(RadonBoolean::try_from(res.clone())?)),
            RadonTypes::Bytes(_) => Ok(RadonTypes::from(RadonBytes::try_from(res.clone())?)),
            RadonTypes::Float(_) => Ok(RadonTypes::from(RadonFloat::try_from(res.clone())?)),
            RadonTypes::Integer(_) => Ok(RadonTypes::from(RadonInteger::try_from(res.clone())?)),
            RadonTypes::Map(_) => Ok(RadonTypes::from(RadonMap::try_from(res.clone())?)),
            RadonTypes::RadonError(_) => unreachable!(),
            RadonTypes::String(_) => Ok(RadonTypes::from(RadonString::try_from(res.clone())?)),
        })
        .unwrap_or(Ok(temp_def))
}

/// Converts a JSON value (`json::JsonValue`) into a CBOR value (`serde_cbor::value::Value`).
/// Some conversions are totally straightforward, but some others  need some more logic (e.g.
/// telling apart integers from floats).
#[allow(clippy::cast_possible_truncation)]
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
        json::JsonValue::Number(value) => {
            let (_, _, exponent) = value.as_parts();
            let floating = f64::from(*value);
            // Cast the float into an integer if it has no fractional part and its value will fit
            // into the range of `i128` (38 is the biggest power of 10 that `i128` can safely hold)
            if floating.fract() == 0.0 && exponent.unsigned_abs() < 38 {
                // This cast is assumed to be safe as per the previous guard
                Value::Integer(floating as i128)
            } else {
                Value::Float(floating)
            }
        }
        json::JsonValue::String(value) => Value::Text(String::from(value.as_str())),
        json::JsonValue::Boolean(b) => Value::Bool(*b),
        json::JsonValue::Null => Value::Null,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{array::RadonArray, bytes::RadonBytes};
    use std::collections::BTreeMap;

    #[test]
    fn test_parse_json_map() {
        let json_map = RadonString::from(r#"{ "Hello": "world" }"#);
        let output = parse_json_map(&json_map).unwrap();

        let key = "Hello";
        let value = RadonTypes::String(RadonString::from("world"));
        let mut map = BTreeMap::new();
        map.insert(key.to_string(), value);
        let expected_output = RadonMap::from(map);

        assert_eq!(output, expected_output);
    }

    fn create_radon_map(input: Vec<(String, RadonTypes)>) -> RadonTypes {
        let mut map = BTreeMap::new();
        for (k, v) in input {
            map.insert(k.clone(), v);
        }

        RadonTypes::from(RadonMap::from(map))
    }

    #[test]
    fn test_parse_xml_map() {
        let xml_map = RadonString::from(
            r#"<?xml version="1.0"?>
            <Tag xmlns="https://witnet.io/">
                <InTag attr="0001">
                    <Name>Witnet</Name>
                    <Price currency="EUR">0.03</Price>
                    <Other><Nothing/></Other>
                </InTag>
                <InTag attr="0002">
                    <Name>Bitcoin</Name>
                    <Price currency="USD">49000</Price>
                    <Other value="nothing"/>
                </InTag>
            </Tag>
        "#,
        );
        let output = parse_xml_map(&xml_map).unwrap();

        let price1_element = create_radon_map(vec![
            ("@currency".to_string(), RadonString::from("EUR").into()),
            ("_text".to_string(), RadonString::from("0.03").into()),
        ]);
        let other1_element =
            create_radon_map(vec![("Nothing".to_string(), RadonString::from("").into())]);

        let price2_element = create_radon_map(vec![
            ("@currency".to_string(), RadonString::from("USD").into()),
            ("_text".to_string(), RadonString::from("49000").into()),
        ]);
        let other2_element = create_radon_map(vec![(
            "@value".to_string(),
            RadonString::from("nothing").into(),
        )]);

        let in_tag1_element = create_radon_map(vec![
            ("@attr".to_string(), RadonString::from("0001").into()),
            ("Name".to_string(), RadonString::from("Witnet").into()),
            ("Price".to_string(), price1_element),
            ("Other".to_string(), other1_element),
        ]);
        let in_tag2_element = create_radon_map(vec![
            ("@attr".to_string(), RadonString::from("0002").into()),
            ("Name".to_string(), RadonString::from("Bitcoin").into()),
            ("Price".to_string(), price2_element),
            ("Other".to_string(), other2_element),
        ]);

        let tag_element = create_radon_map(vec![
            (
                "@xmlns".to_string(),
                RadonString::from("https://witnet.io/").into(),
            ),
            (
                "InTag".to_string(),
                RadonArray::from(vec![in_tag1_element, in_tag2_element]).into(),
            ),
        ]);

        let expected_map = create_radon_map(vec![("Tag".to_string(), tag_element)]);

        assert_eq!(RadonTypes::from(output), expected_map);
    }

    #[test]
    fn test_parse_xml_map_no_ns() {
        let xml_map = RadonString::from(
            r#"<?xml version="1.0"?>
            <Tag>
                <InTag attr="0001">
                    <Name>Witnet</Name>
                </InTag>
            </Tag>
        "#,
        );
        let output = parse_xml_map(&xml_map).unwrap();

        let in_tag1_element = create_radon_map(vec![
            ("@attr".to_string(), RadonString::from("0001").into()),
            ("Name".to_string(), RadonString::from("Witnet").into()),
        ]);

        let tag_element = create_radon_map(vec![("InTag".to_string(), in_tag1_element)]);

        let expected_map = create_radon_map(vec![("Tag".to_string(), tag_element)]);

        assert_eq!(RadonTypes::from(output), expected_map);
    }

    #[test]
    fn test_parse_xml_map_with_2_ns() {
        let xml_map = RadonString::from(
            r#"<?xml version="1.0"?>
            <Tag xmlns:a="ns_A" xmlns:b="ns_B">
                <InTag attr="0001">
                    <Name>Witnet</Name>
                </InTag>
            </Tag>
        "#,
        );
        let output = parse_xml_map(&xml_map).unwrap();

        let in_tag1_element = create_radon_map(vec![
            ("@attr".to_string(), RadonString::from("0001").into()),
            ("Name".to_string(), RadonString::from("Witnet").into()),
        ]);

        let tag_element = create_radon_map(vec![
            ("@xmlns:a".to_string(), RadonString::from("ns_A").into()),
            ("@xmlns:b".to_string(), RadonString::from("ns_B").into()),
            ("InTag".to_string(), in_tag1_element),
        ]);

        let expected_map = create_radon_map(vec![("Tag".to_string(), tag_element)]);

        assert_eq!(RadonTypes::from(output), expected_map);
    }

    #[test]
    fn test_parse_xml_map_stack_overflow() {
        let n = 1000;
        let xml_map = RadonString::from(format!(
            r#"<?xml version="1.0"?>
            <Tests xmlns="https://witnet.io/">
            {}{}
            </Tests>
        "#,
            "<A>".repeat(n),
            "</A>".repeat(n)
        ));
        let output = parse_xml_map(&xml_map).unwrap_err();

        assert_eq!(output, RadError::XmlParseOverflow)
    }

    #[test]
    fn test_parse_json_map_with_null_entries() {
        // When parsing a JSON map, any keys with value `null` are ignored
        let json_map = RadonString::from(r#"{ "Hello": "world", "Bye": null }"#);
        let output = parse_json_map(&json_map).unwrap();

        let key = "Hello";
        let value = RadonTypes::String(RadonString::from("world"));
        let mut map = BTreeMap::new();
        map.insert(key.to_string(), value);
        let expected_output = RadonMap::from(map);

        assert_eq!(output, expected_output);
    }

    #[test]
    fn test_parse_json_map_fail() {
        let invalid_json = RadonString::from(r#"{ "Hello":  }"#);
        let output = parse_json_map(&invalid_json).unwrap_err();

        let expected_err = RadError::JsonParse {
            description: "Unexpected character: } at (1:13)".to_string(),
        };
        assert_eq!(output, expected_err);

        let json_array = RadonString::from(r#"[1,2,3]"#);
        let output = parse_json_map(&json_array).unwrap_err();
        let expected_err = RadError::Decode {
            from: "cbor::value::Value",
            to: RadonMap::radon_type_name(),
        };
        assert_eq!(output, expected_err);
    }

    #[test]
    fn test_parse_json_array() {
        let json_array = RadonString::from(r#"[1,2,3]"#);
        let output = parse_json_array(&json_array).unwrap();

        let expected_output = RadonArray::from(vec![
            RadonTypes::Integer(RadonInteger::from(1)),
            RadonTypes::Integer(RadonInteger::from(2)),
            RadonTypes::Integer(RadonInteger::from(3)),
        ]);

        assert_eq!(output, expected_output);
    }

    #[test]
    fn test_parse_json_array_with_null_entries() {
        // When parsing a JSON array, any elements with value `null` are ignored
        let json_array = RadonString::from(r#"[null, 1, null, null, 2, 3, null]"#);
        let output = parse_json_array(&json_array).unwrap();

        let expected_output = RadonArray::from(vec![
            RadonTypes::Integer(RadonInteger::from(1)),
            RadonTypes::Integer(RadonInteger::from(2)),
            RadonTypes::Integer(RadonInteger::from(3)),
        ]);

        assert_eq!(output, expected_output);
    }

    #[test]
    fn test_parse_json_array_fail() {
        let invalid_json = RadonString::from(r#"{ "Hello":  }"#);
        let output = parse_json_array(&invalid_json).unwrap_err();

        let expected_err = RadError::JsonParse {
            description: "Unexpected character: } at (1:13)".to_string(),
        };
        assert_eq!(output, expected_err);

        let json_map = RadonString::from(r#"{ "Hello": "world" }"#);
        let output = parse_json_array(&json_map).unwrap_err();
        let expected_err = RadError::Decode {
            from: "cbor::value::Value",
            to: RadonArray::radon_type_name(),
        };
        assert_eq!(output, expected_err);
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
            RadonTypes::Bytes(RadonBytes::from(vec![1]))
        );

        input_key = RadonString::from("key2");

        let result = string_match(&input_key, &args);
        assert_eq!(
            result.unwrap(),
            RadonTypes::Bytes(RadonBytes::from(vec![2]))
        );

        input_key = RadonString::from("key3");

        let result = string_match(&input_key, &args);
        assert_eq!(
            result.unwrap(),
            RadonTypes::Bytes(RadonBytes::from(vec![0]))
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
    fn test_string_match_map() {
        use std::convert::TryInto;
        let mut map: BTreeMap<Value, Value> = BTreeMap::new();

        let mut value_map_1: BTreeMap<String, RadonTypes> = BTreeMap::new();
        value_map_1.insert(
            "subkey1".to_string(),
            RadonTypes::String(RadonString::from("value1".to_string())),
        );

        let mut value_map_2: BTreeMap<String, RadonTypes> = BTreeMap::new();
        value_map_2.insert(
            "subkey2".to_string(),
            RadonTypes::String(RadonString::from("value2".to_string())),
        );

        let default_map: BTreeMap<String, RadonTypes> = BTreeMap::new();

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

    #[test]
    fn test_json_numbers_to_cbor_numbers() {
        use json::{number::Number, JsonValue};

        let json = JsonValue::Number(Number::from(2.0));
        let resulting_cbor = json_to_cbor(&json);
        let expected_cbor = serde_cbor::Value::Integer(2);
        assert_eq!(resulting_cbor, expected_cbor);

        let json = JsonValue::Number(Number::from(20.0));
        let resulting_cbor = json_to_cbor(&json);
        let expected_cbor = serde_cbor::Value::Integer(20);
        assert_eq!(resulting_cbor, expected_cbor);

        let json = JsonValue::Number(Number::from(2_000.0));
        let resulting_cbor = json_to_cbor(&json);
        let expected_cbor = serde_cbor::Value::Integer(2_000);
        assert_eq!(resulting_cbor, expected_cbor);

        let json = JsonValue::Number(Number::from(2_000_000.0));
        let resulting_cbor = json_to_cbor(&json);
        let expected_cbor = serde_cbor::Value::Integer(2_000_000);
        assert_eq!(resulting_cbor, expected_cbor);

        let json = JsonValue::Number(Number::from(std::f64::consts::PI));
        let resulting_cbor = json_to_cbor(&json);
        let expected_cbor = serde_cbor::Value::Float(std::f64::consts::PI);
        assert_eq!(resulting_cbor, expected_cbor);

        let json = JsonValue::Number(Number::from(1e100));
        let resulting_cbor = json_to_cbor(&json);
        let expected_cbor = serde_cbor::Value::Float(1e100);
        assert_eq!(resulting_cbor, expected_cbor);

        let json = JsonValue::Number(Number::from(4.0));
        let resulting_cbor = json_to_cbor(&json);
        let expected_cbor = serde_cbor::Value::Integer(4);
        assert_eq!(resulting_cbor, expected_cbor);

        let json = JsonValue::Number(Number::from(4.1));
        let resulting_cbor = json_to_cbor(&json);
        let expected_cbor = serde_cbor::Value::Float(4.1);
        assert_eq!(resulting_cbor, expected_cbor);
    }

    #[test]
    #[allow(clippy::float_cmp)]
    fn test_json_numbers_to_cbor_numbers_exponent_abs_overflow() {
        // Ensure that converting JSON numbers with a large negative exponent into CBOR numbers
        // does not cause any overflows

        // Parse the number using json::parse because otherwise it is just converted to 0.0
        let json = json::parse("0.1E-99999").unwrap();
        // The exponent is too small to fit in a i16, so the json library saturates the value to
        // i16::MIN:
        let (sign, mantissa, exponent) = json.as_number().unwrap().as_parts();
        assert_eq!((sign, mantissa, exponent), (true, 1, i16::MIN));
        // This number is rounded to exactly 0.0 when converted to f64
        assert_eq!(json.as_f64().unwrap(), 0.0);

        // Convert to CBOR
        let resulting_cbor = json_to_cbor(&json);

        // This exponent is too small to fit in a f64, so expected_f64 is equal to 0.0
        let expected_f64 = 0.1E-99999;
        assert_eq!(expected_f64, 0.0);
        // And the expected CBOR value is a float, not an integer
        let expected_cbor = serde_cbor::Value::Float(expected_f64);
        assert_eq!(resulting_cbor, expected_cbor);
    }

    #[test]
    fn test_json_numbers_to_cbor_booleans() {
        use json::JsonValue;

        let json = JsonValue::Boolean(false);
        let resulting_cbor = json_to_cbor(&json);
        let expected_cbor = serde_cbor::Value::Bool(false);
        assert_eq!(resulting_cbor, expected_cbor);

        let json = JsonValue::Boolean(true);
        let resulting_cbor = json_to_cbor(&json);
        let expected_cbor = serde_cbor::Value::Bool(true);
        assert_eq!(resulting_cbor, expected_cbor);
    }
}
