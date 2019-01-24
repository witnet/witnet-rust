use crate::error::*;
use crate::operators::RadonOpCodes;
use crate::types::map::RadonMap;
use crate::types::{mixed::RadonMixed, RadonType};

use rmpv::Value;

pub fn get(input: &RadonMap, args: &[Value]) -> RadResult<RadonMixed> {
    let key = args.first().map(|ref value| value.as_str()).unwrap_or(None);
    match key {
        Some(key_str) => match input.value().get(key_str) {
            Some(value) => Ok(value.clone()),
            None => Err(WitnetError::from(RadError::new(
                RadErrorKind::MapKeyNotFound,
                String::from("Failed to get key from RadonMap"),
            ))),
        },
        None => Err(WitnetError::from(RadError::new(
            RadErrorKind::WrongArguments,
            format!(
                "Call to {:?} with args {:?} is not supported on type RadonString",
                RadonOpCodes::Get,
                args
            ),
        ))),
    }
}

#[test]
fn test_map_get() {
    use std::collections::HashMap;

    let key = "Zero";
    let value = RadonMixed::from(rmpv::Value::from(0));
    let args = vec![Value::from(key)];

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

    let key = "Zero";
    let value = RadonMixed::from(rmpv::Value::from(0));
    let args = vec![Value::from("NotFound")];

    let mut map = HashMap::new();
    map.insert(key.to_string(), value.clone());

    let input = RadonMap::from(map);
    let not_found_object = get(&input, &args);

    assert!(not_found_object.is_err());
}
