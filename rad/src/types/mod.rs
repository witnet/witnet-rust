use std::{
    collections::BTreeMap,
    convert::{TryFrom, TryInto},
    fmt,
};

use cbor::value::Value as CborValue;
use serde::{Serialize, Serializer};
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
use witnet_data_structures::{
    chain::Hash,
    radon_error::{try_from_cbor_value_for_serde_cbor_value, RadonError},
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

#[derive(Clone, Debug)]
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
    pub fn try_error_from_cbor_value(value: CborValue) -> Result<Self, RadError> {
        match try_from_cbor_value_for_serde_cbor_value(value) {
            Value::Array(error_args) => Ok(RadonTypes::RadonError(RadError::try_from_cbor_array(
                error_args,
            )?)),
            value => Err(RadError::DecodeRadonErrorNotArray {
                actual_type: format!("{:?}", value),
            }),
        }
    }
}

/// Satisfy the `TypeLike` trait that ensures generic compatibility of `witnet_rad` and
/// `witnet_data_structures`.
impl TypeLike for RadonTypes {
    type Error = RadError;

    // FIXME(953): Unify all CBOR libraries
    fn encode(&self) -> Result<Vec<u8>, Self::Error> {
        Vec::<u8>::try_from((*self).clone())
    }

    /// Eases interception of RADON errors (errors that we want to commit, reveal and tally) so
    /// they can be handled as valid `RadonTypes::RadonError` values, which are subject to
    /// commitment, revealing, tallying, etc.
    fn intercept(result: Result<Self, Self::Error>) -> Self {
        match result {
            Err(rad_error) => {
                RadonTypes::RadonError(RadonError::try_from(rad_error).unwrap_or_else(|error| {
                    let unhandled_rad_error = RadError::UnhandledIntercept {
                        inner: Some(Box::new(error)),
                        message: None,
                    };
                    log::warn!("{}", unhandled_rad_error);
                    RadonError::new(unhandled_rad_error)
                }))
            }
            Ok(x) => x,
        }
    }
}

