use std::collections::HashMap;
use std::fmt;

use rmpv::Value;

use witnet_data_structures::serializers::decoders::{TryFrom, TryInto};

use crate::error::*;
use crate::operators::{identity, map as map_operators, Operable, RadonOpCodes};
use crate::script::RadonCall;
use crate::types::RadonTypes;
use crate::types::{mixed::RadonMixed, RadonType};

#[derive(Clone, Debug, PartialEq)]
pub struct RadonMap {
    value: HashMap<String, RadonMixed>,
}

impl<'a> RadonType<'a, HashMap<String, RadonMixed>> for RadonMap {
    fn value(&self) -> HashMap<String, RadonMixed> {
        self.value.clone()
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
            .ok_or_else(|| {
                RadError::new(
                    RadErrorKind::EncodeDecode,
                    String::from("Error creating a RadonMap from a MessagePack value"),
                )
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
        write!(f, "RadonMap")
    }
}

impl Operable for RadonMap {
    fn operate(self, call: &RadonCall) -> RadResult<RadonTypes> {
        match call {
            (RadonOpCodes::Identity, None) => identity(self.into()),
            (RadonOpCodes::MapGet, Some(args)) => {
                let key = args[0].as_str();
                match key {
                    Some(key_str) => map_operators::get(&self, key_str).map(RadonTypes::Mixed),
                    None => Err(WitnetError::from(RadError::new(
                        RadErrorKind::MapKeyNotProvided,
                        format!(
                            "Call to {:?} with args {:?} is not supported on type RadonString",
                            RadonOpCodes::MapGet,
                            args
                        ),
                    ))),
                }
            }
            (op_code, args) => Err(WitnetError::from(RadError::new(
                RadErrorKind::UnsupportedOperator,
                format!(
                    "Call to {:?} with args {:?} is not supported on type RadonString",
                    op_code, args
                ),
            ))),
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

    let result: Result<Vec<u8>, _> = input.try_into();

    let expected_vec: Vec<u8> = vec![129, 164, 90, 101, 114, 111, 0];

    assert!(result.is_ok());
    assert_eq!(expected_vec, result.unwrap());
}

#[test]
fn test_try_from() {
    let slice: &[u8] = &[129, 164, 90, 101, 114, 111, 0];

    let result = RadonMap::try_from(slice);

    let mut map = HashMap::new();
    let value = RadonMixed::from(rmpv::Value::from(0));
    map.insert("Zero".to_string(), value);
    let expected_input = RadonMap::from(map);

    assert!(result.is_ok());
    assert_eq!(expected_input, result.unwrap());
}

#[test]
fn test_operate_map_get() {
    let mut map = HashMap::new();
    let value = RadonMixed::from(rmpv::Value::from(0));
    map.insert("Zero".to_string(), value);
    let input = RadonMap::from(map);

    let call = (RadonOpCodes::MapGet, Some(vec![Value::from("Zero")]));
    let result = input.operate(&call);

    let expected_value = RadonTypes::Mixed(RadonMixed::from(rmpv::Value::from(0)));

    assert!(result.is_ok());
    assert_eq!(expected_value, result.unwrap());
}

#[test]
fn test_operate_map_get_error() {
    let mut map = HashMap::new();
    let value = RadonMixed::from(rmpv::Value::from(0));
    map.insert("Zero".to_string(), value);
    let input = RadonMap::from(map);

    let call = (RadonOpCodes::MapGet, Some(vec![Value::from("NotFound")]));
    let result = input.operate(&call);

    assert!(result.is_err());
}
