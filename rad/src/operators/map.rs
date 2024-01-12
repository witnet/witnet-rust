use std::{collections::BTreeMap, convert::TryInto};

use serde_cbor::value::{from_value, Value};
use witnet_data_structures::radon_report::ReportContext;

use crate::{
    error::RadError,
    operators::string,
    script::{
        execute_radon_script, partial_results_extract, unpack_subscript,
        RadonScriptExecutionSettings,
    },
    types::{array::RadonArray, map::RadonMap, string::RadonString, RadonType, RadonTypes},
};

pub fn alter(
    input: &RadonMap,
    args: &[Value],
    context: &mut ReportContext<RadonTypes>,
) -> Result<RadonMap, RadError> {
    let wrong_args = || RadError::WrongArguments {
        input_type: RadonMap::radon_type_name(),
        operator: "Alter".to_string(),
        args: args.to_vec(),
    };

    let first_arg = args.get(0).ok_or_else(wrong_args)?;
    let mut input_keys = vec![];
    match first_arg {
        Value::Array(keys) => {
            for key in keys.iter() {
                let key_string = from_value::<String>(key.to_owned()).map_err(|_| wrong_args())?;
                input_keys.push(key_string);
            }
        }
        Value::Text(key) => {
            input_keys.push(key.clone());
        }
        _ => return Err(wrong_args()),
    };

    let subscript = args.get(1).ok_or_else(wrong_args)?;
    match subscript {
        Value::Array(_arg) => {
            let subscript_err = |e| RadError::Subscript {
                input_type: "RadonMap".to_string(),
                operator: "Alter".to_string(),
                inner: Box::new(e),
            };
            let subscript = unpack_subscript(subscript).map_err(subscript_err)?;

            let not_found = |key_str: &str| RadError::MapKeyNotFound {
                key: String::from(key_str),
            };

            let input_map = input.value();
            let mut output_map = input.value().clone();
            let mut reports = vec![];

            let settings = RadonScriptExecutionSettings::tailored_to_stage(&context.stage);
            for key in input_keys {
                let value = input_map
                    .get(key.as_str())
                    .ok_or_else(|| not_found(key.as_str()))?;
                let report =
                    execute_radon_script(value.clone(), subscript.as_slice(), context, settings)?;
                // If there is an error while altering value, short-circuit and bubble up the error as it comes
                // from the radon script execution
                if let RadonTypes::RadonError(error) = &report.result {
                    return Err(error.clone().into_inner());
                } else {
                    output_map.insert(key, report.result.clone());
                }
                reports.push(report);
            }

            // Extract the partial results from the reports and put them in the execution context if needed
            partial_results_extract(&subscript, &reports, context);

            Ok(RadonMap::from(output_map))
        }
        _ => Err(wrong_args()),
    }
}

