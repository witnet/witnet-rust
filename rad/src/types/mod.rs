use crate::error::*;
use crate::types::array::RadonArray;
use crate::types::float::RadonFloat;
use crate::types::map::RadonMap;
use crate::types::mixed::RadonMixed;
use crate::types::string::RadonString;

use rmpv::Value;
use std::fmt;
use witnet_crypto::hash::calculate_sha256;
use witnet_data_structures::{
    chain::Hash,
    serializers::decoders::{TryFrom, TryInto},
};

pub mod array;
pub mod float;
pub mod map;
pub mod mixed;
pub mod string;

pub trait RadonType<'a, T>:
    fmt::Display
    + From<T>
    + PartialEq
    + TryFrom<&'a [u8]>
    + TryInto<Vec<u8>>
    + TryFrom<Value>
    + TryInto<Value>
where
    T: fmt::Debug,
{
    fn value(&self) -> T;

    fn hash(self) -> RadResult<Hash> {
        self.try_into()
            .map(|vector: Vec<u8>| calculate_sha256(&*vector))
            .map(Hash::from)
            .map_err(|_| {
                WitnetError::from(RadError::new(
                    RadErrorKind::Hash,
                    String::from("Failed to hash RADON value or structure"),
                ))
            })
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum RadonTypes {
    Array(RadonArray),
    Float(RadonFloat),
    Mixed(RadonMixed),
    String(RadonString),
    Map(RadonMap),
}

impl<'a> From<RadonFloat> for RadonTypes {
    fn from(float: RadonFloat) -> Self {
        RadonTypes::Float(float)
    }
}

impl From<RadonMixed> for RadonTypes {
    fn from(mixed: RadonMixed) -> Self {
        RadonTypes::Mixed(mixed)
    }
}

impl From<RadonString> for RadonTypes {
    fn from(string: RadonString) -> Self {
        RadonTypes::String(string)
    }
}

impl From<RadonArray> for RadonTypes {
    fn from(array: RadonArray) -> Self {
        RadonTypes::Array(array)
    }
}

impl From<RadonMap> for RadonTypes {
    fn from(map: RadonMap) -> Self {
        RadonTypes::Map(map)
    }
}

impl TryFrom<Value> for RadonTypes {
    type Error = RadError;

    fn try_from(value: Value) -> Result<RadonTypes, Self::Error> {
        match value {
            Value::String(_) => RadonString::try_from(value).map(RadonTypes::String),
            Value::Array(_) => RadonArray::try_from(value).map(RadonTypes::Array),
            _ => RadonMixed::try_from(value).map(RadonTypes::Mixed),
        }
    }
}

impl TryInto<Value> for RadonTypes {
    type Error = RadError;

    fn try_into(self) -> Result<Value, Self::Error> {
        match self {
            RadonTypes::Mixed(radon_mixed) => radon_mixed.try_into(),
            RadonTypes::String(radon_string) => radon_string.try_into(),
            RadonTypes::Array(radon_array) => radon_array.try_into(),
            RadonTypes::Float(radon_float) => radon_float.try_into(),
            RadonTypes::Map(radon_map) => radon_map.try_into(),
        }
    }
}
