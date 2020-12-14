use serde_cbor::value::{from_value, Value};
use std::convert::TryInto;

use crate::{
    error::RadError,
    types::{
        array::RadonArray, boolean::RadonBoolean, bytes::RadonBytes, float::RadonFloat,
        integer::RadonInteger, map::RadonMap, string::RadonString, RadonType, RadonTypes,
    },
};

pub fn get(input: &RadonMap, args: &[Value]) -> Result<RadonTypes, RadError> {
    let wrong_args = || RadError::WrongArguments {
        input_type: RadonMap::radon_type_name(),
        operator: "Get".to_string(),
        args: args.to_vec(),
    };
    let not_found = |key: String| RadError::MapKeyNotFound { key };

    let arg = args.first().ok_or_else(wrong_args)?.to_owned();
    let key = from_value::<String>(arg).map_err(|_| wrong_args())?;

    input
        .value()
        .get(&key)
        .map(Clone::clone)
        .ok_or_else(|| not_found(key))
}
pub fn get_array(input: &RadonMap, args: &[Value]) -> Result<RadonArray, RadError> {
    let item = get(input, args)?;
    item.try_into()
}
pub fn get_boolean(input: &RadonMap, args: &[Value]) -> Result<RadonBoolean, RadError> {
    let item = get(input, args)?;
    item.try_into()
}
pub fn get_bytes(input: &RadonMap, args: &[Value]) -> Result<RadonBytes, RadError> {
    let item = get(input, args)?;
    item.try_into()
}

pub fn get_float(input: &RadonMap, args: &[Value]) -> Result<RadonFloat, RadError> {
    get_number(input, args)?.try_into()
}

pub fn get_integer(input: &RadonMap, args: &[Value]) -> Result<RadonInteger, RadError> {
    get_number(input, args)?.try_into()
}

pub fn get_map(input: &RadonMap, args: &[Value]) -> Result<RadonMap, RadError> {
    let item = get(input, args)?;
    item.try_into()
}

fn get_number(input: &RadonMap, args: &[Value]) -> Result<RadonTypes, RadError> {
    let item = get(input, args)?;

    if args.len() == 3 {
        replace_separators(item, args[1].clone(), args[2].clone())
    } else {
        Ok(item)
    }
}

pub fn get_string(input: &RadonMap, args: &[Value]) -> Result<RadonString, RadError> {
    let item = get(input, args)?;
    item.try_into()
}

pub fn keys(input: &RadonMap) -> RadonArray {
    let v: Vec<RadonTypes> = input
        .value()
        .keys()
        .map(|key| RadonTypes::from(RadonString::from(key.to_string())))
        .collect();
    RadonArray::from(v)
}

pub fn _replace_separators(
    value: String,
    thousand_separator: serde_cbor::Value,
    decimal_separator: serde_cbor::Value,
) -> String {
    let thousand = from_value::<String>(thousand_separator).unwrap_or_else(|_| "".to_string());
    let decimal = from_value::<String>(decimal_separator).unwrap_or_else(|_| ".".to_string());

    value.replace(&thousand, "").replace(&decimal, ".")
}

pub fn replace_separators(
    value: RadonTypes,
    thousand_separator: serde_cbor::Value,
    decimal_separator: serde_cbor::Value,
) -> Result<RadonTypes, RadError> {
    let rad_str_value: RadonString = value.try_into()?;

    Ok(RadonTypes::from(RadonString::from(_replace_separators(
        rad_str_value.value(),
        thousand_separator,
        decimal_separator,
    ))))
}

