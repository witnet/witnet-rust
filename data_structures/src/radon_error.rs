use std::io::Cursor;

use cbor::{types::Tag, value::Value as CborValue, GenericEncoder};
use failure::Fail;
use num_enum::{IntoPrimitive, TryFromPrimitive};
use serde::Serialize;

#[derive(Clone, Copy, Debug, Eq, IntoPrimitive, PartialEq, Serialize, TryFromPrimitive)]
#[repr(u8)]
/// List of RADON-level errors.
/// **WARNING: these codes are consensus-critical.** They can be renamed but they cannot be
/// re-assigned without causing a non-backwards-compatible protocol upgrade.
pub enum RadonErrors {
    /// Unknown error. Something went really bad!
    Unknown = 0x00,
    // Script format errors
    /// At least one of the source scripts is not a valid CBOR-encoded value.
    SourceScriptNotCBOR = 0x01,
    /// The CBOR value decoded from a source script is not an Array.
    SourceScriptNotArray = 0x02,
    /// The Array value decoded form a source script is not a valid RADON script.
    SourceScriptNotRADON = 0x03,
    // Complexity errors
    /// The request contains too many sources.
    RequestTooManySources = 0x10,
    /// The script contains too many calls.
    ScriptTooManyCalls = 0x11,
    // Operator errors
    /// The operator does not exist.
    UnsupportedOperator = 0x20,
    // Retrieval-specific errors
    /// At least one of the sources could not be retrieved, but returned HTTP error.
    HTTPError = 0x30,
    // Math errors
    /// Math operator caused an underflow.
    Underflow = 0x40,
    /// Math operator caused an overflow.
    Overflow = 0x41,
    /// Tried to divide by zero.
    DivisionByZero = 0x42,
    // Other errors
    /// Received zero reveals
    NoReveals = 0x50,
}

/// Use `RadonErrors::Unknown` as the default value of `RadonErrors`.
impl Default for RadonErrors {
    fn default() -> Self {
        RadonErrors::Unknown
    }
}

/// This trait identifies a structure that can be used as an error type for `RadonError` and
/// `RadonReport`.
pub trait ErrorLike: Clone + Fail {
    fn encode_cbor_array(&self) -> Vec<CborValue>;
}

/// This structure is aimed to be the error type for the `result` field of `witnet_data_structures::radon_report::Report`.
#[derive(Clone, Debug, PartialEq)]
pub struct RadonError<IE>
where
    IE: ErrorLike,
{
    /// The original `RadError` that originated this `RadonError`
    inner: IE,
}

/// Implementation of encoding and convenience methods for `RadonError`.
impl<IE> RadonError<IE>
where
    IE: ErrorLike,
{
    /// Simple factory for `RadonError`.
    pub fn new(inner: IE) -> Self {
        RadonError { inner }
    }

    pub fn encode_tagged_value(&self) -> CborValue {
        let values: Vec<CborValue> = self.inner.encode_cbor_array();

        CborValue::Tagged(Tag::of(39), Box::new(CborValue::Array(values)))
    }

    pub fn encode_tagged_bytes(&self) -> Result<Vec<u8>, cbor::encoder::EncodeError> {
        let mut encoder = GenericEncoder::new(Cursor::new(Vec::new()));
        encoder.value(&self.encode_tagged_value())?;

        Ok(encoder.into_inner().into_writer().into_inner())
    }
}

impl<IE> std::fmt::Display for RadonError<IE>
where
    IE: ErrorLike,
{
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "RadonError({:?})", self.inner)
    }
}
