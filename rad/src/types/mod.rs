use std::{
    convert::{TryFrom, TryInto},
    fmt,
};

use serde::Serialize;
use serde_cbor::{from_slice, to_vec, Value};

use witnet_crypto::hash::calculate_sha256;
use witnet_data_structures::chain::Hash;

use crate::error::RadError;
use crate::types::array::RadonArray;
use crate::types::boolean::RadonBoolean;
use crate::types::bytes::RadonBytes;
use crate::types::float::RadonFloat;
use crate::types::map::RadonMap;
use crate::types::string::RadonString;

pub mod array;
pub mod boolean;
pub mod bytes;
pub mod float;
pub mod map;
pub mod string;

pub trait RadonType<T>:
    fmt::Display + From<T> + PartialEq + TryFrom<Value> + TryInto<Value>
where
    T: fmt::Debug,
{
    fn value(&self) -> T;
    fn radon_type_name() -> String;
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub enum RadonTypes {
    Array(RadonArray),
    Boolean(RadonBoolean),
    Float(RadonFloat),
    Map(RadonMap),
    Bytes(RadonBytes),
    String(RadonString),
}

impl RadonTypes {
    pub fn hash(self) -> Result<Hash, RadError> {
        self.try_into()
            .map(|vector: Vec<u8>| calculate_sha256(&*vector))
            .map(Hash::from)
            .map_err(|_| RadError::Hash)
    }

    pub fn radon_type_name(self) -> String {
        match self {
            RadonTypes::Array(_) => RadonArray::radon_type_name(),
            RadonTypes::Boolean(_) => RadonBoolean::radon_type_name(),
            RadonTypes::Float(_) => RadonFloat::radon_type_name(),
            RadonTypes::Map(_) => RadonMap::radon_type_name(),
            RadonTypes::Bytes(_) => RadonBytes::radon_type_name(),
            RadonTypes::String(_) => RadonString::radon_type_name(),
        }
    }
}

impl fmt::Display for RadonTypes {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            RadonTypes::Array(inner) => write!(f, "RadonTypes::{}", inner),
            RadonTypes::Boolean(inner) => write!(f, "RadonTypes::{}", inner),
            RadonTypes::Float(inner) => write!(f, "RadonTypes::{}", inner),
            RadonTypes::Map(inner) => write!(f, "RadonTypes::{}", inner),
            RadonTypes::Bytes(inner) => write!(f, "RadonTypes::{}", inner),
            RadonTypes::String(inner) => write!(f, "RadonTypes::{}", inner),
        }
    }
}

impl From<RadonArray> for RadonTypes {
    fn from(array: RadonArray) -> Self {
        RadonTypes::Array(array)
    }
}

impl From<RadonBoolean> for RadonTypes {
    fn from(boolean: RadonBoolean) -> Self {
        RadonTypes::Boolean(boolean)
    }
}

impl From<RadonFloat> for RadonTypes {
    fn from(float: RadonFloat) -> Self {
        RadonTypes::Float(float)
    }
}

impl From<RadonMap> for RadonTypes {
    fn from(map: RadonMap) -> Self {
        RadonTypes::Map(map)
    }
}

impl From<RadonBytes> for RadonTypes {
    fn from(bytes: RadonBytes) -> Self {
        RadonTypes::Bytes(bytes)
    }
}

impl From<RadonString> for RadonTypes {
    fn from(string: RadonString) -> Self {
        RadonTypes::String(string)
    }
}

impl TryFrom<Value> for RadonTypes {
    type Error = RadError;

    fn try_from(value: Value) -> Result<RadonTypes, Self::Error> {
        match value {
            Value::Array(_) => RadonArray::try_from(value).map(Into::into),
            Value::Bool(_) => RadonBoolean::try_from(value).map(Into::into),
            Value::Float(_) => RadonFloat::try_from(value).map(Into::into),
            Value::Map(_) => RadonMap::try_from(value).map(Into::into),
            Value::Text(_) => RadonString::try_from(value).map(Into::into),
            _ => Ok(RadonBytes::from(value).into()),
        }
    }
}

impl TryInto<Value> for RadonTypes {
    type Error = RadError;

    fn try_into(self) -> Result<Value, Self::Error> {
        match self {
            RadonTypes::Array(radon_array) => radon_array.try_into(),
            RadonTypes::Boolean(radon_boolean) => radon_boolean.try_into(),
            RadonTypes::Float(radon_float) => radon_float.try_into(),
            RadonTypes::Map(radon_map) => radon_map.try_into(),
            RadonTypes::Bytes(radon_bytes) => radon_bytes.try_into(),
            RadonTypes::String(radon_string) => radon_string.try_into(),
        }
    }
}

impl TryFrom<&[u8]> for RadonTypes {
    type Error = RadError;

    fn try_from(slice: &[u8]) -> Result<RadonTypes, Self::Error> {
        let error = |_| RadError::Decode {
            from: "&[u8]".to_string(),
            to: "RadonType".to_string(),
        };

        let value: Value = from_slice(slice).map_err(error)?;

        RadonTypes::try_from(value)
    }
}

impl TryInto<Vec<u8>> for RadonTypes {
    type Error = RadError;

    fn try_into(self) -> Result<Vec<u8>, Self::Error> {
        let value: Value = self.clone().try_into()?;

        to_vec(&value).map_err(|_| RadError::Decode {
            from: self.radon_type_name(),
            to: "Vec<u8>".to_string(),
        })
    }
}