pub fn values(input: &RadonMap) -> RadonArray {
    let v: Vec<RadonTypes> = input.value().values().cloned().collect();
    RadonArray::from(v)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::integer::RadonInteger;
    use std::{collections::BTreeMap, convert::TryFrom};

    #[test]
    fn test_map_get() {
        let key = "Zero";
        let value = RadonTypes::Integer(RadonInteger::from(0));
        let args = vec![Value::try_from(String::from(key)).unwrap()];

        let mut map = BTreeMap::new();
        map.insert(key.to_string(), value.clone());

        let input = RadonMap::from(map);
        let valid_object = get(&input, &args);

        let expected_value = value;

        assert!(valid_object.is_ok());
        assert_eq!(expected_value, valid_object.unwrap());
    }

    #[test]
    fn test_map_get_error() {
        let key = "Zero";
        let value = RadonTypes::Integer(RadonInteger::from(0));
        let args = vec![Value::Text(String::from("NotFound"))];

        let mut map = BTreeMap::new();
        map.insert(key.to_string(), value);

        let input = RadonMap::from(map);
        let not_found_object = get(&input, &args);

        assert!(not_found_object.is_err());
    }

    #[test]
    fn test_map_keys() {
        let key0 = "Zero";
        let value0 = RadonTypes::Integer(RadonInteger::from(0));
        let key1 = "One";
        let value1 = RadonTypes::Integer(RadonInteger::from(1));
        let key2 = "Two";
        let value2 = RadonTypes::Integer(RadonInteger::from(2));

        let mut map = BTreeMap::new();
        map.insert(key0.to_string(), value0);
        map.insert(key1.to_string(), value1);
        map.insert(key2.to_string(), value2);

        let input = RadonMap::from(map);
        let keys = keys(&input);

        // RadonMap::Keys are sorted by key alphabetically
        assert_eq!(
            keys,
            RadonArray::from(vec![
                RadonTypes::String(RadonString::from(key1.to_string())),
                RadonTypes::String(RadonString::from(key2.to_string())),
                RadonTypes::String(RadonString::from(key0.to_string()))
            ])
        );
    }

    #[test]
    fn test_map_values() {
        let key0 = "Zero";
        let value0 = RadonTypes::Integer(RadonInteger::from(0));
        let key1 = "One";
        let value1 = RadonTypes::Integer(RadonInteger::from(1));
        let key2 = "Two";
        let value2 = RadonTypes::Integer(RadonInteger::from(2));

        let mut map = BTreeMap::new();
        map.insert(key0.to_string(), value0.clone());
        map.insert(key1.to_string(), value1.clone());
        map.insert(key2.to_string(), value2.clone());

        let input = RadonMap::from(map);
        let values = values(&input);

        // RadonMap::Values are sorted by key alphabetically
        assert_eq!(values, RadonArray::from(vec![value1, value2, value0]));
    }

    // Auxiliar functions

    fn radon_map_of_arrays() -> (RadonMap, String, RadonArray) {
        let key0 = "Zero".to_string();
        let key1 = "One".to_string();
        let key2 = "Two".to_string();

        let item0 = RadonArray::from(vec![
            RadonTypes::from(RadonInteger::from(1)),
            RadonTypes::from(RadonInteger::from(2)),
            RadonTypes::from(RadonInteger::from(3)),
        ]);
        let item1 = RadonArray::from(vec![
            RadonTypes::from(RadonInteger::from(11)),
            RadonTypes::from(RadonInteger::from(12)),
            RadonTypes::from(RadonInteger::from(13)),
        ]);
        let item2 = RadonArray::from(vec![
            RadonTypes::from(RadonInteger::from(21)),
            RadonTypes::from(RadonInteger::from(22)),
            RadonTypes::from(RadonInteger::from(23)),
        ]);

        let value0 = RadonTypes::Array(item0);
        let value1 = RadonTypes::Array(item1.clone());
        let value2 = RadonTypes::Array(item2);

        let mut map = BTreeMap::new();
        map.insert(key0, value0);
        map.insert(key1.clone(), value1);
        map.insert(key2, value2);

        let output = RadonMap::from(map);

        (output, key1, item1)
    }

    fn radon_map_of_booleans() -> (RadonMap, String, RadonBoolean) {
        let key0 = "Zero".to_string();
        let key1 = "One".to_string();
        let key2 = "Two".to_string();

        let item0 = RadonBoolean::from(false);
        let item1 = RadonBoolean::from(false);
        let item2 = RadonBoolean::from(false);

        let value0 = RadonTypes::Boolean(item0);
        let value1 = RadonTypes::Boolean(item1.clone());
        let value2 = RadonTypes::Boolean(item2);

        let mut map = BTreeMap::new();
        map.insert(key0, value0);
        map.insert(key1.clone(), value1);
        map.insert(key2, value2);

        let output = RadonMap::from(map);

        (output, key1, item1)
    }

    fn radon_map_of_bytes() -> (RadonMap, String, RadonBytes) {
        let key0 = "Zero".to_string();
        let key1 = "One".to_string();
        let key2 = "Two".to_string();

        let item0 = RadonBytes::from(vec![0x01, 0x02, 0x03]);
        let item1 = RadonBytes::from(vec![0x11, 0x12, 0x13]);
        let item2 = RadonBytes::from(vec![0x21, 0x22, 0x23]);

        let value0 = RadonTypes::Bytes(item0);
        let value1 = RadonTypes::Bytes(item1.clone());
        let value2 = RadonTypes::Bytes(item2);

        let mut map = BTreeMap::new();
        map.insert(key0, value0);
        map.insert(key1.clone(), value1);
        map.insert(key2, value2);

        let output = RadonMap::from(map);

        (output, key1, item1)
    }

    fn radon_map_of_integers() -> (RadonMap, String, RadonInteger) {
        let key0 = "Zero".to_string();
        let key1 = "One".to_string();
        let key2 = "Two".to_string();

        let item0 = RadonInteger::from(1);
        let item1 = RadonInteger::from(2);
        let item2 = RadonInteger::from(3);

        let value0 = RadonTypes::Integer(item0);
        let value1 = RadonTypes::Integer(item1.clone());
        let value2 = RadonTypes::Integer(item2);

        let mut map = BTreeMap::new();
        map.insert(key0, value0);
        map.insert(key1.clone(), value1);
        map.insert(key2, value2);

        let output = RadonMap::from(map);

        (output, key1, item1)
    }

    fn radon_map_of_floats() -> (RadonMap, String, RadonFloat) {
        let key0 = "Zero".to_string();
        let key1 = "One".to_string();
        let key2 = "Two".to_string();

        let item0 = RadonFloat::from(1.0);
        let item1 = RadonFloat::from(2.0);
        let item2 = RadonFloat::from(3.0);

        let value0 = RadonTypes::Float(item0);
        let value1 = RadonTypes::Float(item1.clone());
        let value2 = RadonTypes::Float(item2);

        let mut map = BTreeMap::new();
        map.insert(key0, value0);
        map.insert(key1.clone(), value1);
        map.insert(key2, value2);

        let output = RadonMap::from(map);

        (output, key1, item1)
    }

    fn radon_map_of_maps() -> (RadonMap, String, RadonMap) {
        let key0 = "Zero".to_string();
        let key1 = "One".to_string();
        let key2 = "Two".to_string();

        let (item0, _, _) = radon_map_of_floats();
        let (item1, _, _) = radon_map_of_integers();
        let (item2, _, _) = radon_map_of_booleans();

        let value0 = RadonTypes::Map(item0);
        let value1 = RadonTypes::Map(item1.clone());
        let value2 = RadonTypes::Map(item2);

        let mut map = BTreeMap::new();
        map.insert(key0, value0);
        map.insert(key1.clone(), value1);
        map.insert(key2, value2);

        let output = RadonMap::from(map);

        (output, key1, item1)
    }

    fn radon_map_of_strings() -> (RadonMap, String, RadonString) {
        let key0 = "Zero".to_string();
        let key1 = "One".to_string();
        let key2 = "Two".to_string();

        let item0 = RadonString::from("Hello");
        let item1 = RadonString::from("World");
        let item2 = RadonString::from("Rust");

        let value0 = RadonTypes::String(item0);
        let value1 = RadonTypes::String(item1.clone());
        let value2 = RadonTypes::String(item2);

        let mut map = BTreeMap::new();
        map.insert(key0, value0);
        map.insert(key1.clone(), value1);
        map.insert(key2, value2);

        let output = RadonMap::from(map);

        (output, key1, item1)
    }

    #[test]
    fn test_map_get_array() {
        let (input, index, item) = radon_map_of_arrays();
        let output = get_array(&input, &[Value::Text(index)]).unwrap();
        assert_eq!(output, item);
    }

    #[test]
    fn test_map_get_array_fail() {
        let (input, index, _item) = radon_map_of_floats();
        let output = get_array(&input, &[Value::Text(index)]).unwrap_err();
        let expected_err = RadError::Decode {
            from: "cbor::value::Value",
            to: RadonArray::radon_type_name(),
        };
        assert_eq!(output, expected_err);
    }

    #[test]
    fn test_map_get_boolean() {
        let (input, index, item) = radon_map_of_booleans();
        let output = get_boolean(&input, &[Value::Text(index)]).unwrap();
        assert_eq!(output, item);
    }

    #[test]
    fn test_map_get_boolean_fail() {
        let (input, index, _item) = radon_map_of_floats();
        let output = get_boolean(&input, &[Value::Text(index)]).unwrap_err();
        let expected_err = RadError::Decode {
            from: "cbor::value::Value",
            to: RadonBoolean::radon_type_name(),
        };
        assert_eq!(output, expected_err);
    }

    #[test]
    fn test_map_get_bytes() {
        let (input, index, item) = radon_map_of_bytes();
        let output = get_bytes(&input, &[Value::Text(index)]).unwrap();
        assert_eq!(output, item);
    }

    #[test]
    fn test_map_get_bytes_fail() {
        let (input, index, _item) = radon_map_of_floats();
        let output = get_bytes(&input, &[Value::Text(index)]).unwrap_err();
        let expected_err = RadError::Decode {
            from: "cbor::value::Value",
            to: RadonBytes::radon_type_name(),
        };
        assert_eq!(output, expected_err);
    }

    #[test]
    fn test_map_get_integer() {
        let (input, index, item) = radon_map_of_integers();
        let output = get_integer(&input, &[Value::Text(index)]).unwrap();
        assert_eq!(output, item);
    }

    #[test]
    fn test_map_get_integer_fail() {
        let (input, index, _item) = radon_map_of_booleans();
        let output = get_integer(&input, &[Value::Text(index)]).unwrap_err();
        let expected_err = RadError::Decode {
            from: "cbor::value::Value",
            to: RadonInteger::radon_type_name(),
        };
        assert_eq!(output, expected_err);
    }

    #[test]
    fn test_map_get_float() {
        let (input, index, item) = radon_map_of_floats();
        let output = get_float(&input, &[Value::Text(index)]).unwrap();
        assert_eq!(output, item);
    }

    #[test]
    fn test_map_get_float_fail() {
        let (input, index, _item) = radon_map_of_booleans();
        let output = get_float(&input, &[Value::Text(index)]).unwrap_err();
        let expected_err = RadError::Decode {
            from: "cbor::value::Value",
            to: RadonFloat::radon_type_name(),
        };
        assert_eq!(output, expected_err);
    }

    #[test]
    fn test_map_get_map() {
        let (input, index, item) = radon_map_of_maps();
        let output = get_map(&input, &[Value::Text(index)]).unwrap();
        assert_eq!(output, item);
    }

    #[test]
    fn test_map_get_map_fail() {
        let (input, index, _item) = radon_map_of_booleans();
        let output = get_map(&input, &[Value::Text(index)]).unwrap_err();
        let expected_err = RadError::Decode {
            from: "cbor::value::Value",
            to: RadonMap::radon_type_name(),
        };
        assert_eq!(output, expected_err);
    }

    #[test]
    fn test_map_get_string() {
        let (input, index, item) = radon_map_of_strings();
        let output = get_string(&input, &[Value::Text(index)]).unwrap();
        assert_eq!(output, item);
    }

    #[test]
    fn test_map_get_string_fail() {
        let (input, index, _item) = radon_map_of_booleans();
        let output = get_string(&input, &[Value::Text(index)]).unwrap_err();
        let expected_err = RadError::Decode {
            from: "serde_cbor::value::Value",
            to: RadonString::radon_type_name(),
        };
        assert_eq!(output, expected_err);
    }

    #[test]
    fn test_replace_separators() {
        // English format
        assert_eq!(
            replace_separators(
                RadonTypes::String(RadonString::from("1,234.567")),
                Value::from(String::from(",")),
                Value::from(String::from("."))
            )
            .unwrap(),
            RadonTypes::String(RadonString::from("1234.567"))
        );

        // Spanish format
        assert_eq!(
            replace_separators(
                RadonTypes::String(RadonString::from("1.234,567")),
                Value::from(String::from(".")),
                Value::from(String::from(","))
            )
            .unwrap(),
            RadonTypes::String(RadonString::from("1234.567"))
        );
    }
}
