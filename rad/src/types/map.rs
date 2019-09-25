use std::collections::HashMap;
use std::{
    convert::{TryFrom, TryInto},
    fmt,
};

use serde::Serialize;
use serde_cbor::value::{from_value, to_value, Value};

use crate::error::RadError;
use crate::operators::{identity, map as map_operators, Operable, RadonOpCodes};
use crate::script::RadonCall;
use crate::types::RadonTypes;
use crate::types::{bytes::RadonBytes, RadonType};
use std::collections::btree_map::BTreeMap;

pub const RADON_MAP_TYPE_NAME: &str = "RadonMap";

#[derive(Clone, Debug, PartialEq, Serialize, Default)]
pub struct RadonMap {
    value: HashMap<String, RadonBytes>,
}

impl RadonType<HashMap<String, RadonBytes>> for RadonMap {
    fn value(&self) -> HashMap<String, RadonBytes> {
        self.value.clone()
    }

    fn radon_type_name() -> String {
        RADON_MAP_TYPE_NAME.to_string()
    }
}

impl From<HashMap<String, RadonBytes>> for RadonMap {
    fn from(value: HashMap<String, RadonBytes>) -> Self {
        RadonMap { value }
    }
}

impl From<BTreeMap<String, RadonBytes>> for RadonMap {
    fn from(value: BTreeMap<String, RadonBytes>) -> Self {
        RadonMap {
            value: value.into_iter().collect(),
        }
    }
}

impl TryFrom<Value> for RadonMap {
    type Error = RadError;

    fn try_from(value: Value) -> Result<Self, Self::Error> {
        let error = || RadError::Decode {
            from: "cbor::value::Value".to_string(),
            to: RADON_MAP_TYPE_NAME.to_string(),
        };

        from_value::<HashMap<String, Value>>(value)
            .map_err(|_| error())?
            .iter()
            .try_fold(
                HashMap::<String, RadonBytes>::new(),
                |mut map, (key, cbor_value)| {
                    if let Ok(value) = RadonBytes::try_from(cbor_value.to_owned()) {
                        map.insert(key.to_string(), value);
                        Some(map)
                    } else {
                        None
                    }
                },
            )
            .map(Self::from)
            .ok_or_else(error)
    }
}

impl TryInto<Value> for RadonMap {
    type Error = RadError;

    fn try_into(self) -> Result<Value, Self::Error> {
        let error = || RadError::Encode {
            from: RADON_MAP_TYPE_NAME.to_string(),
            to: "cbor::value::Value".to_string(),
        };

        let map = self
            .value()
            .iter()
            .try_fold(
                BTreeMap::<Value, Value>::new(),
                |mut map, (key, radon_mixed)| {
                    if let Ok(key) = Value::try_from(key.to_string()) {
                        map.insert(key, radon_mixed.value());
                        Some(map)
                    } else {
                        None
                    }
                },
            )
            .ok_or_else(error)?;

        to_value(map).map_err(|_| error())
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
            (RadonOpCodes::MapKeys, None) => Ok(RadonTypes::from(map_operators::keys(&self))),
            (RadonOpCodes::MapValues, None) => Ok(RadonTypes::from(map_operators::values(&self))),
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
    let value = RadonBytes::from(Value::from(0));
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
    let value = RadonBytes::from(Value::from(0));
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
    let value = RadonBytes::from(Value::from(0));
    map.insert("Zero".to_string(), value);
    let input = RadonMap::from(map);

    let result: Vec<u8> = RadonTypes::from(input).try_into().unwrap();

    let expected_vec: Vec<u8> = vec![161, 100, 90, 101, 114, 111, 0];

    assert_eq!(result, expected_vec);
}

#[test]
fn test_try_from() {
    let slice: &[u8] = &[161, 100, 90, 101, 114, 111, 0];

    let result = RadonTypes::try_from(slice).unwrap();

    let mut map = HashMap::new();
    let value = RadonBytes::from(Value::Integer(0));
    map.insert("Zero".to_string(), value);
    let expected_input = RadonTypes::from(RadonMap::from(map));

    assert_eq!(result, expected_input);
}

#[test]
fn test_operate_map_get() {
    let mut map = HashMap::new();
    let value = RadonBytes::from(Value::Integer(0));
    map.insert("Zero".to_string(), value);
    let input = RadonMap::from(map);

    let call = (
        RadonOpCodes::Get,
        Some(vec![Value::Text(String::from("Zero"))]),
    );
    let result = input.operate(&call).unwrap();

    let expected_value = RadonTypes::Bytes(RadonBytes::from(Value::from(0)));

    assert_eq!(result, expected_value);
}

#[test]
fn test_operate_map_get_error() {
    let mut map = HashMap::new();
    let value = RadonBytes::from(Value::Integer(0));
    map.insert("Zero".to_string(), value);
    let input = RadonMap::from(map);

    let call = (
        RadonOpCodes::Get,
        Some(vec![Value::Text(String::from("NotFound"))]),
    );
    let result = input.operate(&call);

    assert!(result.is_err());
}
