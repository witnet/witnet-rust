use crate::error::*;
use crate::types::map::RadonMap;
use crate::types::{mixed::RadonMixed, RadonType};

pub fn get(input: &RadonMap, key: &str) -> RadResult<RadonMixed> {
    match input.value().get(key) {
        Some(value) => Ok(value.clone()),
        None => Err(WitnetError::from(RadError::new(
            RadErrorKind::MapKeyNotFound,
            String::from("Failed to get key from RadonMap"),
        ))),
    }
}

#[test]
fn test_map_get() {
    use std::collections::HashMap;

    let key = "Zero";
    let value = RadonMixed::from(rmpv::Value::from(0));

    let mut map = HashMap::new();
    map.insert(key.to_string(), value.clone());

    let input = RadonMap::from(map);
    let valid_object = get(&input, key);

    assert!(valid_object.is_ok());
    assert_eq!(value, valid_object.unwrap());
}

#[test]
fn test_map_get_error() {
    use std::collections::HashMap;

    let key = "Zero";
    let value = RadonMixed::from(rmpv::Value::from(0));

    let mut map = HashMap::new();
    map.insert(key.to_string(), value.clone());

    let input = RadonMap::from(map);
    let not_found_object = get(&input, "NotFound");

    assert!(not_found_object.is_err());
}
