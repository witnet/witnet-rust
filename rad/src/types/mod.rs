use crate::error::*;
use crate::types::array::RadonArray;
use crate::types::float::RadonFloat;
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

impl From<RadonFloat> for RadonTypes {
    fn from(float: RadonFloat) -> Self {
        RadonTypes::Float(float)
    }
}

impl TryFrom<Value> for RadonTypes {
    type Error = RadError;

    fn try_from(value: Value) -> Result<RadonTypes, Self::Error> {
        match value {
            Value::Array(_) => RadonArray::try_from(value).map(Into::into),
            Value::F64(_) => RadonFloat::try_from(value).map(Into::into),
            Value::String(_) => RadonString::try_from(value).map(Into::into),
            _ => RadonMixed::try_from(value).map(Into::into),
        }
    }
}

impl TryInto<Value> for RadonTypes {
    type Error = RadError;

    fn try_into(self) -> Result<Value, Self::Error> {
        match self {
            RadonTypes::Array(radon_array) => radon_array.try_into(),
            RadonTypes::Float(radon_float) => radon_float.try_into(),
            RadonTypes::Mixed(radon_mixed) => radon_mixed.try_into(),
            RadonTypes::String(radon_string) => radon_string.try_into(),
        }
    }
}
