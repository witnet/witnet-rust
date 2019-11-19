use cbor::types::Tag;
use cbor::value::Value;
use cbor::GenericEncoder;
use num_enum::IntoPrimitive;

use crate::rad_error::RadError;
use std::io::Cursor;

#[derive(Clone, Copy, Debug, IntoPrimitive)]
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
    SourceScriptNotArray = 0x2,
    /// The Array value decoded form a source script is not a valid RADON script.
    SourceScriptNotRADON = 0x3,
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
}

/// This structure is aimed to be the error type for the `result` field of `crate::report::Report`.
#[derive(Clone, Debug)]
pub struct RadonError {
    /// One of the cases in `RadonErrors`.
    pub kind: RadonErrors,
    /// A vector of arguments as `cbor::value::Value`.
    pub arguments: Vec<Value>,
    /// The original `RadError` that originated this `RadonError` (if any)
    pub inner: Option<RadError>,
}

/// Implementation of encoding and convenience methods for `RadonError`.
impl RadonError {
    /// Allow CBOR encoding of `RadonError` structures.
    pub fn encode(&self) -> Result<Vec<u8>, RadError> {
        let mut encoder = GenericEncoder::new(Cursor::new(Vec::new()));
        encoder.value(&Value::from(self))?;

        Ok(encoder.into_inner().into_writer().into_inner())
    }
}

/// Allow constructing a `RadonError` with no arguments by just choosing the `kind` field.
impl From<RadonErrors> for RadonError {
    fn from(kind: RadonErrors) -> Self {
        RadonError {
            kind,
            arguments: Vec::new(),
            inner: None,
        }
    }
}

/// Convert `RadonError` structure into instances of `cbor::value::Value`.
impl From<&RadonError> for Value {
    fn from(error: &RadonError) -> Self {
        let mut values = vec![Value::U8(error.kind.into())];
        error
            .arguments
            .iter()
            .for_each(|argument| values.push(argument.clone()));
        Value::Tagged(Tag::of(37), Box::new(Value::Array(values)))
    }
}

/// Allow recovering the originating `RadError` of a `RadonError` (if any).
impl From<RadonError> for RadError {
    fn from(error: RadonError) -> Self {
        error.inner.unwrap_or(RadError::Unknown)
    }
}
