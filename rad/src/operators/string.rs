use std::{
    collections::BTreeMap,
    convert::{TryFrom, TryInto},
    str::FromStr,
};

use serde_cbor::value::{from_value, Value};
use serde_json::Value as JsonValue;

use slicestring::Slice;
use regex::Regex;

use crate::{
    error::RadError,
    hash_functions::{self, RadonHashFunctions},
    types::{
        array::RadonArray, boolean::RadonBoolean, bytes::RadonBytes, float::RadonFloat,
        integer::RadonInteger, map::RadonMap, string::RadonString, RadonType, RadonTypes,
    },
};

const MAX_DEPTH: u8 = 20;
const DEFAULT_THOUSANDS_SEPARATOR: &str = ",";
const DEFAULT_DECIMAL_SEPARATOR: &str = ".";

/// Parse `RadonTypes` from a JSON-encoded `RadonString`.
pub fn parse_json(input: &RadonString) -> Result<RadonTypes, RadError> {
    let json_value: JsonValue =
        serde_json::from_str(&input.value()).map_err(|err| RadError::JsonParse {
            description: err.to_string(),
        })?;

    RadonTypes::try_from(json_value)
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

pub fn to_bool(input: &RadonString) -> Result<RadonBoolean, RadError> {
    let str_value = radon_trim(input);
    bool::from_str(&str_value)
        .map(RadonBoolean::from)
        .map_err(Into::into)
}

/// Converts a `RadonString` into a `RadonFloat`, provided that the input string actually represents
/// a valid floating point number.
pub fn as_float(input: &RadonString, args: &Option<Vec<Value>>) -> Result<RadonFloat, RadError> {
    f64::from_str(&as_numeric_string(
        input,
        args.as_deref().unwrap_or_default(),
    ))
    .map(RadonFloat::from)
    .map_err(Into::into)
}

/// Converts a `RadonString` into a `RadonFloat`, provided that the input string actually represents
/// a valid integer number.
pub fn as_integer(
    input: &RadonString,
    args: &Option<Vec<Value>>,
) -> Result<RadonInteger, RadError> {
    i128::from_str(&as_numeric_string(
        input,
        args.as_deref().unwrap_or_default(),
    ))
    .map(RadonInteger::from)
    .map_err(Into::into)
}

/// Converts a `RadonString` into a `String` containing a numeric value, provided that the input
/// string actually represents a valid number.
pub fn as_numeric_string(input: &RadonString, args: &[Value]) -> String {
    let str_value = radon_trim(input);
    let (thousands_separator, decimal_separator) = read_separators_from_args(args);

    replace_separators(str_value, thousands_separator, decimal_separator)
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

pub fn string_replace(input: &RadonString, args: &[Value]) -> Result<RadonString, RadError> {
    let wrong_args = || RadError::WrongArguments { 
        input_type: RadonString::radon_type_name(),
        operator: "StringReplace".to_string(),
        args: args.to_vec(),
    };
    let regex = RadonString::try_from(args.first().ok_or_else(wrong_args)?.to_owned())?;
    let replacement = RadonString::try_from(args.get(1).ok_or_else(wrong_args)?.to_owned())?;
    Ok(RadonString::from(input.value().as_str().replace(regex.value().as_str(), replacement.value().as_str())))
}

pub fn string_slice(input: &RadonString, args: &[Value]) -> Result<RadonString, RadError> {
    let wrong_args = || RadError::WrongArguments { 
        input_type: RadonString::radon_type_name(),
        operator: "StringSlice".to_string(),
        args: args.to_vec(),
    };
    let mut end_index: usize = input.value().len();
    match args.len() {
        2 => {
            let start_index = from_value::<i64>(args[0].clone()).unwrap_or_default().rem_euclid(end_index as i64) as usize;
            end_index = from_value::<i64>(args[1].clone()).unwrap_or_default().rem_euclid(end_index as i64) as usize;
            Ok(RadonString::from(input.value().as_str().slice(start_index..end_index)))
        }
        1 => {
            let start_index = from_value::<i64>(args[0].clone()).unwrap_or_default().rem_euclid(end_index as i64) as usize;
            Ok(RadonString::from(input.value().as_str().slice(start_index..end_index)))
        }
        _ => Err(wrong_args())
    }
}

pub fn string_split(input: &RadonString, args: &[Value]) -> Result<RadonArray, RadError> {
    let wrong_args = || RadError::WrongArguments { 
        input_type: RadonString::radon_type_name(),
        operator: "StringSplit".to_string(),
        args: args.to_vec(),
    };
    let pattern = RadonString::try_from(args.first().ok_or_else(wrong_args)?.to_owned())?;
    let parts: Vec<RadonTypes> = Regex::new(pattern.value().as_str()).unwrap().split(input.value().as_str()).map(|part| RadonTypes::from(RadonString::from(part))).collect();
    Ok(RadonArray::from(parts))
}

pub fn string_match(input: &RadonString, args: &[Value]) -> Result<RadonTypes, RadError> {
    let wrong_args = || RadError::WrongArguments {
        input_type: RadonString::radon_type_name(),
        operator: "StringMatch".to_string(),
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
        .map(|res| {
            // Workaround fix for test test_string_match_mismatched_types_default_string:
            // only allow the result to be RadonString if it is actually a RadonString, implicit
            // conversions like RadonInteger to RadonString are disabled.
            match (&res, &default) {
                (RadonTypes::String(_), RadonTypes::String(_)) => {
                    return Ok(RadonTypes::from(RadonString::try_from(res.clone())?));
                }
                (RadonTypes::Bytes(_), RadonTypes::String(_)) => {
                    return Ok(RadonTypes::from(RadonString::try_from(res.clone())?));
                }
                (_, RadonTypes::String(_)) => {
                    return Err(RadError::Decode {
                        from: "serde_cbor::value::Value",
                        to: "RadonString",
                    });
                }
                _ => {}
            }

            match default {
                RadonTypes::Array(_) => Ok(RadonTypes::from(RadonArray::try_from(res.clone())?)),
                RadonTypes::Boolean(_) => {
                    Ok(RadonTypes::from(RadonBoolean::try_from(res.clone())?))
                }
                RadonTypes::Bytes(_) => Ok(RadonTypes::from(RadonBytes::try_from(res.clone())?)),
                RadonTypes::Float(_) => Ok(RadonTypes::from(RadonFloat::try_from(res.clone())?)),
                RadonTypes::Integer(_) => {
                    Ok(RadonTypes::from(RadonInteger::try_from(res.clone())?))
                }
                RadonTypes::Map(_) => Ok(RadonTypes::from(RadonMap::try_from(res.clone())?)),
                RadonTypes::RadonError(_) => unreachable!(),
                RadonTypes::String(_) => {
                    // Handled above
                    unreachable!();
                }
            }
        })
        .unwrap_or(Ok(temp_def))
}

/// Replace thousands and decimals separators in a `String`.
#[inline]
pub fn replace_separators(
    value: String,
    thousands_separator: String,
    decimal_separator: String,
) -> String {
    value
        .replace(&thousands_separator, "")
        .replace(&decimal_separator, DEFAULT_DECIMAL_SEPARATOR)
}

/// Read separators from RAD call arguments, and fall back to the default ones if not provided.
pub fn read_separators_from_args(args: &[serde_cbor::Value]) -> (String, String) {
    match args.len() {
        2 => (
            from_value::<String>(args[0].clone()).unwrap_or_else(default_thousands_separator),
            from_value::<String>(args[1].clone()).unwrap_or_else(default_decimal_separator),
        ),
        1 => (
            from_value::<String>(args[0].clone()).unwrap_or_else(default_thousands_separator),
            default_decimal_separator(()),
        ),
        _ => (
            default_thousands_separator(()),
            default_decimal_separator(()),
        ),
    }
}

#[inline]
fn default_thousands_separator<T>(_: T) -> String {
    String::from(DEFAULT_THOUSANDS_SEPARATOR)
}

#[inline]
fn default_decimal_separator<T>(_: T) -> String {
    String::from(DEFAULT_DECIMAL_SEPARATOR)
}

/// This module was introduced for encapsulating the interim legacy logic before WIP-0024 is
/// introduced, for the sake of maintainability.
///
/// Because RADON scripts are never evaluated for old blocks (e.g. during synchronization), this
/// module can theoretically be removed altogether once WIP-0024 is activated.
pub mod legacy {
    use super::*;

    /// Legacy (pre-WIP0024) version of `as_float`.
    pub fn as_float_before_wip0024(input: &RadonString) -> Result<RadonFloat, RadError> {
        let str_value = radon_trim(input);
        f64::from_str(&str_value)
            .map(RadonFloat::from)
            .map_err(Into::into)
    }

    /// Legacy (pre-WIP0024) version of `as_integer`.
    pub fn as_integer_before_wip0024(input: &RadonString) -> Result<RadonInteger, RadError> {
        let str_value = radon_trim(input);
        i128::from_str(&str_value)
            .map(RadonInteger::from)
            .map_err(Into::into)
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use crate::types::{array::RadonArray, bytes::RadonBytes};

    use super::*;

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
            description: "expected value at line 1 column 13".to_string(),
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
            description: "expected value at line 1 column 13".to_string(),
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

        assert_eq!(as_integer(&rad_string, &None).unwrap(), rad_int);
    }

    #[test]
    fn test_string_to_integer_with_separators() {
        let rad_integer = RadonInteger::from(1234);

        // English style numbers, i.e. commas for thousands and dots for decimals.
        let rad_string: RadonString = RadonString::from("1234");
        assert_eq!(as_integer(&rad_string, &None).unwrap(), rad_integer);

        // English style numbers, i.e. commas for thousands and dots for decimals.
        let rad_string: RadonString = RadonString::from("1,234");
        assert_eq!(as_integer(&rad_string, &None).unwrap(), rad_integer);

        // Spanish/Italian/German/Norwegian style, i.e. dots for thousands, commas for decimals
        let rad_string: RadonString = RadonString::from("1.234");
        assert_eq!(
            as_integer(
                &rad_string,
                &Some(vec![serde_cbor::Value::from(String::from(".")),])
            )
            .unwrap(),
            rad_integer
        );

        // Danish/Finnish/French/Canadian/Swedish style, i.e. spaces for thousands, commas for decimals
        let rad_string: RadonString = RadonString::from("1 234");
        assert_eq!(
            as_integer(
                &rad_string,
                &Some(vec![serde_cbor::Value::from(String::from(" ")),])
            )
            .unwrap(),
            rad_integer
        );
    }

    #[test]
    fn test_string_to_float() {
        let rad_float = RadonFloat::from(10.2);
        let rad_string: RadonString = RadonString::from("10.2");

        assert_eq!(as_float(&rad_string, &None).unwrap(), rad_float);
    }

    #[test]
    fn test_string_to_float_with_separators() {
        let rad_float = RadonFloat::from(1234.567);

        // English style numbers, i.e. commas for thousands and dots for decimals.
        let rad_string: RadonString = RadonString::from("1234.567");
        assert_eq!(as_float(&rad_string, &None).unwrap(), rad_float);

        // English style numbers, i.e. commas for thousands and dots for decimals.
        let rad_string: RadonString = RadonString::from("1,234.567");
        assert_eq!(as_float(&rad_string, &None).unwrap(), rad_float);

        // Spanish/Italian/German/Norwegian style, i.e. dots for thousands, commas for decimals
        let rad_string: RadonString = RadonString::from("1234,567");
        assert_eq!(
            as_float(
                &rad_string,
                &Some(vec![
                    serde_cbor::Value::from(String::from(".")),
                    serde_cbor::Value::from(String::from(","))
                ])
            )
            .unwrap(),
            rad_float
        );

        // Spanish/Italian/German/Norwegian style, i.e. dots for thousands, commas for decimals
        let rad_string: RadonString = RadonString::from("1.234,567");
        assert_eq!(
            as_float(
                &rad_string,
                &Some(vec![
                    serde_cbor::Value::from(String::from(".")),
                    serde_cbor::Value::from(String::from(","))
                ])
            )
            .unwrap(),
            rad_float
        );

        // Danish/Finnish/French/Canadian/Swedish style, i.e. spaces for thousands, commas for decimals
        let rad_string: RadonString = RadonString::from("1 234,567");
        assert_eq!(
            as_float(
                &rad_string,
                &Some(vec![
                    serde_cbor::Value::from(String::from(" ")),
                    serde_cbor::Value::from(String::from(","))
                ])
            )
            .unwrap(),
            rad_float
        );
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
    fn test_string_match_mismatched_types_default_string() {
        // Check if the StringMatch operator performs implicit conversions to string
        let mut map: BTreeMap<Value, Value> = BTreeMap::new();
        map.insert(Value::Text("key_array".to_string()), Value::Array(vec![]));
        map.insert(Value::Text("key_bool".to_string()), Value::Bool(true));
        map.insert(Value::Text("key_bytes".to_string()), Value::Bytes(vec![]));
        map.insert(Value::Text("key_float".to_string()), Value::Float(1.0));
        map.insert(Value::Text("key_int".to_string()), Value::Integer(1));
        map.insert(
            Value::Text("key_map".to_string()),
            Value::Map(BTreeMap::from([])),
        );
        map.insert(Value::Text("key_null".to_string()), Value::Null);
        map.insert(
            Value::Text("key_string".to_string()),
            Value::Text("".to_string()),
        );
        let args = vec![Value::Map(map), Value::Text("default_value".to_string())];

        let input_key = RadonString::from("key_array");
        let result = string_match(&input_key, &args);
        assert_eq!(
            result.unwrap_err().to_string(),
            "Failed to decode RadonString from serde_cbor::value::Value"
        );

        let input_key = RadonString::from("key_bool");
        let result = string_match(&input_key, &args);
        assert_eq!(
            result.unwrap_err().to_string(),
            "Failed to decode RadonString from serde_cbor::value::Value"
        );

        let input_key = RadonString::from("key_int");
        let result = string_match(&input_key, &args);
        assert_eq!(
            result.unwrap_err().to_string(),
            "Failed to decode RadonString from serde_cbor::value::Value"
        );

        let input_key = RadonString::from("key_float");
        let result = string_match(&input_key, &args);
        assert_eq!(
            result.unwrap_err().to_string(),
            "Failed to decode RadonString from serde_cbor::value::Value"
        );

        let input_key = RadonString::from("key_map");
        let result = string_match(&input_key, &args);
        assert_eq!(
            result.unwrap_err().to_string(),
            "Failed to decode RadonString from serde_cbor::value::Value"
        );

        let input_key = RadonString::from("key_null");
        let result = string_match(&input_key, &args);
        assert_eq!(
            result.unwrap(),
            RadonTypes::String(RadonString::from("default_value".to_string())),
        );

        let input_key = RadonString::from("key_bytes");
        let result = string_match(&input_key, &args);
        assert_eq!(
            result.unwrap(),
            RadonTypes::String(RadonString::from("".to_string())),
        );

        let input_key = RadonString::from("key_string");
        let result = string_match(&input_key, &args);
        assert_eq!(
            result.unwrap(),
            RadonTypes::String(RadonString::from("".to_string())),
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
    fn test_json_numbers_to_radon_numbers() {
        use serde_json::{value::Number, Value as JsonValue};

        let json = JsonValue::Number(Number::from_f64(2.0).unwrap());
        let resulting_radon = RadonTypes::try_from(json).unwrap();
        let expected_radon = RadonInteger::from(2).into();
        assert_eq!(resulting_radon, expected_radon);

        let json = JsonValue::Number(Number::from_f64(20.0).unwrap());
        let resulting_radon = RadonTypes::try_from(json).unwrap();
        let expected_radon = RadonInteger::from(20).into();
        assert_eq!(resulting_radon, expected_radon);

        let json = JsonValue::Number(Number::from_f64(2_000.0).unwrap());
        let resulting_radon = RadonTypes::try_from(json).unwrap();
        let expected_radon = RadonInteger::from(2_000).into();
        assert_eq!(resulting_radon, expected_radon);

        let json = JsonValue::Number(Number::from_f64(2_000_000.0).unwrap());
        let resulting_radon = RadonTypes::try_from(json).unwrap();
        let expected_radon = RadonInteger::from(2_000_000).into();
        assert_eq!(resulting_radon, expected_radon);

        let json = JsonValue::Number(Number::from_f64(std::f64::consts::PI).unwrap());
        let resulting_radon = RadonTypes::try_from(json).unwrap();
        let expected_radon = RadonFloat::from(std::f64::consts::PI).into();
        assert_eq!(resulting_radon, expected_radon);

        let json = JsonValue::Number(Number::from_f64(1e100).unwrap());
        let resulting_radon = RadonTypes::try_from(json).unwrap();
        let expected_radon = RadonFloat::from(1e100).into();
        assert_eq!(resulting_radon, expected_radon);

        let json = JsonValue::Number(Number::from_f64(4.0).unwrap());
        let resulting_radon = RadonTypes::try_from(json).unwrap();
        let expected_radon = RadonInteger::from(4).into();
        assert_eq!(resulting_radon, expected_radon);

        let json = JsonValue::Number(Number::from_f64(4.1).unwrap());
        let resulting_radon = RadonTypes::try_from(json).unwrap();
        let expected_radon = RadonFloat::from(4.1).into();
        assert_eq!(resulting_radon, expected_radon);
    }

    #[test]
    #[allow(clippy::float_cmp)]
    fn test_json_numbers_to_cbor_numbers_exponent_abs_overflow() {
        // Ensure that converting JSON numbers with a large negative exponent into CBOR numbers
        // does not cause any overflows

        // Parse the number using json::parse because otherwise it is just converted to 0.0
        let number: serde_json::Number = serde_json::from_str("0.1E-99999").unwrap();
        // This number is rounded to exactly 0.0 when converted to f64
        assert_eq!(number.as_f64().unwrap(), 0.0);

        // Convert to RadonTypes
        let resulting_radon = RadonTypes::try_from(JsonValue::Number(number)).unwrap();

        // This exponent is too small to fit in a f64, so expected_f64 is equal to 0.0
        let expected_f64 = 0.1E-99999;
        assert_eq!(expected_f64, 0.0);
        // And the expected CBOR value is a float, not an integer
        let expected_radon = RadonFloat::from(expected_f64).into();
        assert_eq!(resulting_radon, expected_radon);
    }

    #[test]
    fn test_replace_separators() {
        // English style numbers, i.e. commas for thousands and dots for decimals.
        assert_eq!(
            replace_separators(
                String::from("1,234.567"),
                String::from(","),
                String::from(".")
            ),
            String::from("1234.567")
        );

        // Spanish/Italian/German/Norwegian style, i.e. dots for thousands, commas for decimals
        assert_eq!(
            replace_separators(
                String::from("1.234,567"),
                String::from("."),
                String::from(",")
            ),
            String::from("1234.567")
        );

        // Danish/Finnish/French/Canadian/Swedish style, i.e. spaces for thousands, commas for decimals
        assert_eq!(
            replace_separators(
                String::from("1 234,567"),
                String::from(" "),
                String::from(",")
            ),
            String::from("1234.567")
        );
    }

    #[test]
    fn test_read_separators_from_args() {
        let args = vec![];
        let separators = read_separators_from_args(&args);
        let expected = (String::from(","), String::from("."));
        assert_eq!(separators, expected);

        let args = vec![Value::from(String::from("x"))];
        let separators = read_separators_from_args(&args);
        let expected = (String::from("x"), String::from("."));
        assert_eq!(separators, expected);

        let args = vec![
            Value::from(String::from("x")),
            Value::from(String::from("y")),
        ];
        let separators = read_separators_from_args(&args);
        let expected = (String::from("x"), String::from("y"));
        assert_eq!(separators, expected);
    }
}
