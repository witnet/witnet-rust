use std::{
    collections::BTreeMap,
    convert::{TryFrom, TryInto},
    fmt,
};

use cbor::value::Value as CborValue;
use serde::Serialize;
use serde_cbor::{to_vec, Value};

use witnet_crypto::hash::calculate_sha256;

use crate::{
    error::RadError,
    operators::Operable,
    types::{
        array::RadonArray, boolean::RadonBoolean, bytes::RadonBytes, float::RadonFloat,
        integer::RadonInteger, map::RadonMap, string::RadonString,
    },
};
use witnet_data_structures::radon_error::RadonError;
use witnet_data_structures::{
    chain::Hash,
    radon_error::RadonErrors,
    radon_report::{RadonReport, ReportContext, TypeLike},
};

pub mod array;
pub mod boolean;
pub mod bytes;
pub mod float;
pub mod integer;
pub mod map;
pub mod string;

pub trait RadonType<T>:
    fmt::Display + From<T> + PartialEq + TryFrom<Value> + TryInto<Value> + TryFrom<RadonTypes>
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
    RadonError(RadonError<RadError>),
    Float(RadonFloat),
    Integer(RadonInteger),
    Map(RadonMap),
    String(RadonString),
}

impl RadonTypes {
    pub fn hash(self) -> Result<Hash, RadError> {
        self.encode()
            .map(|vector: Vec<u8>| calculate_sha256(&*vector))
            .map(Hash::from)
            .map_err(|_| RadError::Hash)
    }

    /// Returns the name of the internal type of a `RadonTypes` item.
    pub fn radon_type_name(&self) -> String {
        match self {
            RadonTypes::Array(_) => RadonArray::radon_type_name(),
            RadonTypes::Boolean(_) => RadonBoolean::radon_type_name(),
            RadonTypes::Bytes(_) => RadonBytes::radon_type_name(),
            RadonTypes::Float(_) => RadonFloat::radon_type_name(),
            RadonTypes::Integer(_) => RadonInteger::radon_type_name(),
            RadonTypes::Map(_) => RadonMap::radon_type_name(),
            // `RadonError` does not implement `RadonType` nor `Operable` (it is not a type per se),
            // but exists inside `RadonTypes` for the sake of dealing with error encoding / decoding
            // in a more convenient way.
            RadonTypes::RadonError(_) => String::from("RadonError"),
            RadonTypes::String(_) => RadonString::radon_type_name(),
        }
    }

    pub fn discriminant(&self) -> usize {
        match self {
            RadonTypes::Array(_) => 0,
            RadonTypes::Boolean(_) => 1,
            RadonTypes::Bytes(_) => 2,
            RadonTypes::Float(_) => 3,
            RadonTypes::Integer(_) => 4,
            RadonTypes::Map(_) => 5,
            RadonTypes::RadonError(_) => 6,
            RadonTypes::String(_) => 7,
        }
    }

    pub fn num_types() -> usize {
        8
    }

    pub fn as_operable(&self) -> &dyn Operable {
        match self {
            RadonTypes::Array(inner) => inner,
            RadonTypes::Boolean(inner) => inner,
            RadonTypes::Bytes(inner) => inner,
            RadonTypes::Float(inner) => inner,
            RadonTypes::Integer(inner) => inner,
            RadonTypes::Map(inner) => inner,
            RadonTypes::RadonError(_) => panic!("`RadonTypes::RadonError` is not operable"),
            RadonTypes::String(inner) => inner,
        }
    }

    /// Decodes `RadonTypes::RadonError` items from `cbor::value::Value::Array` values.
    pub fn try_error_from_cbor_value(value: &CborValue) -> Result<Self, RadError> {
        match value {
            CborValue::Array(error_args) => {
                if !error_args.is_empty() {
                    match error_args.as_slice().split_first() {
                        Some((head, tail)) => {
                            if let CborValue::U8(error_code) = head {
                                let kind = RadonErrors::try_from(*error_code).map_err(|_| {
                                    RadError::DecodeRadonErrorUnknownCode {
                                        error_code: *error_code,
                                    }
                                })?;

                                Ok(RadonTypes::RadonError(RadonError::new(
                                    kind,
                                    None,
                                    Vec::from(tail),
                                )))
                            } else {
                                Err(RadError::DecodeRadonErrorBadCode {
                                    actual_type: format!("{:?}", head),
                                })
                            }
                        }
                        None => Err(RadError::DecodeRadonErrorEmptyArray),
                    }
                } else {
                    Err(RadError::DecodeRadonErrorEmptyArray)
                }
            }
            _ => Err(RadError::DecodeRadonErrorNotArray {
                actual_type: format!("{:?}", value),
            }),
        }
    }
}

