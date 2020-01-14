//! Error type definitions for the RAD module.

use std::convert::TryFrom;

use cbor::value::Value as CborValue;
use failure::{self, Fail};
use serde_cbor::value::Value as SerdeCborValue;

use witnet_data_structures::radon_error::{ErrorLike, RadonError, RadonErrors};

use crate::types::RadonTypes;
use crate::{operators::RadonOpCodes, types::array::RadonArray};

/// RAD errors.
#[derive(Clone, Debug, PartialEq, Fail)]
pub enum RadError {
    /// An unknown error. Something went really bad!
    #[fail(display = "Unknown error")]
    Unknown,
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
    /// The given filter code is unknown
    #[fail(display = "Filter code `{}` is unknown", code)]
    UnknownFilter { code: i128 },
    /// The given reducer code is unknown
    #[fail(display = "Reducer code `{}` is unknown", code)]
    UnknownReducer { code: i128 },
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
        args: Option<Vec<SerdeCborValue>>,
    },
    /// The given reducer is not implemented for the type of the input Array
    #[fail(
        display = "Reducer `{}` is not implemented for Array `{:?}`",
        reducer, array
    )]
    UnsupportedReducer { array: RadonArray, reducer: String },
    /// The given filter is not implemented for the type of the input Array
    #[fail(
        display = "Filter `{}` is not implemented for Array `{:?}`",
        filter, array
    )]
    UnsupportedFilter { array: RadonArray, filter: String },
    /// The sort operator is not implemented for non-string arrays
    #[fail(display = "ArraySort is not supported for RadonArray `{:?}`", array)]
    UnsupportedSortOp { array: RadonArray },
    /// The operator is not implemented for non-homogeneous arrays
    #[fail(
        display = "`{}` is not supported for RadonArray with non homogeneous types",
        operator
    )]
    UnsupportedOpNonHomogeneous { operator: String },
    /// This operator cannot be used in tally stage
    #[fail(display = "Operator {} cannot be used in tally stage", operator)]
    UnsupportedOperatorInTally { operator: RadonOpCodes },
    /// This filter cannot be used in aggregation or tally stage
    #[fail(
        display = "Filter {} cannot be used in aggregation or tally stage",
        operator
    )]
    UnsupportedFilterInAT { operator: u8 },
    /// This reducer cannot be used in aggregation or tally stage
    #[fail(
        display = "Reducer {} cannot be used in aggregation or tally stage",
        operator
    )]
    UnsupportedReducerInAT { operator: u8 },
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
        args: Vec<SerdeCborValue>,
    },
    /// The HTTP response was an error code
    #[fail(display = "HTTP GET response was an HTTP error code: {}", status_code)]
    HttpStatus { status_code: u16 },
    /// Failed to execute HTTP request
    #[fail(
        display = "Failed to execute HTTP GET request with error message: {}",
        message
    )]
    HttpOther { message: String },
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
    BadSubscriptFormat { value: SerdeCborValue },
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
    /// Error while parsing retrieval URL
    #[fail(display = "URL parse error: {}: url={:?}", inner, url)]
    UrlParseError {
        #[cause]
        inner: url::ParseError,
        url: String,
    },
    /// Timeout during retrieval phase
    #[fail(display = "Timeout during retrieval phase")]
    RetrieveTimeout,
    /// Invalid script
    #[fail(
        display = "CBOR value cannot be translated into a proper RADON script: {:?}",
        value
    )]
    InvalidScript { value: SerdeCborValue },
    /// Alleged `RadonError` is actually not an instance of `cbor::value::Value::Array`
    #[fail(
        display = "Failed to decode a `RadonError` from a `cbor::value::Value` that was not `Array` (was actually `{}`)",
        actual_type
    )]
    DecodeRadonErrorNotArray { actual_type: String },
    /// Alleged `RadonError` is actually an empty `cbor::value::Value::Array`
    #[fail(
        display = "Failed to decode a `RadonError` from a `cbor::value::Value::Array` because the array was empty"
    )]
    DecodeRadonErrorEmptyArray,
    /// Alleged `RadonError` contains an error code that is not `u8`
    #[fail(
        display = "Failed to decode a `RadonError` from a `cbor::value::Value::Array` because its first element (the error code) was not `cbor::value::Value::U8` (was actually `{}`)",
        actual_type
    )]
    DecodeRadonErrorBadCode { actual_type: String },
    /// Alleged `RadonError` contains an unknown error code
    #[fail(
        display = "Failed to decode a `RadonError` from a `cbor::value::Value::Array` because its first element (`{:?}`) did not match any known error code",
        error_code
    )]
    DecodeRadonErrorUnknownCode { error_code: u8 },
    /// No reveals received
    #[fail(display = "No reveals received")]
    NoReveals,
    /// Insufficient consensus in tally precondition clause
    #[fail(
        display = "Tally precondition clause failed because of insufficient consensus (achieved: {}, required: {})",
        achieved, required
    )]
    InsufficientConsensus { achieved: f64, required: f64 },
}

/// Satisfy the `ErrorLike` trait that ensures generic compatibility of `witnet_rad` and
/// `witnet_data_structures`.
impl ErrorLike for RadError {}

/// Use `RadError::Unknown` as the default error.
impl std::default::Default for RadError {
    fn default() -> Self {
        RadError::Unknown
    }
}

impl From<reqwest::Error> for RadError {
    fn from(err: reqwest::Error) -> Self {
        match err.status() {
            Some(status_code) => RadError::HttpStatus {
                status_code: status_code.as_u16(),
            },
            None => RadError::HttpOther {
                message: err.to_string(),
            },
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
            from: String::from("RadonTypes"),
            to: String::from("CBOR"),
        }
    }
}

impl From<cbor::decoder::DecodeError> for RadError {
    fn from(_err: cbor::decoder::DecodeError) -> Self {
        RadError::Decode {
            from: String::from("CBOR"),
            to: String::from("RadonTypes"),
        }
    }
}

impl TryFrom<RadError> for RadonError<RadError> {
    type Error = RadError;

    /// This is the main logic for intercepting `RadError` items and converting them into
    /// `RadonError` so that they can be committed, revealed, tallied, etc.
    fn try_from(rad_error: RadError) -> Result<Self, Self::Error> {
        match rad_error {
            // TODO: support all cases of `RadError`
            RadError::HttpStatus { status_code } => Ok(RadonError::new(
                RadonErrors::HTTPError,
                Some(rad_error),
                vec![CborValue::U8(status_code as u8)],
            )),
            RadError::NoReveals => Ok(RadonError::new(
                RadonErrors::NoReveals,
                Some(rad_error),
                vec![],
            )),
            not_intercepted => Err(not_intercepted),
        }
    }
}

impl TryFrom<Result<RadonTypes, RadError>> for RadonTypes {
    type Error = RadError;

    /// This is an alternative version of `RadError` interception that operates on the main result
    /// type of the `witnet_rad` module, i.e. `Result<RadonTypes, RadError`.
    fn try_from(value: Result<RadonTypes, RadError>) -> Result<Self, Self::Error> {
        match value {
            // Try to intercept errors
            Err(rad_error) => {
                RadonError::<RadError>::try_from(rad_error).map(RadonTypes::RadonError)
            }
            // Pass through actual values
            ok => ok,
        }
    }
}
