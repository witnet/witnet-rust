//! Error type definitions for the RAD module.

use crate::types::array::RadonArray;
use failure::{self, Fail};
use serde_cbor::value::Value;

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
    /// The given index is not present in a RadonArray
    #[fail(display = "Failed to get item at index `{}` from RadonArray", index)]
    ArrayIndexNotFound { index: i32 },
    /// The given key is not present in a RadonMap
    #[fail(display = "Failed to get key `{}` from RadonMap", key)]
    MapKeyNotFound { key: String },
    /// The given subscript does not return RadonBoolean in an ArrayFilter
    #[fail(
        display = "ArrayFilter subscript output was not RadonBoolean (was `{}`)",
        value
    )]
    ArrayFilterWrongSubscript { value: String },
    /// Failed to parse a Value from a buffer
    #[fail(
        display = "Failed to parse a Value from a buffer. Error message: {}",
        description
    )]
    BufferIsNotValue { description: String },
    /// No operator found in compound call
    #[fail(display = "No operator found in compound call")]
    NoOperatorInCompoundCall,
    /// The given operator code is not a valid Integer
    #[fail(display = "Operator code is not a valid Integer")]
    NotIntegerOperator,
    /// The given operator code is not a valid natural number
    #[fail(display = "Operator code `{}` is not a valid natural number", code)]
    NotNaturalOperator { code: i128 },
    /// The parsed value was expected to be a script but is not even an Array
    #[fail(
        display = "The parsed value was expected to be a script but is not even an Array (it was a `{}`)",
        input_type
    )]
    ScriptNotArray { input_type: String },
    /// The given operator code is unknown
    #[fail(display = "Operator code `{}` is unknown", code)]
    UnknownOperator { code: i128 },
    /// The given hash function is not implemented
    #[fail(display = "Hash function `{}` is not implemented", function)]
    UnsupportedHashFunction { function: String },
    /// The given operator is not implemented for the input type
    #[fail(
        display = "Call to operator `{}` with args `{:?}` is not supported for input type `{}`",
        operator, args, input_type
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
    /// The given filter is not implemented for the type of the input Array
    #[fail(
        display = "Filter `{}` is not implemented for Array with inner type `{}`",
        filter, inner_type
    )]
    UnsupportedFilter { inner_type: String, filter: String },
    /// The sort operator is not implemented for non-string arrays
    #[fail(
        display = "ArraySort is not supported for RadonArray with inner type `{}`",
        inner_type
    )]
    UnsupportedSortOp { inner_type: String },
    /// The operator is not implemented for non-homogeneous arrays
    #[fail(
        display = "`{}` is not supported for RadonArray with non homogeneous types",
        operator
    )]
    UnsupportedOpNonHomogeneous { operator: String },
    /// There was a tie after applying the mode reducer
    #[fail(
        display = "There was a tie after applying the mode reducer on values: `{:?}`",
        values
    )]
    ModeTie { values: RadonArray },
    /// Tried to apply mod reducer on an empty array
    #[fail(display = "Tried to apply mode reducer on an empty array")]
    ModeEmpty,
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
    /// Failed to convert string to int
    #[fail(
        display = "Failed to convert string to int with error message: {}",
        message
    )]
    ParseInt { message: String },
    /// Failed to convert string to bool
    #[fail(
        display = "Failed to convert string to bool with error message: {}",
        message
    )]
    ParseBool { message: String },
    /// Overflow error
    #[fail(display = "Overflow error")]
    Overflow,
    /// Mismatching types
    #[fail(
        display = "Mismatching types in {}. Expected: {}, found: {}",
        method, expected, found
    )]
    MismatchingTypes {
        method: String,
        expected: String,
        found: String,
    },
    /// Arrays to be reduced have different sizes
    #[fail(
        display = "Arrays to be reduced in {} have different sizes. {} != {}",
        method, first, second
    )]
    DifferentSizeArrays {
        method: String,
        first: usize,
        second: usize,
    },
    /// Subscripts should be an array
    #[fail(display = "Subscript should be an array but is: {:?}", value)]
    BadSubscriptFormat { value: Value },
    /// Error while executing subscript
    #[fail(
        display = "`{}::{}()`: Error in subscript: {}",
        input_type, operator, inner
    )]
    Subscript {
        input_type: String,
        operator: String,
        inner: Box<RadError>,
    },
}

impl From<reqwest::Error> for RadError {
    fn from(err: reqwest::Error) -> Self {
        RadError::Http {
            message: err.to_string(),
        }
    }
}

impl From<std::num::ParseFloatError> for RadError {
    fn from(err: std::num::ParseFloatError) -> Self {
        RadError::ParseFloat {
            message: err.to_string(),
        }
    }
}

impl From<std::num::ParseIntError> for RadError {
    fn from(err: std::num::ParseIntError) -> Self {
        RadError::ParseInt {
            message: err.to_string(),
        }
    }
}

impl From<std::str::ParseBoolError> for RadError {
    fn from(err: std::str::ParseBoolError) -> Self {
        RadError::ParseBool {
            message: err.to_string(),
        }
    }
}

impl From<cbor::encoder::EncodeError> for RadError {
    fn from(_err: cbor::encoder::EncodeError) -> Self {
        RadError::Encode {
            from: String::from("RadonError"),
            to: String::from("CBOR"),
        }
    }
}