impl Serialize for RadonTypes {
    fn serialize<S>(&self, serializer: S) -> Result<<S as Serializer>::Ok, <S as Serializer>::Error>
    where
        S: Serializer,
    {
        use serde::ser::SerializeStruct;

        let mut state = serializer.serialize_struct("RadonTypes", 2)?;
        state.serialize_field("type", &self.radon_type_name())?;
        match &self {
            RadonTypes::Array(radon_type) => state.serialize_field("value", &radon_type.value())?,
            RadonTypes::Boolean(radon_type) => {
                state.serialize_field("value", &radon_type.value())?
            }
            RadonTypes::Bytes(radon_type) => state.serialize_field("value", &radon_type.value())?,
            RadonTypes::RadonError(radon_error) => state.serialize_field("value", &radon_error)?,
            RadonTypes::Float(radon_type) => state.serialize_field("value", &radon_type.value())?,
            RadonTypes::Integer(radon_type) => {
                state.serialize_field("value", &radon_type.value())?
            }
            RadonTypes::Map(radon_type) => state.serialize_field("value", &radon_type.value())?,
            RadonTypes::String(radon_type) => {
                state.serialize_field("value", &radon_type.value())?
            }
        }
        state.end()
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
    // FIXME(953): Unify all CBOR libraries
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

    // FIXME(953): Unify all CBOR libraries
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

    // FIXME(953): Unify all CBOR libraries
    fn try_from(input: RadonTypes) -> Result<Self, Self::Error> {
        match input {
            RadonTypes::Array(radon_array) => radon_array.try_into(),
            RadonTypes::Boolean(radon_boolean) => radon_boolean.try_into(),
            RadonTypes::Bytes(radon_bytes) => radon_bytes.try_into(),
            RadonTypes::RadonError(error) => panic!(
                "Should never try to build a `serde_cbor::Value` from `RadonTypes::RadonError`. Error was: {:?}", error
            ),
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

        RadonTypes::try_from(cbor_value)
    }
}

/// Allow CBOR encoding of any variant of `RadonTypes`.
impl TryFrom<RadonTypes> for Vec<u8> {
    type Error = RadError;

    // FIXME(953): Unify all CBOR libraries
    fn try_from(
        radon_types: RadonTypes,
    ) -> Result<Vec<u8>, <Vec<u8> as TryFrom<RadonTypes>>::Error> {
        let type_name = RadonTypes::radon_type_name(&radon_types);

        match radon_types {
            RadonTypes::RadonError(radon_error) => {
                radon_error
                    .encode_tagged_bytes()
                    .map_err(|_| RadError::Encode {
                        from: type_name,
                        to: "Vec<u8>".to_string(),
                    })
            }
            _ => {
                let value: Value = radon_types.try_into()?;
                to_vec(&value).map_err(|_| RadError::Encode {
                    from: type_name,
                    to: "Vec<u8>".to_string(),
                })
            }
        }
    }
}

// FIXME(953): migrate everything to using `cbor-codec` or wait for `serde_cbor` to support CBOR tags.
/// Allow decoding RADON types also from `Value` structures coming from the `cbor-codec` crate.
/// Take into account the difference between `cbor::value::Value` and `serde_cbor::Value`.
impl TryFrom<CborValue> for RadonTypes {
    type Error = RadError;

    fn try_from(cbor_value: CborValue) -> Result<Self, Self::Error> {
        use cbor::value::Int;

        match cbor_value {
            // If the tag is 39, we try to decode the value as `RadonTypes::RadonError`, otherwise
            // we ignore the tag, unbox the tagged value and decode it through recurrently calling
            // this same function.
            CborValue::Tagged(tag, boxed) => match (tag, boxed) {
                (cbor::types::Tag::Unassigned(39), other) => {
                    RadonTypes::try_error_from_cbor_value(*other)
                }
                (_, other) => {
                    let unboxed: CborValue = *other;
                    RadonTypes::try_from(unboxed)
                }
            },
            // Booleans, numbers, strings and bytes are all converted easily.
            CborValue::Bool(x) => Ok(RadonTypes::Boolean(RadonBoolean::from(x))),
            CborValue::U8(x) => Ok(RadonTypes::Integer(RadonInteger::from(i128::from(x)))),
            CborValue::U16(x) => Ok(RadonTypes::Integer(RadonInteger::from(i128::from(x)))),
            CborValue::U32(x) => Ok(RadonTypes::Integer(RadonInteger::from(i128::from(x)))),
            CborValue::U64(x) => Ok(RadonTypes::Integer(RadonInteger::from(i128::from(x)))),
            CborValue::I8(x) => Ok(RadonTypes::Integer(RadonInteger::from(i128::from(x)))),
            CborValue::I16(x) => Ok(RadonTypes::Integer(RadonInteger::from(i128::from(x)))),
            CborValue::I32(x) => Ok(RadonTypes::Integer(RadonInteger::from(i128::from(x)))),
            CborValue::I64(x) => Ok(RadonTypes::Integer(RadonInteger::from(i128::from(x)))),
            CborValue::Int(Int::Neg(x)) => {
                Ok(RadonTypes::Integer(RadonInteger::from(i128::from(x))))
            }
            CborValue::Int(Int::Pos(x)) => {
                Ok(RadonTypes::Integer(RadonInteger::from(i128::from(x))))
            }
            CborValue::F32(x) => Ok(RadonTypes::Float(RadonFloat::from(f64::from(x)))),
            CborValue::F64(x) => Ok(RadonTypes::Float(RadonFloat::from(x))),
            CborValue::Text(cbor::value::Text::Text(x)) => {
                Ok(RadonTypes::String(RadonString::from(x)))
            }
            CborValue::Bytes(cbor::value::Bytes::Bytes(x)) => {
                Ok(RadonTypes::Bytes(RadonBytes::from(x)))
            }
            // Arrays need to be mapped.
            CborValue::Array(x) => x
                .into_iter()
                .map(RadonTypes::try_from)
                .collect::<Result<Vec<RadonTypes>, RadError>>()
                .map(|rt_vec| RadonTypes::Array(RadonArray::from(rt_vec))),
            // Maps are a little tougher to convert, as we need to map keys and values independently.
            CborValue::Map(x) => Ok(RadonTypes::Map(RadonMap::from(
                x.into_iter()
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
    iter.filter_map(|(slice, inner)| match RadonTypes::try_from(slice) {
        Ok(radon_types) => Some(RadonReport::from_result(
            Ok(radon_types),
            &ReportContext::default(),
        )),
        Err(e) => err_action(e, slice, inner),
    })
    .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serial_iter_decode_invalid_reveals() {
        #[allow(clippy::trivially_copy_pass_by_ref)]
        fn ignore_invalid_fn(_: RadError, _: &[u8], _: &()) -> Option<RadonReport<RadonTypes>> {
            None
        }
        #[allow(clippy::trivially_copy_pass_by_ref)]
        fn malformed_reveal_fn(_: RadError, _: &[u8], _: &()) -> Option<RadonReport<RadonTypes>> {
            Some(RadonReport::from_result(
                Err(RadError::MalformedReveal),
                &ReportContext::default(),
            ))
        }

        let malformed_reveal =
            RadonTypes::RadonError(RadonError::try_from(RadError::MalformedReveal).unwrap());

        // No reveals: returns empty vector
        let zero_empty_bytes: Vec<(&[u8], &())> = vec![];
        let empty: Vec<_> =
            serial_iter_decode(&mut zero_empty_bytes.into_iter(), ignore_invalid_fn)
                .into_iter()
                .map(|report| report.into_inner())
                .collect();
        assert_eq!(empty, vec![]);

        // One reveal with zero bytes: return err_action
        // In this case, filter out invalid reveals, so it returns empty vector
        let one_empty_bytes: Vec<(&[u8], &())> = vec![(&[], &())];
        let still_empty: Vec<_> =
            serial_iter_decode(&mut one_empty_bytes.into_iter(), ignore_invalid_fn)
                .into_iter()
                .map(|report| report.into_inner())
                .collect();
        assert_eq!(still_empty, vec![]);

        // One reveal with zero bytes: return err_action
        // In this case, replace invalid reveals with RadError::MalformedReveal
        let one_empty_bytes: Vec<(&[u8], &())> = vec![(&[], &())];
        let rad_decode_error_as_result: Vec<_> =
            serial_iter_decode(&mut one_empty_bytes.into_iter(), malformed_reveal_fn)
                .into_iter()
                .map(|report| report.into_inner())
                .collect();
        assert_eq!(rad_decode_error_as_result, vec![malformed_reveal]);
    }

    #[test]
    fn test_radontypes_try_error_from_cbor_value() {
        let cbor_value_ok = CborValue::Array(vec![CborValue::U8(0x10), CborValue::U8(9)]);
        let cbor_value_wrong_type = CborValue::U8(u8::default());
        let cbor_value_empty_array = CborValue::Array(Vec::default());
        let cbor_value_short_array = CborValue::Array(vec![CborValue::U8(0x11)]);
        let cbor_value_bad_error_code = CborValue::Array(vec![CborValue::Bool(false)]);
        let cbor_value_unknown_error_code = CborValue::Array(vec![CborValue::U8(0xF5)]);

        let radon_types_ok = RadonTypes::try_error_from_cbor_value(cbor_value_ok).unwrap();
        let rad_error_wrong_type =
            RadonTypes::try_error_from_cbor_value(cbor_value_wrong_type).unwrap_err();
        let rad_error_empty_array =
            RadonTypes::try_error_from_cbor_value(cbor_value_empty_array).unwrap_err();
        let radon_types_short_array =
            RadonTypes::try_error_from_cbor_value(cbor_value_short_array).unwrap();
        let rad_error_bad_error_code =
            RadonTypes::try_error_from_cbor_value(cbor_value_bad_error_code).unwrap_err();
        let rad_error_unknown_error_code =
            RadonTypes::try_error_from_cbor_value(cbor_value_unknown_error_code).unwrap_err();

        let expected_ok =
            RadonTypes::RadonError(RadonError::try_from(RadError::RequestTooManySources).unwrap());
        let expected_wrong_type = RadError::DecodeRadonErrorNotArray {
            actual_type: format!("{:?}", Value::Integer(0)),
        };
        let expected_empty_array = RadError::DecodeRadonErrorEmptyArray;
        let expected_short_array =
            RadonTypes::RadonError(RadonError::try_from(RadError::ScriptTooManyCalls).unwrap());
        let expected_bad_error_code = RadError::DecodeRadonErrorBadCode {
            actual_type: format!("{:?}", Value::Bool(false)),
        };
        let expected_unknown_error_code =
            RadError::DecodeRadonErrorUnknownCode { error_code: 0xF5 };

        assert_eq!(radon_types_ok, expected_ok);
        assert_eq!(rad_error_wrong_type, expected_wrong_type);
        assert_eq!(rad_error_empty_array, expected_empty_array);
        assert_eq!(radon_types_short_array, expected_short_array);
        assert_eq!(rad_error_bad_error_code, expected_bad_error_code);
        assert_eq!(rad_error_unknown_error_code, expected_unknown_error_code);
    }
}
