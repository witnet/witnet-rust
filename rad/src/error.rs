//! Error type definitions for the RAD module.

use failure::{self, Fail};
use jsonrpc_ws_server::jsonrpc_core;
use rmpv::{Integer, Value};

/// RAD error codes
pub mod rad_error_codes{
    /// RadError Codes
    pub const DECODE: i64 = 100;
    pub const ENCODE: i64 = 101;
    pub const HASH: i64 = 102;
    pub const HTTP: i64 = 103;
    pub const JSON_PARSE: i64 = 104;
    pub const MAP_KEY_NOT_FOUND: i64 = 105;
    pub const MESSAGEPACK: i64 = 106;
    pub const NO_OPERATOR_IN_COMPOUND_CALL: i64 = 107;
    pub const NOT_INTEGER_OPERATOR: i64 = 108;
    pub const NOT_NATURAL_OPERATOR: i64 = 109;
    pub const PARSE_FLOAT: i64 = 110;
    pub const SCRIPT_NOT_ARRAY: i64 = 111;
    pub const UNKNOWN_OPERATOR: i64 = 112;
    pub const UNSUPPORTED_HASH_FUNCTION: i64 = 113;
    pub const UNSUPPORTED_OPERATOR: i64 = 114;
    pub const UNSUPPORTED_REDUCER: i64 = 115;
    pub const WRONG_ARGUMENTS: i64 = 116;
}

/// RAD errors.
#[derive(Debug, PartialEq, Fail)]
pub enum RadError {
    /// Failed to decode a type from other
    #[fail(display = "Failed to decode {} from {}", to, from)]
    Decode { from: String, to: String },
    /// Failed to encode a type into other
    #[fail(display = "Failed to encode {} into {}", from, to)]
    Encode { from: String, to: String },
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
    /// Failed to convert string to float
    #[fail(
        display = "Failed to convert string to float with error message: {}",
        message
    )]
    ParseFloat { message: String },
}

impl From<reqwest::Error> for RadError {
    fn from(err: reqwest::Error) -> RadError {
        RadError::Http {
            message: err.to_string(),
        }
    }
}

impl From<std::num::ParseFloatError> for RadError {
    fn from(err: std::num::ParseFloatError) -> RadError {
        RadError::ParseFloat {
            message: err.to_string(),
        }
    }
}

impl Into<jsonrpc_core::Error> for RadError {
    fn into(self) -> jsonrpc_core::Error {
        let build_error = |rad_error: RadError, code: i64| {
            let mut err =
                jsonrpc_core::types::error::Error::new(jsonrpc_core::ErrorCode::from(code));
            err.message = rad_error.to_string();
            err
        };

        match &self {
            RadError::Encode { .. } => build_error(self, rad_error_codes::ENCODE),
            RadError::Decode { .. } => build_error(self, rad_error_codes::DECODE),
            RadError::Hash => build_error(self, rad_error_codes::HASH),
            RadError::JsonParse { .. } => build_error(self, rad_error_codes::JSON_PARSE),
            RadError::MapKeyNotFound { .. } => build_error(self, rad_error_codes::MAP_KEY_NOT_FOUND),
            RadError::MessagePack { .. } => build_error(self, rad_error_codes::MESSAGEPACK),
            RadError::NoOperatorInCompoundCall { .. } => {
                build_error(self, rad_error_codes::NO_OPERATOR_IN_COMPOUND_CALL)
            }
            RadError::NotIntegerOperator { .. } => {
                build_error(self, rad_error_codes::NOT_INTEGER_OPERATOR)
            }
            RadError::NotNaturalOperator { .. } => {
                build_error(self, rad_error_codes::NOT_NATURAL_OPERATOR)
            }
            RadError::ParseFloat { .. } => build_error(self, rad_error_codes::PARSE_FLOAT),
            RadError::ScriptNotArray { .. } => build_error(self, rad_error_codes::SCRIPT_NOT_ARRAY),
            RadError::UnknownOperator { .. } => build_error(self, rad_error_codes::UNKNOWN_OPERATOR),
            RadError::UnsupportedHashFunction { .. } => {
                build_error(self, rad_error_codes::UNSUPPORTED_HASH_FUNCTION)
            }
            RadError::UnsupportedOperator { .. } => {
                build_error(self, rad_error_codes::UNSUPPORTED_OPERATOR)
            }
            RadError::UnsupportedReducer { .. } => {
                build_error(self, rad_error_codes::UNSUPPORTED_REDUCER)
            }
            RadError::WrongArguments { .. } => build_error(self, rad_error_codes::WRONG_ARGUMENTS),
            RadError::Http { .. } => build_error(self, rad_error_codes::HTTP),
        }
    }
}
