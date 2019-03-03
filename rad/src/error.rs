//! Error type definitions for the RAD module.

use failure::{self, Fail};
use rmpv::{Integer, Value};

/// RAD errors.
#[derive(Debug, PartialEq, Fail)]
pub enum RadError {
    /// Failed to decode a type from other
    #[fail(display = "Failed to decode {} from {}", from, to)]
    Decode {
        from: &'static str,
        to: &'static str,
    },
    /// Failed to encode a type into other
    #[fail(display = "Failed to encode {} into {}", from, to)]
    Encode {
        from: &'static str,
        to: &'static str,
    },
    /// Failed to calculate the hash of a RADON value or structure
    #[fail(display = "Failed to calculate the hash of a RADON value or structure")]
    Hash,
    /// Failed to parse an object from a JSON buffer
    #[fail(
        display = "Failed to parse an object from a JSON buffer: {:?}",
        description
    )]
    JsonParse { description: String },
    /// The given key is not present in a RadonMap
    #[fail(display = "Failed to get key `{}` from RadonMap", key)]
    MapKeyNotFound { key: String },
    /// Failed to parse a Value from a MessagePack buffer
    #[fail(
        display = "Failed to parse a Value from a MessagePack buffer. Error message: {}",
        description
    )]
    MessagePack { description: String },
    /// No operator found in compound call
    #[fail(display = "No operator found in compound call")]
    NoOperatorInCompoundCall,
    /// The given operator code is not a valid Integer
    #[fail(display = "Operator code `{}` is not a valid Integer", code)]
    NotIntegerOperator { code: Box<Value> },
    /// The given operator code is not a valid natural number
    #[fail(display = "Operator code `{}` is not a valid natural number", code)]
    NotNaturalOperator { code: Integer },
    /// The parsed value was expected to be a script but is not even an Array
    #[fail(
        display = "The parsed value was expected to be a script but is not even an Array (it was a `{}`)",
        input_type
    )]
    ScriptNotArray { input_type: String },
    /// The given operator code is unknown
    #[fail(display = "Operator code `{}` is unknown", code)]
    UnknownOperator { code: u64 },
    /// The given hash function is not implemented
    #[fail(display = "Hash function `{}` is not implemented", function)]
    UnsupportedHashFunction { function: String },
    /// The given operator is not implemented for the input type
    #[fail(
        display = "Call to operator `{}` with args `{:?}` is not supported for input type `{}`",
        input_type, args, operator
    )]
    UnsupportedOperator {
        input_type: String,
        operator: String,
        args: Option<Vec<Value>>,
    },
    /// The given reducer is not implemented for the type of the input Array
    #[fail(
        display = "Reducer `{}` is not implemented for Array with inner type `{}`",
        reducer, inner_type
    )]
    UnsupportedReducer { inner_type: String, reducer: String },
    /// The given arguments are not valid for the given operator
    #[fail(
        display = "Wrong `{}::{}()` arguments: `{:?}`",
        input_type, operator, args
    )]
    WrongArguments {
        input_type: String,
        operator: String,
        args: Vec<Value>,
    },
    /// Failed to execute HTTP request
    #[fail(
        display = "Failed to execute HTTP request with error message: {}",
        message
    )]
    Http { message: String },
}

impl From<reqwest::Error> for RadError {
    fn from(err: reqwest::Error) -> RadError {
        RadError::Http {
            message: err.to_string(),
        }
    }
}
