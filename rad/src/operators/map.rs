use crate::{
    error::RadError,
    types::{
        array::RadonArray, bytes::RadonBytes, map::RadonMap, string::RadonString, RadonType,
        RadonTypes,
    },
};

use serde_cbor::value::{from_value, Value};

pub fn get(input: &RadonMap, args: &[Value]) -> Result<RadonBytes, RadError> {
    let wrong_args = || RadError::WrongArguments {
        input_type: RadonMap::radon_type_name(),
        operator: "Multiply".to_string(),
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

pub fn keys(input: &RadonMap) -> RadonArray {
    let v: Vec<RadonTypes> = input
        .value()
        .keys()
        .map(|key| RadonTypes::from(RadonString::from(key.to_string())))
        .collect();
    RadonArray::from(v)
}

#[test]
fn test_map_get() {
    use std::collections::HashMap;
    use std::convert::TryFrom;

    let key = "Zero";
    let value = RadonBytes::from(Value::try_from(0).unwrap());
    let args = vec![Value::try_from(String::from(key)).unwrap()];

    let mut map = HashMap::new();
    map.insert(key.to_string(), value.clone());

    let input = RadonMap::from(map);
    let valid_object = get(&input, &args);

    assert!(valid_object.is_ok());
    assert_eq!(value, valid_object.unwrap());
}

#[test]
fn test_map_get_error() {
    use std::collections::HashMap;
    use std::convert::TryFrom;

    let key = "Zero";
    let value = RadonBytes::from(Value::try_from(0).unwrap());
    let args = vec![Value::Text(String::from("NotFound"))];

    let mut map = HashMap::new();
    map.insert(key.to_string(), value.clone());

    let input = RadonMap::from(map);
    let not_found_object = get(&input, &args);

    assert!(not_found_object.is_err());
}

#[test]
fn test_map_keys() {
    use std::collections::HashMap;
    use std::convert::TryFrom;

    let key0 = "Zero";
    let value0 = RadonBytes::from(Value::try_from(0).unwrap());
    let key1 = "One";
    let value1 = RadonBytes::from(Value::try_from(1).unwrap());
    let key2 = "Two";
    let value2 = RadonBytes::from(Value::try_from(2).unwrap());

    let mut map = HashMap::new();
    map.insert(key0.to_string(), value0.clone());
    map.insert(key1.to_string(), value1.clone());
    map.insert(key2.to_string(), value2.clone());

    let input = RadonMap::from(map.clone());
    let keys = keys(&input);

    for key in keys.value() {
        match key {
            RadonTypes::String(rad_string) => assert!(map.contains_key(&rad_string.value())),

            _ => panic!("No RadonString as a key"),
        }
    }
}
