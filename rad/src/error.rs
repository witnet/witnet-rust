//! Error type definitions for the RAD module.

use failure::Fail;
use std::fmt;
pub use witnet_util::error::{WitnetError, WitnetResult};

/// RAD Error
#[derive(Debug, Fail)]
#[fail(display = "{} : {}", kind, msg)]
pub struct RadError {
    /// Error kind.
    kind: RadErrorKind,
    /// Error message (likely passed from the originating exception).
    msg: String,
}

impl RadError {
    /// Create a RAD error based on error kind, context and message.
    pub fn new(kind: RadErrorKind, msg: String) -> Self {
        Self { kind, msg }
    }
}

/// RAD errors.
#[derive(Debug)]
pub enum RadErrorKind {
    /// Failed to encode or decode a RADON type into / from bytes
    EncodeDecode,
    /// Failed to calculate the hash of a RADON value or structure
    Hash,
    /// Failed to parse an object from a JSON buffer
    JsonParse,
    /// Failed to parse a Value from a MessagePack buffer
    MessagePack,
    /// No operator found in compound call
    NoOperatorInCompoundCall,
    /// The given operator code is not a valid Integer
    NotIntegerOperator,
    /// The given operator code is not a valid natural number
    NotNaturalOperator,
    /// The parsed value was expected to be a script but is not even an Array
    ScriptNotArray,
    /// The given operator code is unknown
    UnknownOperator,
    /// The given operator is not implemented for the input type
    UnsupportedOperator,
}

impl fmt::Display for RadErrorKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "RadError::{:?}", self)
    }
}

/// Result type for the RAD module.
/// This is the only return type acceptable for any public method in a storage backend.
pub type RadResult<T> = WitnetResult<T, RadError>;
