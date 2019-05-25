use std::collections::HashMap;
use std::{
    convert::{TryFrom, TryInto},
    fmt,
};

use rmpv::Value;
use serde::Serialize;

use crate::error::RadError;
use crate::operators::{identity, map as map_operators, Operable, RadonOpCodes};
use crate::script::RadonCall;
use crate::types::RadonTypes;
use crate::types::{mixed::RadonMixed, RadonType};

pub const RADON_MAP_TYPE_NAME: &str = "RadonMap";

#[derive(Clone, Debug, PartialEq, Serialize, Default)]
pub struct RadonMap {
    value: HashMap<String, RadonMixed>,
}

impl RadonType<HashMap<String, RadonMixed>> for RadonMap {
    fn value(&self) -> HashMap<String, RadonMixed> {
        self.value.clone()
    }

    fn radon_type_name() -> String {
        RADON_MAP_TYPE_NAME.to_string()
    }
}

impl From<HashMap<String, RadonMixed>> for RadonMap {
    fn from(value: HashMap<String, RadonMixed>) -> Self {
        RadonMap { value }
    }
}

impl TryFrom<Value> for RadonMap {
    type Error = RadError;

    fn try_from(value: Value) -> Result<Self, Self::Error> {
        value
            .as_map()
            .map(|value_map| {
                value_map
                    .iter()
                    .try_fold(HashMap::new(), |mut map, (rmpv_key, rmpv_value)| {
                        let key = rmpv_key.as_str();
                        let value: Option<RadonMixed> =
                            RadonMixed::try_from(rmpv_value.clone()).ok();
                        if let (Some(key), Some(value)) = (key, value) {
                            map.insert(key.to_string(), value);
                            Some(map)
                        } else {
                            None
                        }
                    })
            })
            .unwrap_or(None)
            .map(Self::from)
            .ok_or_else(|| RadError::Decode {
                from: "rmpv::Value".to_string(),
                to: RADON_MAP_TYPE_NAME.to_string(),
            })
    }
}

impl TryInto<Value> for RadonMap {
    type Error = RadError;

    fn try_into(self) -> Result<Value, Self::Error> {
        Ok(Value::from(
            self.value()
                .iter()
                .map(|(key, value)| (Value::from(key.clone()), value.value()))
                .collect::<Vec<(Value, Value)>>(),
        ))
    }
}

impl fmt::Display for RadonMap {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}({:?})", RADON_MAP_TYPE_NAME, self.value)
    }
}

impl Operable for RadonMap {
    fn operate(self, call: &RadonCall) -> Result<RadonTypes, RadError> {
        match call {
            (RadonOpCodes::Identity, None) => identity(self.into()),
            (RadonOpCodes::Get, Some(args)) | (RadonOpCodes::MapGet, Some(args)) => {
                map_operators::get(&self, args.as_slice()).map(Into::into)
            }
            (op_code, args) => Err(RadError::UnsupportedOperator {
                input_type: RADON_MAP_TYPE_NAME.to_string(),
                operator: op_code.to_string(),
                args: args.to_owned(),
            }),
        }
    }
}

#[test]
fn test_operate_identity() {
    let mut map = HashMap::new();
    let value = RadonMixed::from(rmpv::Value::from(0));
    map.insert("Zero".to_string(), value);

    let input = RadonMap::from(map.clone());
    let expected = RadonMap::from(map).into();

    let call = (RadonOpCodes::Identity, None);
    let output = input.operate(&call).unwrap();

    assert_eq!(output, expected);
}

#[test]
fn test_operate_unimplemented() {
    let mut map = HashMap::new();
    let value = RadonMixed::from(rmpv::Value::from(0));
    map.insert("Zero".to_string(), value);

    let input = RadonMap::from(map);

    let call = (RadonOpCodes::Fail, None);
    let result = input.operate(&call);

    assert!(if let Err(_error) = result {
        true
    } else {
        false
    });
}

#[test]
fn test_try_into() {
    let mut map = HashMap::new();
    let value = RadonMixed::from(rmpv::Value::from(0));
    map.insert("Zero".to_string(), value);
    let input = RadonMap::from(map);

    let result: Vec<u8> = RadonTypes::from(input).try_into().unwrap();

    let expected_vec: Vec<u8> = vec![129, 164, 90, 101, 114, 111, 0];

    assert_eq!(result, expected_vec);
}

#[test]
fn test_try_from() {
    let slice: &[u8] = &[129, 164, 90, 101, 114, 111, 0];

    let result = RadonTypes::try_from(slice).unwrap();

    let mut map = HashMap::new();
    let value = RadonMixed::from(rmpv::Value::from(0));
    map.insert("Zero".to_string(), value);
    let expected_input = RadonTypes::from(RadonMap::from(map));

    assert_eq!(result, expected_input);
}

#[test]
fn test_operate_map_get() {
    let mut map = HashMap::new();
    let value = RadonMixed::from(rmpv::Value::from(0));
    map.insert("Zero".to_string(), value);
    let input = RadonMap::from(map);

    let call = (RadonOpCodes::Get, Some(vec![Value::from("Zero")]));
    let result = input.operate(&call).unwrap();

    let expected_value = RadonTypes::Mixed(RadonMixed::from(rmpv::Value::from(0)));

    assert_eq!(result, expected_value);
}

#[test]
fn test_operate_map_get_error() {
    let mut map = HashMap::new();
    let value = RadonMixed::from(rmpv::Value::from(0));
    map.insert("Zero".to_string(), value);
    let input = RadonMap::from(map);

    let call = (RadonOpCodes::Get, Some(vec![Value::from("NotFound")]));
    let result = input.operate(&call);

    assert!(result.is_err());
}