/// Satisfy the `TypeLike` trait that ensures generic compatibility of `witnet_rad` and
/// `witnet_data_structures`.
impl TypeLike for RadonTypes {
    type Error = RadError;

    // FIXME(953)
    fn encode(&self) -> Result<Vec<u8>, Self::Error> {
        Vec::<u8>::try_from(self)
    }

    /// Eases interception of RADON errors (errors that we want to commit, reveal and tally) so
    /// they can be handled as valid `RadonTypes::RadonError` values, which are subject to
    /// commitment, revealing, tallying, etc.
    fn intercept(result: Result<Self, Self::Error>) -> Result<Self, Self::Error> {
        match result {
            Err(rad_error) => Ok(RadonTypes::RadonError(RadonError::try_from(rad_error)?)),
            ok => ok,
        }
    }
}

impl std::cmp::Eq for RadonTypes {}

// Manually implement PartialEq to ensure
// k1 == k2 ⇒ hash(k1) == hash(k2)
// https://rust-lang.github.io/rust-clippy/master/index.html#derive_hash_xor_eq
impl PartialEq for RadonTypes {
    fn eq(&self, other: &RadonTypes) -> bool {
        if self.discriminant() != other.discriminant() {
            return false;
        }

        let vec1 = self.encode();
        let vec2 = other.encode();

        vec1 == vec2
    }
}

impl std::hash::Hash for RadonTypes {
    // FIXME(953)
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
            RadonTypes::RadonError(inner) => write!(f, "RadonTypes::{}", inner),
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

impl From<RadonError<RadError>> for RadonTypes {
    fn from(error: RadonError<RadError>) -> Self {
        RadonTypes::RadonError(error)
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

impl From<RadonString> for RadonTypes {
    fn from(string: RadonString) -> Self {
        RadonTypes::String(string)
    }
}

impl TryFrom<Value> for RadonTypes {
    type Error = RadError;

    // FIXME(953)
    fn try_from(value: Value) -> Result<RadonTypes, Self::Error> {
        match value {
            Value::Array(_) => RadonArray::try_from(value).map(Into::into),
            Value::Bool(_) => RadonBoolean::try_from(value).map(Into::into),
            Value::Float(_) => RadonFloat::try_from(value).map(Into::into),
            Value::Map(_) => RadonMap::try_from(value).map(Into::into),
            Value::Text(_) => RadonString::try_from(value).map(Into::into),
            Value::Integer(_) => RadonInteger::try_from(value).map(Into::into),
            Value::Bytes(_) => RadonBytes::try_from(value).map(Into::into),
            Value::Null => Err(RadError::Decode {
                from: String::from("serde_cbor::Value::Null"),
                to: String::from("RadonTypes"),
            }),
            _ => Err(RadError::Decode {
                from: String::from("serde_cbor::Value"),
                to: String::from("RadonTypes"),
            }),
        }
    }
}

impl TryFrom<RadonTypes> for Value {
    type Error = RadError;

    // FIXME(953)
    fn try_from(input: RadonTypes) -> Result<Self, Self::Error> {
        match input {
            RadonTypes::Array(radon_array) => radon_array.try_into(),
            RadonTypes::Boolean(radon_boolean) => radon_boolean.try_into(),
            RadonTypes::Bytes(radon_bytes) => radon_bytes.try_into(),
            RadonTypes::RadonError(error) => Ok(Value::Integer(error.kind as i128)),
            RadonTypes::Float(radon_float) => radon_float.try_into(),
            RadonTypes::Integer(radon_integer) => radon_integer.try_into(),
            RadonTypes::Map(radon_map) => radon_map.try_into(),
            RadonTypes::String(radon_string) => radon_string.try_into(),
        }
    }
}

/// Allow CBOR decoding of any variant of `RadonTypes`.
impl TryFrom<&[u8]> for RadonTypes {
    type Error = RadError;

    fn try_from(slice: &[u8]) -> Result<RadonTypes, <RadonTypes as TryFrom<&[u8]>>::Error> {
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

    // FIXME(953)
    fn try_from(
        radon_types: &RadonTypes,
    ) -> Result<Vec<u8>, <Vec<u8> as TryFrom<&RadonTypes>>::Error> {
        let type_name = RadonTypes::radon_type_name(radon_types);
        let value: Value = radon_types.clone().try_into()?;

        to_vec(&value).map_err(|_| RadError::Encode {
            from: type_name,
            to: "Vec<u8>".to_string(),
        })
    }
}

// FIXME(953): migrate everything to using `cbor-codec` or wait for `serde_cbor` to support CBOR tags.
/// Allow decoding RADON types also from `Value` structures coming from the `cbor-codec` crate.
/// Take into account the difference between `cbor::value::Value` and `serde_cbor::Value`.
impl TryFrom<&cbor::value::Value> for RadonTypes {
    type Error = RadError;

    fn try_from(cbor_value: &cbor::value::Value) -> Result<Self, Self::Error> {
        use cbor::value::Int;

        match cbor_value {
            // If the tag is 39, we try to decode the value as `RadonTypes::RadonError`, otherwise
            // we ignore the tag, unbox the tagged value and decode it through recurrently calling
            // this same function.
            CborValue::Tagged(tag, boxed) => match (tag, std::boxed::Box::leak(boxed.clone())) {
                (cbor::types::Tag::Unassigned(39), other) => {
                    RadonTypes::try_error_from_cbor_value(other)
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
            CborValue::Bytes(cbor::value::Bytes::Bytes(x)) => {
                Ok(RadonTypes::Bytes(RadonBytes::from(x.clone())))
            }
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
                            RadonTypes::try_from(val).map(|val| (key.clone(), val)).ok()
                        }
                        _ => None,
                    })
                    .collect::<BTreeMap<String, RadonTypes>>(),
            ))),
            // Fail on `Break`, `Null`, `Simple` or `Undefined`
            _ => Err(RadError::default()),
        }
    }
}

