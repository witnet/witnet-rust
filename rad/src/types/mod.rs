use std::collections::BTreeMap;
use std::{
    convert::{TryFrom, TryInto},
    fmt,
};

use serde::Serialize;
use serde_cbor::{to_vec, Value};

use witnet_crypto::hash::calculate_sha256;
use witnet_data_structures::chain::Hash;
use witnet_data_structures::radon_report::TypeLike;

use crate::operators::Operable;
use crate::types::result::RadonResult;
use crate::{
    error::RadError,
    types::{
        array::RadonArray, boolean::RadonBoolean, bytes::RadonBytes, float::RadonFloat,
        integer::RadonInteger, map::RadonMap, string::RadonString,
    },
};

pub mod array;
pub mod boolean;
pub mod bytes;
pub mod float;
pub mod integer;
pub mod map;
pub mod result;
pub mod string;

pub trait RadonType<T>:
    fmt::Display + From<T> + PartialEq + TryFrom<Value> + TryInto<Value>
where
    T: fmt::Debug,
{
    fn value(&self) -> T;
    fn radon_type_name() -> String;
}

#[derive(Clone, Debug, Serialize)]
pub enum RadonTypes {
    Array(RadonArray),
    Boolean(RadonBoolean),
    Bytes(RadonBytes),
    Float(RadonFloat),
    Integer(RadonInteger),
    Map(RadonMap),
    Result(RadonResult),
    String(RadonString),
}

impl RadonTypes {
    pub fn hash(self) -> Result<Hash, RadError> {
        self.encode()
            .map(|vector: Vec<u8>| calculate_sha256(&*vector))
            .map(Hash::from)
            .map_err(|_| RadError::Hash)
    }

    pub fn radon_type_name(&self) -> String {
        match self {
            RadonTypes::Array(_) => RadonArray::radon_type_name(),
            RadonTypes::Boolean(_) => RadonBoolean::radon_type_name(),
            RadonTypes::Bytes(_) => RadonBytes::radon_type_name(),
            RadonTypes::Float(_) => RadonFloat::radon_type_name(),
            RadonTypes::Integer(_) => RadonInteger::radon_type_name(),
            RadonTypes::Map(_) => RadonMap::radon_type_name(),
            RadonTypes::Result(_) => RadonResult::radon_type_name(),
            RadonTypes::String(_) => RadonString::radon_type_name(),
        }
    }

    pub fn as_operable(&self) -> &dyn Operable {
        match self {
            RadonTypes::Array(inner) => inner,
            RadonTypes::Boolean(inner) => inner,
            RadonTypes::Bytes(inner) => inner,
            RadonTypes::Float(inner) => inner,
            RadonTypes::Integer(inner) => inner,
            RadonTypes::Map(inner) => inner,
            RadonTypes::Result(inner) => inner,
            RadonTypes::String(inner) => inner,
        }
    }
}

/// Satisfy the `TypeLike` trait that ensures generic compatibility of `witnet_rad` and
/// `witnet_data_structures`.
impl TypeLike for RadonTypes {
    type Error = RadError;

    fn encode(&self) -> Result<Vec<u8>, Self::Error> {
        Vec::<u8>::try_from(self)
    }
}

impl std::cmp::Eq for RadonTypes {}

// Manually implement PartialEq to ensure
// k1 == k2 ⇒ hash(k1) == hash(k2)
// https://rust-lang.github.io/rust-clippy/master/index.html#derive_hash_xor_eq
impl PartialEq for RadonTypes {
    fn eq(&self, other: &RadonTypes) -> bool {
        let vec1 = self.encode();
        let vec2 = other.encode();

        vec1 == vec2
    }
}

impl std::hash::Hash for RadonTypes {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.encode().map(|vec| vec.hash(state)).unwrap();
    }
}

