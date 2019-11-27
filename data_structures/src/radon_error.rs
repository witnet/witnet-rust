use std::convert::TryFrom;
use std::io::Cursor;

use cbor::GenericEncoder;
use cbor::types::Tag;
use cbor::value::Value;
use num_enum::IntoPrimitive;

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
}

/// Use `RadonErrors::Unknown` as the default value of `RadonErrors`.
impl Default for RadonErrors {
    fn default() -> Self {
        RadonErrors::Unknown
    }
}

/// This trait identifies a structure that can be used as an error type for `RadonError` and
/// `RadonReport`.
pub trait ErrorLike: Default + From<cbor::encoder::EncodeError> {
    fn intercept<RT>(value: Result<RT, Self>) -> Result<RT, RadonError<Self>>;
}

/// This structure is aimed to be the error type for the `result` field of `witnet_data_structures::radon_report::Report`.
#[derive(Clone, Debug)]
pub struct RadonError<IE>
where
    IE: ErrorLike,
{
    /// A vector of arguments as `cbor::value::Value`.
    pub arguments: Vec<Value>,
    /// The original `RadError` that originated this `RadonError` (if any)
    pub inner: Option<IE>,
    /// One of the cases in `RadonErrors`.
    pub kind: RadonErrors,
}

/// Implementation of encoding and convenience methods for `RadonError`.
impl<IE> RadonError<IE>
where
    IE: ErrorLike,
{
    /// Simple factory for `RadonError`.
    pub fn new(kind: RadonErrors, inner: Option<IE>, arguments: Vec<Value>) -> Self {
        RadonError {
            arguments,
            inner,
            kind,
        }
    }

    /// Allow CBOR encoding of `RadonError` structures.
    pub fn encode(&self) -> Result<Vec<u8>, IE> {
        Vec::<u8>::try_from(self)
    }
}

/// Allow constructing a `RadonError` with no arguments by just choosing the `kind` field.
impl<IE> From<RadonErrors> for RadonError<IE>
where
    IE: ErrorLike,
{
    fn from(kind: RadonErrors) -> Self {
        RadonError {
            kind,
            arguments: Vec::new(),
            inner: None,
        }
    }
}

/// Allow constructing a `RadonError` with no arguments by just passing the `inner` field.
impl<IE> From<IE> for RadonError<IE>
where
    IE: ErrorLike,
{
    fn from(inner: IE) -> Self {
        RadonError {
            kind: RadonErrors::default(),
            arguments: Vec::new(),
            inner: Some(inner),
        }
    }
}

/// Convert `RadonError` structure into instances of `cbor::value::Value`.
impl<IE> From<&RadonError<IE>> for Value
where
    IE: ErrorLike,
{
    fn from(error: &RadonError<IE>) -> Self {
        let mut values = vec![Value::U8(error.kind.into())];
        error
            .arguments
            .iter()
            .for_each(|argument| values.push(argument.clone()));
        Value::Tagged(Tag::of(39), Box::new(Value::Array(values)))
    }
}

/// Allow CBOR encoding of `RadonError` structures.
impl<IE> TryFrom<&RadonError<IE>> for Vec<u8>
where
    IE: ErrorLike,
{
    type Error = IE;

    fn try_from(error: &RadonError<IE>) -> Result<Self, Self::Error> {
        let mut encoder = GenericEncoder::new(Cursor::new(Vec::new()));
        encoder.value(&Value::from(error))?;

        Ok(encoder.into_inner().into_writer().into_inner())
    }
}