/// Decode a vector of instances of RadonTypes from any iterator that yields `(&[u8], &T)`.
/// The `err_action` argument allows the caller of this function to decide whether
/// it should act in a lossy way, i.e. ignoring items that cannot be decoded or replacing them with
/// default values.
pub fn serial_iter_decode<T>(
    iter: &mut dyn Iterator<Item = (&[u8], &T)>,
    err_action: fn(RadError, &[u8], &T) -> Option<RadonReport<RadonTypes>>,
) -> Vec<RadonReport<RadonTypes>> {
    iter.filter_map(|(slice, inner)| {
        match RadonReport::from_result(RadonTypes::try_from(slice), &ReportContext::default()) {
            Ok(result) => Some(result),
            Err(e) => err_action(e, slice, inner),
        }
    })
    .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_radontypes_try_error_from_cbor_value() {
        use cbor::value::Value as CborValue;

        let cbor_value_ok = CborValue::Array(vec![CborValue::U8(0x10), CborValue::U8(9)]);
        let cbor_value_wrong_type = CborValue::U8(u8::default());
        let cbor_value_empty_array = CborValue::Array(Vec::default());
        let cbor_value_short_array = CborValue::Array(vec![CborValue::U8(0x11)]);
        let cbor_value_bad_error_code = CborValue::Array(vec![CborValue::Bool(false)]);
        let cbor_value_unknown_error_code = CborValue::Array(vec![CborValue::U8(0xFF)]);

        let radon_types_ok = RadonTypes::try_error_from_cbor_value(&cbor_value_ok).unwrap();
        let rad_error_wrong_type =
            RadonTypes::try_error_from_cbor_value(&cbor_value_wrong_type).unwrap_err();
        let rad_error_empty_array =
            RadonTypes::try_error_from_cbor_value(&cbor_value_empty_array).unwrap_err();
        let radon_types_short_array =
            RadonTypes::try_error_from_cbor_value(&cbor_value_short_array).unwrap();
        let rad_error_bad_error_code =
            RadonTypes::try_error_from_cbor_value(&cbor_value_bad_error_code).unwrap_err();
        let rad_error_unknown_error_code =
            RadonTypes::try_error_from_cbor_value(&cbor_value_unknown_error_code).unwrap_err();

        let expected_ok = RadonTypes::RadonError(RadonError::new(
            RadonErrors::RequestTooManySources,
            None,
            vec![CborValue::U8(9)],
        ));
        let expected_wrong_type = RadError::DecodeRadonErrorNotArray {
            actual_type: format!("{:?}", CborValue::U8(u8::default())),
        };
        let expected_empty_array = RadError::DecodeRadonErrorEmptyArray;
        let expected_short_array =
            RadonTypes::RadonError(RadonError::from(RadonErrors::ScriptTooManyCalls));
        let expected_bad_error_code = RadError::DecodeRadonErrorBadCode {
            actual_type: format!("{:?}", CborValue::Bool(false)),
        };
        let expected_unknown_error_code =
            RadError::DecodeRadonErrorUnknownCode { error_code: 0xFF };

        assert_eq!(radon_types_ok, expected_ok);
        assert_eq!(rad_error_wrong_type, expected_wrong_type);
        assert_eq!(rad_error_empty_array, expected_empty_array);
        assert_eq!(radon_types_short_array, expected_short_array);
        assert_eq!(rad_error_bad_error_code, expected_bad_error_code);
        assert_eq!(rad_error_unknown_error_code, expected_unknown_error_code);
    }
}