impl fmt::Display for RadonTypes {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            RadonTypes::Array(inner) => write!(f, "RadonTypes::{}", inner),
            RadonTypes::Boolean(inner) => write!(f, "RadonTypes::{}", inner),
            RadonTypes::Bytes(inner) => write!(f, "RadonTypes::{}", inner),
            RadonTypes::Float(inner) => write!(f, "RadonTypes::{}", inner),
            RadonTypes::Integer(inner) => write!(f, "RadonTypes::{}", inner),
            RadonTypes::Map(inner) => write!(f, "RadonTypes::{}", inner),
            RadonTypes::Result(inner) => write!(f, "RadonTypes::{}", inner),
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

impl From<RadonBytes> for RadonTypes {
    fn from(bytes: RadonBytes) -> Self {
        RadonTypes::Bytes(bytes)
    }
}

impl From<RadonFloat> for RadonTypes {
    fn from(float: RadonFloat) -> Self {
        RadonTypes::Float(float)
    }
}

impl From<RadonInteger> for RadonTypes {
    fn from(integer: RadonInteger) -> Self {
        RadonTypes::Integer(integer)
    }
}

impl From<RadonMap> for RadonTypes {
    fn from(map: RadonMap) -> Self {
        RadonTypes::Map(map)
    }
}

impl From<RadonResult> for RadonTypes {
    fn from(result: RadonResult) -> Self {
        RadonTypes::Result(result)
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
            Value::Integer(_) => RadonInteger::try_from(value).map(Into::into),
            Value::Bytes(_) => Ok(RadonTypes::from(RadonBytes::from(value))),
            Value::Null => Err(RadError::Decode {
                from: String::from("serde_cbor::Value::Null"),
                to: String::from("RadonTypes"),
            }),
            Value::__Hidden => Err(RadError::Decode {
                from: String::from("serde_cbor::Value::__Hidden"),
                to: String::from("RadonTypes"),
            }),
        }
    }
}

impl TryFrom<RadonTypes> for Value {
    type Error = RadError;

    fn try_from(input: RadonTypes) -> Result<Self, Self::Error> {
        match input {
            RadonTypes::Array(radon_array) => radon_array.try_into(),
            RadonTypes::Boolean(radon_boolean) => radon_boolean.try_into(),
            RadonTypes::Bytes(radon_bytes) => radon_bytes.try_into(),
            RadonTypes::Float(radon_float) => radon_float.try_into(),
            RadonTypes::Integer(radon_integer) => radon_integer.try_into(),
            RadonTypes::Map(radon_map) => radon_map.try_into(),
            RadonTypes::Result(radon_result) => radon_result.try_into(),
            RadonTypes::String(radon_string) => radon_string.try_into(),
        }
    }
}

/// Allow CBOR decoding of any variant of `RadonTypes`.
impl TryFrom<&[u8]> for RadonTypes {
    type Error = RadError;

    fn try_from(slice: &[u8]) -> Result<RadonTypes, Self::Error> {
        let mut decoder = cbor::decoder::GenericDecoder::new(
            cbor::Config::default(),
            std::io::Cursor::new(slice),
        );

        let cbor_value = decoder.value()?;

        RadonTypes::try_from(&cbor_value)
    }
}

/// Allow CBOR encoding of any variant of `RadonTypes`.
impl TryFrom<&RadonTypes> for Vec<u8> {
    type Error = RadError;

    fn try_from(radon_types: &RadonTypes) -> Result<Vec<u8>, Self::Error> {
        let type_name = RadonTypes::radon_type_name(radon_types);
        let value: Value = radon_types.clone().try_into()?;

        to_vec(&value).map_err(|_| RadError::Encode {
            from: type_name,
            to: "Vec<u8>".to_string(),
        })
    }
}

// TODO: migrate everything to using `cbor-codec` or wait for `serde_cbor` to support CBOR tags.
/// Allow decoding RADON types also from `Value` structures coming from the `cbor-codec` crate.
/// Take into account the difference between `cbor::value::Value` and `serde_cbor::Value`.
impl TryFrom<&cbor::value::Value> for RadonTypes {
    type Error = RadError;