fn inner_get(input: &RadonMap, args: &[Value]) -> Result<RadonTypes, RadError> {
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

/// Try to get any kind of `RadonType` from an entry in the input `RadonArray`, as specified
/// by the first argument, which is used as the search key.
pub fn get<O: RadonType<T>, T>(input: &RadonMap, args: &[Value]) -> Result<O, RadError>
where
    T: std::fmt::Debug,
{
    let item = inner_get(input, args)?;
    let original_type = item.radon_type_name();

    item.try_into().map_err(|_| RadError::Decode {
        from: original_type,
        to: O::radon_type_name(),
    })
}

/// Try to get a `RadonFloat` or  `RadonInteger` from an entry in the input `RadonMap`, as specified
/// by the first argument, which is used as the search key. Internally does some pre-processing
/// to normalize decimal and thousands separators.
pub fn get_number<O>(input: &RadonMap, args: &[Value]) -> Result<O, RadError>
where
    O: TryFrom<RadonString, Error = RadError>,
{
    get_numeric_string(input, args).and_then(O::try_from)
}

/// Try to get a `RadonTypes` from an entry in the input `RadonMap`, as specified by the first
/// argument, which is used as the search key.
///
/// This simply assumes that the element in that position is a number (i.e., `RadonFloat` or
/// `RadonInteger`). If it is not, it will fail with a `RadError` because of `replace_separators`.
fn get_numeric_string(input: &RadonMap, args: &[Value]) -> Result<RadonString, RadError> {
    let item = get::<RadonString, _>(input, &args[..1])?.value();
    let (thousands_separator, decimal_separator) = string::read_separators_from_args(&args[1..]);

    Ok(RadonString::from(string::replace_separators(
        item,
        thousands_separator,
        decimal_separator,
    )))
}

pub fn keys(input: &RadonMap) -> RadonArray {
    let v: Vec<RadonTypes> = input
        .value()
        .keys()
        .map(|key| RadonTypes::from(RadonString::from(key.to_string())))
        .collect();
    RadonArray::from(v)
}

pub fn values(input: &RadonMap) -> RadonArray {
    let v: Vec<RadonTypes> = input.value().values().cloned().collect();
    RadonArray::from(v)
}

pub fn pick(input: &RadonMap, args: &[Value]) -> Result<RadonMap, RadError> {
    let not_found = |key_str: &str| RadError::MapKeyNotFound {
        key: String::from(key_str),
    };

    let wrong_args = || RadError::WrongArguments {
        input_type: RadonMap::radon_type_name(),
        operator: "Pick".to_string(),
        args: args.to_vec(),
    };

    let mut input_keys = vec![];
    if args.len() > 1 {
        return Err(wrong_args());
    } else {
        let first_arg = args.get(0).ok_or_else(wrong_args)?;
        match first_arg {
            Value::Array(keys) => {
                for key in keys.iter() {
                    let key_string =
                        from_value::<String>(key.to_owned()).map_err(|_| wrong_args())?;
                    input_keys.push(key_string);
                }
            }
            Value::Text(key) => {
                input_keys.push(key.clone());
            }
            _ => return Err(wrong_args()),
        };
    }

    let mut output_map = BTreeMap::<String, RadonTypes>::default();
    for key in input_keys {
        if let Some(value) = input.value().get(&key) {
            output_map.insert(key, value.clone());
        } else {
            return Err(not_found(key.as_str()));
        }
    }
    Ok(RadonMap::from(output_map))
}

pub fn stringify(input: &RadonMap) -> Result<RadonString, RadError> {
    let json_string = serde_json::to_string(&input.value()).map_err(|_| RadError::Decode {
        from: "RadonMap",
        to: "RadonString",
    })?;
    Ok(RadonString::from(json_string))
}

/// This module was introduced for encapsulating the interim legacy logic before WIP-0024 is
/// introduced, for the sake of maintainability.
///
/// Because RADON scripts are never evaluated for old blocks (e.g. during synchronization), this
/// module can theoretically be removed altogether once WIP-0024 is activated.
pub mod legacy {
    use crate::types::{float::RadonFloat, integer::RadonInteger};

    use super::*;

    /// Legacy (pre-WIP0024) version of `get::<RadonFloat, _>`.
    pub fn get_float_before_wip0024(
        input: &RadonMap,
        args: &[Value],
    ) -> Result<RadonFloat, RadError> {
        let item = inner_get(input, args)?;
        item.try_into().map_err(|_| RadError::ParseFloat {
            message: "invalid float literal".to_string(),
        })
    }

    /// Legacy (pre-WIP0024) version of `get::<RadonInteger, _>`.
    pub fn get_integer_before_wip0024(
        input: &RadonMap,
        args: &[Value],
    ) -> Result<RadonInteger, RadError> {
        let item = inner_get(input, args)?;
        item.try_into().map_err(|_| RadError::ParseInt {
            message: "invalid digit found in string".to_string(),
        })
    }
}

#[cfg(test)]
mod tests {
    use std::{collections::BTreeMap, convert::TryFrom};

    use crate::{
        operators::{Operable, RadonOpCodes},
        types::{
            boolean::RadonBoolean, bytes::RadonBytes, float::RadonFloat, integer::RadonInteger,
        },
    };

    use super::*;

    #[test]
    fn test_map_get() {
        let key = "Zero";
        let value = RadonTypes::Integer(RadonInteger::from(0));
        let args = vec![Value::try_from(String::from(key)).unwrap()];

        let mut map = BTreeMap::new();
        map.insert(key.to_string(), value.clone());

        let input = RadonMap::from(map);
        let valid_object = inner_get(&input, &args);

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
        let not_found_object = inner_get(&input, &args);

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
        let output = get::<RadonArray, _>(&input, &[Value::Text(index)]).unwrap();
        assert_eq!(output, item);
    }

    #[test]
    fn test_map_get_array_fail() {
        let (input, index, _item) = radon_map_of_floats();
        let output = get::<RadonArray, _>(&input, &[Value::Text(index)]).unwrap_err();
        let expected_err = RadError::Decode {
            from: RadonFloat::radon_type_name(),
            to: RadonArray::radon_type_name(),
        };
        assert_eq!(output, expected_err);
    }

    #[test]
    fn test_map_get_boolean() {
        let (input, index, item) = radon_map_of_booleans();
        let output = get::<RadonBoolean, _>(&input, &[Value::Text(index)]).unwrap();
        assert_eq!(output, item);
    }

    #[test]
    fn test_map_get_boolean_fail() {
        let (input, index, _item) = radon_map_of_floats();
        let output = get::<RadonBoolean, _>(&input, &[Value::Text(index)]).unwrap_err();
        let expected_err = RadError::Decode {
            from: RadonFloat::radon_type_name(),
            to: RadonBoolean::radon_type_name(),
        };
        assert_eq!(output, expected_err);
    }

    #[test]
    fn test_map_get_bytes() {
        let (input, index, item) = radon_map_of_bytes();
        let output = get::<RadonBytes, _>(&input, &[Value::Text(index)]).unwrap();
        assert_eq!(output, item);
    }

    #[test]
    fn test_map_get_bytes_fail() {
        let (input, index, _item) = radon_map_of_floats();
        let output = get::<RadonBytes, _>(&input, &[Value::Text(index)]).unwrap_err();
        let expected_err = RadError::Decode {
            from: RadonFloat::radon_type_name(),
            to: RadonBytes::radon_type_name(),
        };
        assert_eq!(output, expected_err);
    }

    #[test]
    fn test_map_get_integer() {
        let (input, index, item) = radon_map_of_integers();
        let output = get_number::<RadonInteger>(&input, &[Value::Text(index)]).unwrap();
        assert_eq!(output, item);
    }

    #[test]
    fn test_map_get_integer_fail() {
        let (input, index, _item) = radon_map_of_booleans();
        let output = get_number::<RadonInteger>(&input, &[Value::Text(index)]).unwrap_err();
        let expected_err = RadError::ParseInt {
            message: "invalid digit found in string".to_string(),
        };
        assert_eq!(output, expected_err);
    }

    #[test]
    fn test_map_get_float() {
        let (input, index, item) = radon_map_of_floats();
        let output = get_number::<RadonFloat>(&input, &[Value::Text(index)]).unwrap();
        assert_eq!(output, item);
    }

    #[test]
    fn test_map_get_float_fail() {
        let (input, index, _item) = radon_map_of_booleans();
        let output = get_number::<RadonFloat>(&input, &[Value::Text(index)]).unwrap_err();
        let expected_err = RadError::ParseFloat {
            message: "invalid float literal".to_string(),
        };
        assert_eq!(output, expected_err);
    }

    #[test]
    fn test_map_get_map() {
        let (input, index, item) = radon_map_of_maps();
        let output = get::<RadonMap, _>(&input, &[Value::Text(index)]).unwrap();
        assert_eq!(output, item);
    }

    #[test]
    fn test_map_get_map_fail() {
        let (input, index, _item) = radon_map_of_booleans();
        let output = get::<RadonMap, _>(&input, &[Value::Text(index)]).unwrap_err();
        let expected_err = RadError::Decode {
            from: RadonBoolean::radon_type_name(),
            to: RadonMap::radon_type_name(),
        };
        assert_eq!(output, expected_err);
    }

    #[test]
    fn test_map_get_string() {
        let (input, index, item) = radon_map_of_strings();
        let output = get::<RadonString, _>(&input, &[Value::Text(index)]).unwrap();
        assert_eq!(output, item);
    }

    #[test]
    fn test_map_get_string_fail() {
        let (input, index, _item) = radon_map_of_arrays();
        let output = get::<RadonString, _>(&input, &[Value::Text(index)]).unwrap_err();
        let expected_err = RadError::Decode {
            from: RadonArray::radon_type_name(),
            to: RadonString::radon_type_name(),
        };
        assert_eq!(output, expected_err);
    }

    #[test]
    fn test_get_float_with_separators() {
        let expected = RadonTypes::from(RadonFloat::from(1234.567));

        // English style numbers, i.e. commas for thousands and dots for decimals.
        let map = RadonMap::from(BTreeMap::from([(
            String::from("foo"),
            RadonTypes::from(RadonString::from("1234.567")),
        )]));
        let output = map
            .operate(&(
                RadonOpCodes::MapGetFloat,
                Some(vec![serde_cbor::Value::from(String::from("foo"))]),
            ))
            .unwrap();
        assert_eq!(output, expected);

        // English style numbers, i.e. commas for thousands and dots for decimals.
        let map = RadonMap::from(BTreeMap::from([(
            String::from("foo"),
            RadonTypes::from(RadonString::from("1,234.567")),
        )]));
        let output = map
            .operate(&(
                RadonOpCodes::MapGetFloat,
                Some(vec![serde_cbor::Value::from(String::from("foo"))]),
            ))
            .unwrap();
        assert_eq!(output, expected);

        // Spanish/Italian/German/Norwegian style, i.e. dots for thousands, commas for decimals
        let map = RadonMap::from(BTreeMap::from([(
            String::from("foo"),
            RadonTypes::from(RadonString::from("1234,567")),
        )]));
        let output = map
            .operate(&(
                RadonOpCodes::MapGetFloat,
                Some(vec![
                    serde_cbor::Value::from(String::from("foo")),
                    serde_cbor::Value::from(String::from(".")),
                    serde_cbor::Value::from(String::from(",")),
                ]),
            ))
            .unwrap();
        assert_eq!(output, expected);

        // Spanish/Italian/German/Norwegian style, i.e. dots for thousands, commas for decimals
        let map = RadonMap::from(BTreeMap::from([(
            String::from("foo"),
            RadonTypes::from(RadonString::from("1.234,567")),
        )]));
        let output = map
            .operate(&(
                RadonOpCodes::MapGetFloat,
                Some(vec![
                    serde_cbor::Value::from(String::from("foo")),
                    serde_cbor::Value::from(String::from(".")),
                    serde_cbor::Value::from(String::from(",")),
                ]),
            ))
            .unwrap();
        assert_eq!(output, expected);

        // Danish/Finnish/French/Canadian/Swedish style, i.e. spaces for thousands, commas for decimals
        let map = RadonMap::from(BTreeMap::from([(
            String::from("foo"),
            RadonTypes::from(RadonString::from("1 234,567")),
        )]));
        let output = map
            .operate(&(
                RadonOpCodes::MapGetFloat,
                Some(vec![
                    serde_cbor::Value::from(String::from("foo")),
                    serde_cbor::Value::from(String::from(" ")),
                    serde_cbor::Value::from(String::from(",")),
                ]),
            ))
            .unwrap();
        assert_eq!(output, expected);
    }

    #[test]
    fn test_get_integer_with_separators() {
        let expected = RadonTypes::from(RadonInteger::from(1234));

        // English style numbers, i.e. commas for thousands and dots for decimals.
        let map = RadonMap::from(BTreeMap::from([(
            String::from("foo"),
            RadonTypes::from(RadonString::from("1234")),
        )]));
        let output = map
            .operate(&(
                RadonOpCodes::MapGetInteger,
                Some(vec![serde_cbor::Value::from(String::from("foo"))]),
            ))
            .unwrap();
        assert_eq!(output, expected);

        // Spanish/Italian/German/Norwegian style, i.e. dots for thousands, commas for decimals
        let map = RadonMap::from(BTreeMap::from([(
            String::from("foo"),
            RadonTypes::from(RadonString::from("1.234")),
        )]));
        let output = map
            .operate(&(
                RadonOpCodes::MapGetInteger,
                Some(vec![
                    serde_cbor::Value::from(String::from("foo")),
                    serde_cbor::Value::from(String::from(".")),
                ]),
            ))
            .unwrap();
        assert_eq!(output, expected);

        // Danish/Finnish/French/Canadian/Swedish style, i.e. spaces for thousands, commas for decimals
        let map = RadonMap::from(BTreeMap::from([(
            String::from("foo"),
            RadonTypes::from(RadonString::from("1 234")),
        )]));
        let output = map
            .operate(&(
                RadonOpCodes::MapGetInteger,
                Some(vec![
                    serde_cbor::Value::from(String::from("foo")),
                    serde_cbor::Value::from(String::from(" ")),
                ]),
            ))
            .unwrap();
        assert_eq!(output, expected);
    }
}
