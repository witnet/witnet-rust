use crate::error::RadError;
use crate::types::array::RadonArray;
use crate::types::float::RadonFloat;
use crate::types::map::RadonMap;
use crate::types::mixed::RadonMixed;
use crate::types::string::RadonString;

use rmpv::{decode, encode, Value};
use std::{fmt, io::Cursor};
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

pub trait RadonType<T>:
    fmt::Display + From<T> + PartialEq + TryFrom<Value> + TryInto<Value>
where
    T: fmt::Debug,
{
    fn value(&self) -> T;
}

#[derive(Clone, Debug, PartialEq)]
pub enum RadonTypes {
    Array(RadonArray),
    Float(RadonFloat),
    Map(RadonMap),
    Mixed(RadonMixed),
    String(RadonString),
}

impl RadonTypes {
    pub fn hash(self) -> Result<Hash, RadError> {
        self.try_into()
            .map(|vector: Vec<u8>| calculate_sha256(&*vector))
            .map(Hash::from)
            .map_err(|_| RadError::Hash)
    }
}

impl fmt::Display for RadonTypes {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            RadonTypes::Array(_array) => write!(f, "RadonArray"),
            RadonTypes::Float(_float) => write!(f, "RadonFloat"),
            RadonTypes::Map(_map) => write!(f, "RadonMap"),
            RadonTypes::Mixed(_mixed) => write!(f, "RadonMixed"),
            RadonTypes::String(_string) => write!(f, "RadonString"),
            //_ => write!(f, "RadonTypes::{:?}", self),
        }
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

impl From<RadonMap> for RadonTypes {
    fn from(map: RadonMap) -> Self {
        RadonTypes::Map(map)
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

impl TryFrom<Value> for RadonTypes {
    type Error = RadError;

    fn try_from(value: Value) -> Result<RadonTypes, Self::Error> {
        match value {
            Value::Array(_) => RadonArray::try_from(value).map(Into::into),
            Value::F64(_) => RadonFloat::try_from(value).map(Into::into),
            Value::Map(_) => RadonMap::try_from(value).map(Into::into),
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
            RadonTypes::Map(radon_map) => radon_map.try_into(),
            RadonTypes::Mixed(radon_mixed) => radon_mixed.try_into(),
            RadonTypes::String(radon_string) => radon_string.try_into(),
        }
    }
}

impl TryFrom<&[u8]> for RadonTypes {
    type Error = RadError;

    fn try_from(slice: &[u8]) -> Result<RadonTypes, Self::Error> {
        let mut cursor = Cursor::new(slice);
        let value_result = decode::read_value(&mut cursor);
        let radon_result = value_result.map(RadonTypes::try_from);

        match radon_result {
            Ok(Ok(radon)) => Ok(radon),
            _ => Err(RadError::Decode {
                from: "&[u8]",
                to: "RadonType",
            }),
        }
    }
}

impl TryInto<Vec<u8>> for RadonTypes {
    type Error = RadError;

    fn try_into(self) -> Result<Vec<u8>, Self::Error> {
        let mut cursor = Cursor::new(Vec::new());
        let value_result = self.try_into();
        let result = value_result.map(|value| encode::write_value(&mut cursor, &value));
        let vector = cursor.into_inner();

        match result {
            Ok(Ok(())) => Ok(vector),
            _ => Err(RadError::Decode {
                from: "RadonType",
                to: "Vec<u8>",
            }),
        }
    }
}