    fn try_from(cbor_value: &cbor::value::Value) -> Result<Self, Self::Error> {
        use cbor::value::Int;
        use cbor::value::Value as CborValue;
        use serde_cbor::Value as SerdeCborValue;

        match cbor_value {
            // If the tag is 37, we treat the value as a `RadonResult`, otherwise we ignore the tag,
            // unbox the tagged value and decode it through recurrently calling this same function.
            CborValue::Tagged(tag, boxed) => match (tag, std::boxed::Box::leak(boxed.clone())) {
                (cbor::types::Tag::Unassigned(37), CborValue::U8(error_code)) => {
                    Ok(RadonTypes::Result(RadonResult::from(Err(*error_code))))
                }
                (_, other) => RadonTypes::try_from(&*other),
            },
            // Booleans, numbers, strings and bytes are all converted easily.
            CborValue::Bool(x) => Ok(RadonTypes::Boolean(RadonBoolean::from(*x))),
            CborValue::U8(x) => Ok(RadonTypes::Integer(RadonInteger::from(i128::from(*x)))),
            CborValue::U16(x) => Ok(RadonTypes::Integer(RadonInteger::from(i128::from(*x)))),
            CborValue::U32(x) => Ok(RadonTypes::Integer(RadonInteger::from(i128::from(*x)))),
            CborValue::U64(x) => Ok(RadonTypes::Integer(RadonInteger::from(i128::from(*x)))),
            CborValue::I8(x) => Ok(RadonTypes::Integer(RadonInteger::from(i128::from(*x)))),
            CborValue::I16(x) => Ok(RadonTypes::Integer(RadonInteger::from(i128::from(*x)))),
            CborValue::I32(x) => Ok(RadonTypes::Integer(RadonInteger::from(i128::from(*x)))),
            CborValue::I64(x) => Ok(RadonTypes::Integer(RadonInteger::from(i128::from(*x)))),
            CborValue::Int(Int::Neg(x)) => {
                Ok(RadonTypes::Integer(RadonInteger::from(i128::from(*x))))
            }
            CborValue::Int(Int::Pos(x)) => {
                Ok(RadonTypes::Integer(RadonInteger::from(i128::from(*x))))
            }
            CborValue::F32(x) => Ok(RadonTypes::Float(RadonFloat::from(f64::from(*x)))),
            CborValue::F64(x) => Ok(RadonTypes::Float(RadonFloat::from(*x))),
            CborValue::Text(cbor::value::Text::Text(x)) => {
                Ok(RadonTypes::String(RadonString::from(x.clone())))
            }
            CborValue::Bytes(cbor::value::Bytes::Bytes(x)) => Ok(RadonTypes::Bytes(
                RadonBytes::from(serde_cbor::Value::Bytes(x.clone())),
            )),
            // Arrays need to be mapped.
            CborValue::Array(x) => x
                .iter()
                .map(RadonTypes::try_from)
                .collect::<Result<Vec<RadonTypes>, RadError>>()
                .map(|rt_vec| RadonTypes::Array(RadonArray::from(rt_vec))),
            // Maps are a little tougher to convert, as we need to map keys and values independently.
            CborValue::Map(x) => Ok(RadonTypes::Map(RadonMap::from(
                x.iter()
                    // FIXME: could we use `try_fold` instead of `filter_map` for short-circuiting
                    //  rather than ignoring non-string keys and weird values?
                    .filter_map(|(key, val)| match (key, val) {
                        (cbor::value::Key::Text(cbor::value::Text::Text(key)), val) => {
                            RadonTypes::try_from(val)
                                .and_then(|val| {
                                    SerdeCborValue::try_from(val).map(|serde_cbor_value| {
                                        (key.clone(), RadonBytes::from(serde_cbor_value))
                                    })
                                })
                                .ok()
                        }
                        _ => None,
                    })
                    .collect::<BTreeMap<String, RadonBytes>>(),
            ))),
            // Fail on `Break`, `Null`, `Simple` or `Undefined`
            _ => Err(RadError::default()),
        }
    }
}
