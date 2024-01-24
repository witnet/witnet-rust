//! Error type definitions for the RAD module.

use std::convert::TryFrom;

use failure::{self, Fail};
use serde::{Serialize, Serializer};
use serde_cbor::value::Value as SerdeCborValue;

use witnet_data_structures::chain::tapi::ActiveWips;
use witnet_data_structures::radon_error::{ErrorLike, RadonError, RadonErrors};

use crate::types::RadonTypes;
use crate::{operators::RadonOpCodes, types::array::RadonArray};

/// RAD errors.
#[derive(Clone, Debug, Fail, PartialEq)]
pub enum RadError {

    /// The given subscript does not return RadonBoolean in an ArrayFilter
    #[fail(
        display = "ArrayFilter subscript output was not RadonBoolean (was `{}`)",
        value
    )]
    ArrayFilterWrongSubscript { value: String },    
    /// The given index is not present in a RadonArray
    #[fail(display = "Failed to get item at index `{}` from RadonArray", index)]
    ArrayIndexOutOfBounds { index: i32 },
    /// Subscripts should be an array
    #[fail(display = "Subscript should be an array but is: {:?}", value)]
    BadSubscriptFormat { value: SerdeCborValue },
    /// Failed to parse a Value from a buffer
    #[fail(
        display = "Failed to parse a Value from a buffer. Error message: {}",
        description
    )]
    BufferIsNotValue { description: String },
    /// Failed to decode a type from other
    #[fail(display = "Failed to decode {} from {}", to, from)]
    Decode {
        from: &'static str,
        to: &'static str,
    },
    /// Alleged `RadonError` has a `RadonTypes` argument which was wrongly serialized
    // FIXME(#953): this error should not exist, but it is useful to detect problems with the
    // current hacky implementation
    #[fail(
        display = "Failed to decode a `RadonError` from a `cbor::value::Value::Array` because its arguments (`{:?}`) were not compatible: {}",
        arguments, message
    )]
    DecodeRadonErrorArgumentsRadonTypesFail {
        arguments: Option<Vec<SerdeCborValue>>,
        message: String,
    },
    /// Alleged `RadonError` contains an error code that is not `u8`
    #[fail(
        display = "Failed to decode a `RadonError` from a `cbor::value::Value::Array` because its first element (the error code) was not `cbor::value::Value::U8` (was actually `{}`)",
        actual_type
    )]
    DecodeRadonErrorBadCode { actual_type: String },
    /// Alleged `RadonError` is actually an empty `cbor::value::Value::Array`
    #[fail(
        display = "Failed to decode a `RadonError` from a `cbor::value::Value::Array` because the array was empty"
    )]
    DecodeRadonErrorEmptyArray,
    /// Alleged `RadonError` does not have any arguments
    #[fail(
        display = "Failed to decode a `RadonError` from a `cbor::value::Value::Array` because its arguments are empty"
    )]
    DecodeRadonErrorMissingArguments,
    /// Alleged `RadonError` is actually not an instance of `cbor::value::Value::Array`
    #[fail(
        display = "Failed to decode a `RadonError` from a `cbor::value::Value` that was not `Array` (was actually `{}`)",
        actual_type
    )]
    DecodeRadonErrorNotArray { actual_type: String },    
    /// Alleged `RadonError` contains an unknown error code
    #[fail(
        display = "Failed to decode a `RadonError` from a `cbor::value::Value::Array` because its first element (`{:?}`) did not match any known error code",
        error_code
    )]
    DecodeRadonErrorUnknownCode { error_code: u8 },
    /// Alleged `RadonError` does not have the expected arguments
    #[fail(
        display = "Failed to decode a `RadonError` from a `cbor::value::Value::Array` because its arguments (`{:?}`) were not compatible: {}",
        arguments, message
    )]
    DecodeRadonErrorWrongArguments {
        arguments: Option<SerdeCborValue>,
        message: String,
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
    /// Tried to apply mode reducer on an empty array
    #[fail(display = "Tried to apply mode reducer on an empty array")]
    EmptyArray,
    /// Failed to encode a type into other
    #[fail(display = "Failed to encode {} into {}", from, to)]
    Encode {
        from: &'static str,
        to: &'static str,
    },
    /// Failed to encode `RadonError` arguments
    #[fail(display = "Failed to encode `RadonError` arguments `{}`", error_args)]
    EncodeRadonErrorArguments { error_args: String },
    /// `RadError` cannot be converted to `RadonError` because the error code is not defined
    #[fail(
        display = "`RadError` cannot be converted to `RadonError` because the error code is not defined"
    )]
    EncodeRadonErrorUnknownCode,
    /// Failed to encode reveal
    #[fail(display = "Error when encoding reveal value")]
    EncodeReveal,
    /// Failed to calculate the hash of a RADON value or structure
    #[fail(display = "Failed to calculate the hash of a RADON value or structure")]
    Hash,
    /// Failed to execute HTTP request
    #[fail(
        display = "Failed to execute HTTP request with error message: {}",
        message
    )]
    HttpOther { message: String },
    /// The HTTP response was an error code
    #[fail(display = "HTTP GET response was an HTTP error code: {}", status_code)]
    HttpStatus { status_code: u16 },
    /// Source looks inconsistent when queried through multiple transports at once.
    #[fail(display = "Source looks inconsistent when queried through multiple transports at once")]
    InconsistentSource,
    /// Error while parsing HTTP header
    #[fail(
        display = "invalid HTTP body: {}",
        error
    )]
    InvalidHttpBody { error: String },
    /// Error while parsing HTTP header
    #[fail(
        display = "invalid HTTP header: {}. name={:?}, value={:?}",
        error, name, value
    )]
    InvalidHttpHeader {
        name: String,
        value: String,
        error: String,
    },
    /// Error while parsing HTTP response
    #[fail(
        display = "invalid HTTP response: {}",
        error
    )]
    InvalidHttpResponse { error: String },
    /// Invalid script
    #[fail(
        display = "CBOR value cannot be translated into a proper RADON script: {:?}",
        value
    )]
    InvalidScript { value: SerdeCborValue },
    /// Failed to parse an object from a JSON buffer
    #[fail(
        display = "Failed to parse an object from a JSON buffer: {:?}",
        description
    )]
    JsonParse { description: String },
    /// Invalid reveal serialization (malformed reveals are converted to this value)
    #[fail(display = "The reveal was not serialized correctly")]
    MalformedReveal,
    /// The given key is not present in a RadonMap
    #[fail(display = "Failed to get key `{}` from RadonMap", key)]
    MapKeyNotFound { key: String },
    /// Tried to divide by zero.
    #[fail(display = "Tried to divide by zero")]
    MathDivisionByZero,
    /// Overflow error
    #[fail(display = "Overflow error")]
    MathOverflow,
    /// Math operator caused an underflow.
    #[fail(display = "Math operator caused an underflow")]
    MathUnderflow,
    /// Mismatching types
    #[fail(
        display = "Mismatching types in {}. Expected: {}, found: {}",
        method, expected, found
    )]
    MismatchingTypes {
        method: String,
        expected: &'static str,
        found: &'static str,
    },
    /// There was a tie after applying the mode reducer
    #[fail(
        display = "There was a tie after applying the mode reducer on values: `{:?}`",
        values
    )]
    ModeTie { values: RadonArray, max_count: u16 },
    /// No operator found in compound call
    #[fail(display = "No operator found in compound call")]
    NoOperatorInCompoundCall,
    /// The given operator code is not a valid Integer
    #[fail(display = "Operator code is not a valid Integer")]
    NotIntegerOperator,
    /// The given operator code is not a valid natural number
    #[fail(display = "Operator code `{}` is not a valid natural number", code)]
    NotNaturalOperator { code: i128 },
    /// Failed to convert string to bool
    #[fail(
        display = "Failed to convert string to bool with error message: {}",
        message
    )]
    ParseBool { message: String },
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
    /// The request contains too many sources.
    #[fail(display = "The request contains too many sources")]
    RequestTooManySources,
    /// Timeout during retrieval phase
    #[fail(display = "Timeout during retrieval phase")]
    RetrieveTimeout,
    /// The parsed value was expected to be a script but is not even an Array
    #[fail(
        display = "The parsed value was expected to be a script but is not even an Array (it was a `{}`)",
        input_type
    )]
    ScriptNotArray { input_type: String },
    /// The script contains too many calls.
    #[fail(display = "The script contains too many calls")]
    ScriptTooManyCalls,
    /// At least one of the source scripts is not a valid CBOR-encoded value.
    #[fail(display = "At least one of the source scripts is not a valid CBOR-encoded value")]
    SourceScriptNotCBOR,
    /// The CBOR value decoded from a source script is not an Array.
    #[fail(display = "The CBOR value decoded from a source script is not an Array")]
    SourceScriptNotArray,
    /// The Array value decoded form a source script is not a valid RADON script.
    #[fail(display = "The Array value decoded form a source script is not a valid RADON script")]
    SourceScriptNotRADON,
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
    /// An unknown error. Something went really bad!
    #[fail(display = "Unknown error")]
    Unknown,
    /// The given filter code is unknown
    #[fail(display = "Filter code `{}` is unknown", code)]
    UnknownFilter { code: i128 },
    /// The given operator code is unknown
    #[fail(display = "Operator code `{}` is unknown", code)]
    UnknownOperator { code: i128 },
    /// The given reducer code is unknown
    #[fail(display = "Reducer code `{}` is unknown", code)]
    UnknownReducer { code: i128 },
    /// The given retrieval code is unknown
    #[fail(display = "Retrieval code is unknown")]
    UnknownRetrieval,    
    /// The given filter is not implemented for the type of the input Array
    #[fail(
        display = "Filter `{}` is not implemented for Array `{:?}`",
        filter, array
    )]
    UnsupportedFilter { array: RadonArray, filter: String },
    /// This filter cannot be used in aggregation or tally stage
    #[fail(
        display = "Filter {} cannot be used in aggregation or tally stage",
        operator
    )]
    UnsupportedFilterInAT { operator: u8 },
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
    /// This operator cannot be used in tally stage
    #[fail(display = "Operator {} cannot be used in tally stage", operator)]
    UnsupportedOperatorInTally { operator: RadonOpCodes },
    /// The operator is not implemented for non-homogeneous arrays
    #[fail(
        display = "`{}` is not supported for RadonArray with non homogeneous types",
        operator
    )]
    UnsupportedOpNonHomogeneous { operator: String },
    /// The given reducer is not implemented for the type of the input Array
    #[fail(
        display = "Reducer `{}` is not implemented for Array `{:?}`",
        reducer, array
    )]
    UnsupportedReducer { array: RadonArray, reducer: String },
    /// This reducer cannot be used in aggregation or tally stage
    #[fail(
        display = "Reducer {} cannot be used in aggregation or tally stage",
        operator
    )]
    UnsupportedReducerInAT { operator: u8 },
    /// The sort operator is not implemented for non-string arrays
    #[fail(display = "ArraySort is not supported for RadonArray `{:?}`", array)]
    UnsupportedSortOp { array: RadonArray },
    /// Error while parsing retrieval URL
    #[fail(display = "URL parse error: {}: url={:?}", inner, url)]
    UrlParseError {
        #[cause]
        inner: url::ParseError,
        url: String,
    },
    /// The given arguments are not valid for the given operator
    #[fail(
        display = "Wrong `{}::{}()` arguments: `{:?}`",
        input_type, operator, args
    )]
    WrongArguments {
        input_type: &'static str,
        operator: String,
        args: Vec<SerdeCborValue>,
    },
    /// Failed to parse an object from a XML buffer
    #[fail(
        display = "Failed to parse an object from a XML buffer: {:?}",
        description
    )]
    XmlParse { description: String },
    /// Failed to parse an object from a XML buffer by depth overflow
    #[fail(display = "Failed to parse an object from a XML buffer: XML depth overflow")]
    XmlParseOverflow,

    /// ===============================================================================================================
    /// Errors that can be eventually wrapped into a RadonTypes::RadonError<RadError> ---------------------------------
    
    /// Received insufficient commits 
    #[fail(display = "Insufficient commits received")]
    InsufficientCommits,
    
    /// Received insufficient reveals
    #[fail(display = "Insufficient reveals received")]
    InsufficientQuorum,
    
    /// Insufficient consensus in tally precondition clause
    #[fail(
        display = "Tally precondition clause failed because of insufficient consensus (achieved: {}, required: {})",
        achieved, required
    )]
    InsufficientMajority { achieved: f64, required: f64 },    
    
    /// A majority of data sources could either be temporarily unresponsive or failing to report the requested data:
    #[fail(
        display = "Data sources seems to be temporarily unresponsive: subcode {:?}: args: {:?}",
        subcode,
        args
    )]
    CircumstantialFailure { subcode: u8, args: Option<Vec<serde_cbor::Value>> },

    /// At least one data source is inconsistent when queried through multiple transports at once:
    #[fail(display = "Inconsistent sources")]
    InconsistentSources,
    
    /// Any one of the (multiple) Retrieve, Aggregate or Tally scripts were badly formated:
    #[fail(
        display = "Malformed data request: {:?}",
        subcode
    )]
    MalformedDataRequest { subcode: u8 },
    
    /// Values returned from data sources seem not to match the expected schema:
    #[fail(
        display = "Malformed response from data sources: {:?}",
        subcode
    )]
    MalformedResponses { subcode: u8 },
    
    /// `RadError` cannot be converted to `RadonError` but it should, because it is needed for the tally result
    #[fail(
        display = "`RadError` cannot be converted to `RadonError` but it should, because it is needed for the tally result. Inner: `{:?}`",
        inner
    )]
    UnhandledInterceptV2 { inner: Option<Box<RadError>> },
    
    /// Legacy: Errors during tally execution
    #[fail(
        display = "Error during tally execution. Message: {:?}. Inner: `{:?}`",
        message, inner
    )]
    TallyExecution {
        inner: Option<Box<RadError>>,
        message: Option<String>,
    },
    
    /// Legacy: `RadError` cannot be converted to `RadonError` but it should, because it is needed for the tally result
    #[fail(
        display = "`RadError` cannot be converted to `RadonError` but it should, because it is needed for the tally result. Message: {:?}. Inner: `{:?}`",
        message, inner
    )]
    UnhandledIntercept {
        inner: Option<Box<RadError>>,
        message: Option<String>,
    },
}


impl RadError {
   
    pub fn into_error_code(&self) -> RadonErrors {
        match self {

            RadError::CircumstantialFailure { .. }
                | RadError::ArrayIndexOutOfBounds { .. }
                | RadError::HttpStatus { .. }
                | RadError::MapKeyNotFound { .. }
                | RadError::ModeTie { .. }
                | RadError::RetrieveTimeout
                | RadError::TallyExecution { .. }
            => {
                RadonErrors::CircumstantialFailure
            }
            RadError::InconsistentSource
                | RadError::InconsistentSources
            => {
                RadonErrors::InconsistentSources
            }
            RadError::InsufficientCommits => RadonErrors::InsufficientCommits,
            RadError::InsufficientMajority { .. } => RadonErrors::InsufficientMajority,
            RadError::InsufficientQuorum => RadonErrors::InsufficientQuorum,
            RadError::MalformedDataRequest { .. }
                | RadError::ArrayFilterWrongSubscript { .. }
                | RadError::BadSubscriptFormat { .. }
                | RadError::HttpOther { .. }
                | RadError::InvalidHttpBody { .. } 
                | RadError::InvalidHttpHeader { .. }
                | RadError::InvalidScript { .. }
                | RadError::NoOperatorInCompoundCall
                | RadError::NotIntegerOperator
                | RadError::NotNaturalOperator { .. }
                | RadError::RequestTooManySources
                | RadError::ScriptNotArray { .. }
                | RadError::ScriptTooManyCalls { .. }
                | RadError::SourceScriptNotArray 
                | RadError::SourceScriptNotCBOR
                | RadError::SourceScriptNotRADON
                | RadError::UnknownFilter { .. }
                | RadError::UnknownOperator { .. }
                | RadError::UnknownReducer { .. }
                | RadError::UnknownRetrieval { .. }
                | RadError::UnsupportedFilter { .. }
                | RadError::UnsupportedFilterInAT { .. }
                | RadError::UnsupportedHashFunction { .. }
                | RadError::UnsupportedOperator { .. }
                | RadError::UnsupportedOperatorInTally { .. }
                | RadError::UnsupportedOpNonHomogeneous { .. }
                | RadError::UnsupportedReducer { .. }
                | RadError::UnsupportedReducerInAT { .. }
                | RadError::UrlParseError { .. } 
                | RadError::WrongArguments { .. }
            => {
                RadonErrors::MalformedDataRequest
            }
            RadError::MalformedResponses { .. }
                | RadError::BufferIsNotValue { .. } 
                | RadError::Decode { .. }
                | RadError::DifferentSizeArrays { .. }
                | RadError::EmptyArray 
                | RadError::Encode { .. }
                | RadError::EncodeReveal
                | RadError::Hash
                | RadError::InvalidHttpResponse { .. }
                | RadError::JsonParse { .. }
                | RadError::MalformedReveal 
                | RadError::MathDivisionByZero
                | RadError::MathOverflow
                | RadError::MathUnderflow
                | RadError::MismatchingTypes { .. }
                | RadError::ParseBool { .. }
                | RadError::ParseFloat { .. }
                | RadError::ParseInt { .. }
                | RadError::UnsupportedSortOp { .. }
                | RadError::XmlParse { .. }
                | RadError::XmlParseOverflow 
            => {
                RadonErrors::MalformedResponses
            }
            RadError::Subscript { inner, .. } => {
                inner.into_error_code()
            }
            RadError::DecodeRadonErrorArgumentsRadonTypesFail { .. }
                | RadError::DecodeRadonErrorBadCode { .. }
                | RadError::DecodeRadonErrorEmptyArray 
                | RadError::DecodeRadonErrorMissingArguments
                | RadError::DecodeRadonErrorNotArray { .. }
                | RadError::DecodeRadonErrorUnknownCode { .. }
                | RadError::DecodeRadonErrorWrongArguments { .. }
                | RadError::EncodeRadonErrorArguments { .. }
                | RadError::EncodeRadonErrorUnknownCode
                | RadError::UnhandledIntercept { .. }
                | RadError::UnhandledInterceptV2 { .. }
                | RadError::Unknown
            => {
                RadonErrors::UnhandledIntercept
            }
        }
    }

    pub fn into_error_subcode(&self) -> RadonErrors {
        match self {

            RadError::ArrayIndexOutOfBounds { .. } => RadonErrors::ArrayIndexOutOfBounds,
            RadError::ArrayFilterWrongSubscript { .. } 
                | RadError::UnsupportedSortOp { .. } 
            => {
                RadonErrors::WrongSubscriptInput
            }
            RadError::BadSubscriptFormat { .. }
                | RadError::InvalidScript { .. }
                | RadError::ScriptNotArray { .. }
                | RadError::SourceScriptNotArray 
            => {
                RadonErrors::SourceScriptNotArray
            }
            RadError::BufferIsNotValue { .. } => RadonErrors::BufferIsNotValue,
            RadError::CircumstantialFailure { subcode, .. } => {
                RadonErrors::try_from(*subcode).unwrap_or_default()
            }
            RadError::Decode { .. } => RadonErrors::Decode,
            RadError::DecodeRadonErrorArgumentsRadonTypesFail { .. }
                | RadError::DecodeRadonErrorBadCode { .. }
                | RadError::DecodeRadonErrorEmptyArray { .. }
                | RadError::DecodeRadonErrorMissingArguments { .. }
                | RadError::DecodeRadonErrorNotArray { .. }
                | RadError::DecodeRadonErrorUnknownCode { .. }
                | RadError::DecodeRadonErrorWrongArguments { .. }
                | RadError::MalformedReveal
            => {
                RadonErrors::MalformedReveals
            }
            RadError::DifferentSizeArrays { .. } => RadonErrors::MismatchingArrays,
            RadError::EmptyArray => RadonErrors::EmptyArray,
            RadError::Encode { .. } => RadonErrors::Encode,
            RadError::EncodeReveal 
                | RadError::EncodeRadonErrorArguments { .. }
                | RadError::EncodeRadonErrorUnknownCode { .. }
            => {
                RadonErrors::EncodeReveals
            }
            // RadError::Filter { .. } => RadonErrors::Filter,
            RadError::Hash => RadonErrors::Hash,
            RadError::HttpStatus { .. }
                | RadError::HttpOther { .. }
            => {
                RadonErrors::HttpErrors
            }
            RadError::InvalidHttpBody { .. } => RadonErrors::SourceRequestBody,
            RadError::InvalidHttpHeader { .. } => RadonErrors::SourceRequestHeaders,
            RadError::JsonParse { .. } 
                | RadError::XmlParse { .. }
                | RadError::ParseBool { .. } 
                | RadError::ParseFloat { .. }
                | RadError::ParseInt { .. }
            => {
                RadonErrors::Parse
            }
            RadError::MalformedDataRequest { subcode } => {
                RadonErrors::try_from(*subcode).unwrap_or_default()
            }
            RadError::MalformedResponses { subcode } => {
                RadonErrors::try_from(*subcode).unwrap_or_default()
            }
            RadError::MapKeyNotFound { .. } => RadonErrors::MapKeyNotFound,
            RadError::MathDivisionByZero => RadonErrors::MathDivisionByZero,
            RadError::MathOverflow => RadonErrors::MathOverflow,
            RadError::MathUnderflow => RadonErrors::MathUnderflow,
            RadError::MismatchingTypes { .. }
                | RadError::WrongArguments { .. }
            => {
                RadonErrors::WrongArguments
            }
            RadError::ModeTie { .. } => RadonErrors::ModeTie,
            // RadError::Reduce { .. } => RadonErrors::Reduce,
            RadError::RequestTooManySources => RadonErrors::RequestTooManySources,
            RadError::RetrieveTimeout => RadonErrors::RetrievalsTimeout,
            RadError::ScriptTooManyCalls { .. } => RadonErrors::ScriptTooManyCalls,
            RadError::SourceScriptNotCBOR { .. } => RadonErrors::SourceScriptNotCBOR,
            RadError::SourceScriptNotRADON { .. } => RadonErrors::SourceScriptNotRADON,
            RadError::Subscript { inner, .. } => {
                inner.as_ref().into_error_subcode()
            }
            RadError::UnknownFilter { .. }
                | RadError::UnsupportedFilter { .. }
                | RadError::UnsupportedFilterInAT { .. }
            => {
                RadonErrors::UnsupportedFilter
            }
            RadError::UnknownOperator { .. }
                | RadError::UnsupportedOperator { .. }
                | RadError::UnsupportedOperatorInTally { .. }
                | RadError::NoOperatorInCompoundCall { .. } 
                | RadError::NotIntegerOperator 
                | RadError::NotNaturalOperator { .. }
            => {
                RadonErrors::UnsupportedOperator
            }
            RadError::UnknownReducer { .. }
                | RadError::UnsupportedReducer { .. }
                | RadError::UnsupportedReducerInAT { .. }
            => {
                RadonErrors::UnsupportedReducer
            }
            RadError::UnknownRetrieval => RadonErrors::UnsupportedRequestType,
            RadError::UnsupportedHashFunction { .. } => RadonErrors::UnsupportedHashFunction,
            RadError::UnsupportedOpNonHomogeneous { .. } => RadonErrors::NonHomogeneousArrays,
            RadError::UrlParseError { .. } => RadonErrors::SourceRequestURL,
            RadError::XmlParseOverflow => RadonErrors::ParseOverflow,
            _ => {
                RadonErrors::Unknown
            }
        }
    }

    pub fn try_from_cbor_array(
        serde_cbor_array: Vec<SerdeCborValue>,
    ) -> Result<RadonError<Self>, RadError> {
        match serde_cbor_array.split_first() {
            Some((head, tail)) => {
                if let SerdeCborValue::Integer(error_code) = head {
                    let error_code = u8::try_from(*error_code).map_err(|_| {
                        RadError::DecodeRadonErrorBadCode {
                            actual_type: format!("{:?}", head),
                        }
                    })?;
                    let kind = RadonErrors::try_from(error_code)
                        .map_err(|_| RadError::DecodeRadonErrorUnknownCode { error_code })?;

                    let serde_cbor_error_args = if tail.is_empty() {
                        None
                    } else {
                        Some(tail.to_vec())
                    };

                    Ok(RadError::try_into_radon_error(
                        kind,
                        serde_cbor_error_args,
                    )?)
                } else {
                    Err(RadError::DecodeRadonErrorBadCode {
                        actual_type: format!("{:?}", head),
                    })
                }
            }
            None => {
                Err(RadError::DecodeRadonErrorEmptyArray)
            }
        }
    }

    pub fn try_into_cbor_array(&self) -> Result<Vec<SerdeCborValue>, RadError> {
        let (kind, subcode) = (self.into_error_code(), u8::from(self.into_error_subcode()));
        let args = match kind {
            
            RadonErrors::CircumstantialFailure => {
                match self {
                    RadError::ArrayIndexOutOfBounds { index } => Some(legacy::serialize_args((subcode, index,))?),
                    RadError::CircumstantialFailure { subcode, args } => Some(legacy::serialize_args((subcode, args))?),
                    RadError::HttpStatus { status_code } => Some(legacy::serialize_args((subcode, status_code,))?),
                    RadError::MapKeyNotFound { key } => Some(legacy::serialize_args((subcode, key,))?),
                    RadError::TallyExecution { inner, message } => {
                        let message = match (inner, message) {
                            // Only serialize the message
                            (_, Some(message)) => message.clone(),
                            // But if there is no message, serialize the debug representation of inner
                            (Some(inner), None) => format!("inner: {:?}", inner),
                            // And if there is no inner, serialize this string
                            (None, None) => "inner: None".to_string(),
                        };
                        Some(legacy::serialize_args((subcode, message,))?)
                    }
                    _ => Some(legacy::serialize_args((u8::from(subcode),))?)
                }
            }

            RadonErrors::InconsistentSources | RadonErrors::InsufficientCommits | RadonErrors::InsufficientQuorum => {
                None
            }
            
            RadonErrors::InsufficientMajority => {
                if let RadError::InsufficientMajority { achieved, required } = self {
                    Some(legacy::serialize_args((achieved, required,))?)
                } else {
                    None
                }
            }

            RadonErrors::MalformedDataRequest | RadonErrors::MalformedResponses => {
                Some(legacy::serialize_args((subcode, ))?)
            }

            _ => {
                // Unhandled intercepts must be serialized without a subcode
                let message = if_rust_version::if_rust_version! { >= 1.53 {
                    format!("{:?}", self).replace('\'', "\\'")
                } else {
                    format!("{:?}", self)
                }};
                Some(legacy::serialize_args((message,))?)
            }
        };

        let mut v = vec![
            SerdeCborValue::Integer(i128::from(u8::from(kind))),
            SerdeCborValue::Integer(i128::from(subcode)),
        ];

        match args {
            None => {}
            Some(SerdeCborValue::Array(a)) => {
                // Append arguments to resulting array. The format of the resulting array is:
                // [kind, arg0, arg1, arg2, ...]
                v.extend(a);
            }
            Some(value) => {
                // This can only happen if `serialize_args` is called with a non-tuple argument
                // For example:
                // `serialize_args(x)` is invalid, it should be `serialize_args((x,))`
                panic!("Args should be an array, is {:?}", value);
            }
        }

        Ok(v)
    }

    pub fn try_into_radon_error(
        kind: RadonErrors, 
        error_args: Option<Vec<SerdeCborValue>>
    ) -> Result<RadonError<Self>, Self> {
        Ok(RadonError::new(match kind {

            RadonErrors::CircumstantialFailure => {
                match error_args.unwrap_or_default().split_first() {
                    Some((head, tail)) => {
                        if let SerdeCborValue::Integer(error_code) = head {
                            let error_code = u8::try_from(*error_code).map_err(|_| {
                                RadError::DecodeRadonErrorBadCode {
                                    actual_type: format!("{:?}", error_code),
                                }
                            })?;
                            
                            let subcode = RadonErrors::try_from(error_code)
                                .map_err(|_| RadError::DecodeRadonErrorUnknownCode { error_code })?;
                            
                            let subcode_args = if tail.is_empty() {
                                None
                            } else {
                                Some(tail.to_vec())
                            };

                            RadError::CircumstantialFailure { 
                                subcode: u8::from(subcode), 
                                args: subcode_args
                            }
                        } else {
                            return Err(RadError::DecodeRadonErrorBadCode {
                                actual_type: format!("{:?}", head),
                            });
                        }
                    }
                    None => {
                        return Err(RadError::DecodeRadonErrorEmptyArray);
                    }
                }
            }

            RadonErrors::InconsistentSources => RadError::InconsistentSource,

            RadonErrors::MalformedDataRequest => {
                let (subcode, ) = legacy::deserialize_args(error_args)?;
                RadError::MalformedDataRequest { subcode }
            }

            RadonErrors::MalformedResponses => {
                let (subcode, ) = legacy::deserialize_args(error_args)?;
                RadError::MalformedResponses { subcode }
            }
            
            _ => {
                return Self::try_into_radon_error_before_wip0028(kind, error_args);
            }

        }))
    }

    /// Replaces the `from` field in instances of `RadError::Decode`
    pub fn replace_decode_from(self, from: &'static str) -> Self {
        match self {
            Self::Decode { to, .. } => Self::Decode { from, to },
            other => other,
        }
    }

}

/// Satisfy the `ErrorLike` trait that ensures generic compatibility of `witnet_rad` and
/// `witnet_data_structures`.
impl ErrorLike for RadError {
    fn encode_cbor_array(&self, active_wips: &Option<ActiveWips>) -> Result<Vec<SerdeCborValue>, failure::Error> {
        if let Some(active_wips) = active_wips {
            if active_wips.wip_guidiaz_extended_radon_errors() {
                return self.try_into_cbor_array().map_err(Into::into);
            }
        }
        self.try_into_cbor_array_before_wip0028().map_err(Into::into)
    }

    fn decode_cbor_array(serde_cbor_array: Vec<SerdeCborValue>) -> Result<RadonError<Self>, failure::Error> {
        Self::try_from_cbor_array(serde_cbor_array).map_err(Into::into)
    }
}

/// Use `RadError::Unknown` as the default error.
impl std::default::Default for RadError {
    fn default() -> Self {
        RadError::Unknown
    }
}

impl Serialize for RadError {
    fn serialize<S>(&self, serializer: S) -> Result<<S as Serializer>::Ok, <S as Serializer>::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&self.to_string())
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
            from: "RadonTypes",
            to: "CBOR",
        }
    }
}

impl From<cbor::decoder::DecodeError> for RadError {
    fn from(_err: cbor::decoder::DecodeError) -> Self {
        RadError::Decode {
            from: "CBOR",
            to: "RadonTypes",
        }
    }
}

impl TryFrom<RadError> for RadonError<RadError> {
    type Error = RadError;

    /// This is the main logic for intercepting `RadError` items and converting them into
    /// `RadonError` so that they can be committed, revealed, tallied, etc.
    fn try_from(rad_error: RadError) -> Result<Self, Self::Error> {
        Ok(RadonError::new(rad_error))
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

/// This module was introduced for encapsulating the interim legacy logic before WIP-0028 is
/// introduced, for the sake of maintainability.
pub mod legacy {
    use super::*;

    pub fn deserialize_args<T: serde::de::DeserializeOwned>(
        error_args: Option<Vec<SerdeCborValue>>,
    ) -> Result<T, RadError> {
        let error_args = if let Some(x) = error_args {
            SerdeCborValue::Array(x)
        } else {
            return Err(RadError::DecodeRadonErrorMissingArguments);
        };
    
        serde_cbor::value::from_value(error_args.clone()).map_err(|e| {
            RadError::DecodeRadonErrorWrongArguments {
                arguments: Some(error_args),
                message: e.to_string(),
            }
        })
    }
    
    pub fn serialize_args<T: serde::Serialize + std::fmt::Debug>(
        args: T,
    ) -> Result<SerdeCborValue, RadError> {
        serde_cbor::value::to_value(&args).map_err(|_| RadError::EncodeRadonErrorArguments {
            error_args: format!("{:?}", args),
        })
    }

    impl RadError {

        pub fn try_into_error_code_before_wip0028(&self) -> Result<RadonErrors, RadError> {
            Ok(match self {
                RadError::ArrayIndexOutOfBounds { .. } => RadonErrors::ArrayIndexOutOfBounds,
                RadError::BufferIsNotValue { .. } => RadonErrors::BufferIsNotValue,
                RadError::EncodeReveal => RadonErrors::EncodeReveals,
                RadError::HttpStatus { .. } => RadonErrors::HttpErrors,               
                RadError::InsufficientCommits => RadonErrors::InsufficientCommits,
                RadError::InsufficientMajority { .. } => RadonErrors::InsufficientMajority,
                RadError::InsufficientQuorum => RadonErrors::InsufficientQuorum,
                RadError::MalformedReveal => RadonErrors::MalformedReveals,
                RadError::MapKeyNotFound { .. } => RadonErrors::MapKeyNotFound,
                RadError::MathDivisionByZero => RadonErrors::MathDivisionByZero,
                RadError::MathOverflow => RadonErrors::MathOverflow,
                RadError::MathUnderflow => RadonErrors::MathUnderflow,
                RadError::RequestTooManySources => RadonErrors::RequestTooManySources,
                RadError::RetrieveTimeout => RadonErrors::RetrievalsTimeout,
                RadError::ScriptTooManyCalls => RadonErrors::ScriptTooManyCalls,
                RadError::SourceScriptNotCBOR => RadonErrors::SourceScriptNotCBOR,
                RadError::SourceScriptNotArray => RadonErrors::SourceScriptNotArray,
                RadError::SourceScriptNotRADON => RadonErrors::SourceScriptNotRADON,
                RadError::TallyExecution { .. } => RadonErrors::CircumstantialFailure,
                RadError::UnhandledIntercept { .. } | RadError::UnhandledInterceptV2 { .. } => {
                    RadonErrors::UnhandledIntercept
                }
                RadError::UnsupportedOperator { .. } => RadonErrors::UnsupportedOperator,
                
                // The `InconsistentSource` error was mapped here before WIP#0028 for the sake of backwards
                // compatibility. Namely, to enable paranoid retrieval without having to immediately
                // introduce a breaking change that may jeopardize oracle queries. The point of making
                // the mapping here was to only affect actual witnessing and committing, but not the
                // `try_data_request` function, which could rather use the `InconsistentSource` error for
                // clarity.
                RadError::Unknown | RadError::InconsistentSource => RadonErrors::Unknown,

                _ => return Err(RadError::EncodeRadonErrorUnknownCode),
            })
        } 

        pub fn try_into_cbor_array_before_wip0028(&self) -> Result<Vec<SerdeCborValue>, RadError> {
            let kind = u8::from(self.try_into_error_code_before_wip0028()?);
            let args = match self {
                
                RadError::ArrayIndexOutOfBounds { index } => Some(serialize_args((index,))?),
                RadError::BufferIsNotValue { description } => Some(serialize_args((description,))?),
                RadError::HttpStatus { status_code } => Some(serialize_args((status_code,))?),
                RadError::MapKeyNotFound { key } => Some(serialize_args((key,))?),
                
                RadError::InsufficientMajority { achieved, required } => {
                    Some(serialize_args((achieved, required))?)
                }
                
                RadError::TallyExecution { inner, message } => {
                    let message = match (inner, message) {
                        // Only serialize the message
                        (_, Some(message)) => message.clone(),
                        // But if there is no message, serialize the debug representation of inner
                        (Some(inner), None) => format!("inner: {:?}", inner),
                        // And if there is no inner, serialize this string
                        (None, None) => "inner: None".to_string(),
                    };
                    Some(serialize_args((message,))?)
                }
                
                RadError::UnsupportedOperator {
                    input_type,
                    operator,
                    args,
                } => Some(serialize_args((input_type, operator, args))?),

                RadError::UnhandledIntercept { inner, message } => {
                    let message = match (inner, message) {
                        // Only serialize the message
                        (_, Some(message)) => message.clone(),
                        // But if there is no message, serialize the debug representation of inner
                        (Some(inner), None) => {
                            // Fix #1993 by emulating a bug from old versions of Rust (rust-lang/rust#83046)
                            if_rust_version::if_rust_version! { >= 1.53 {
                                format!("inner: {:?}", inner).replace('\'', "\\'")
                            } else {
                                format!("inner: {:?}", inner)
                            }}
                        }
                        // And if there is no inner, serialize this string
                        (None, None) => "inner: None".to_string(),
                    };
                    Some(serialize_args((message,))?)
                }
                
                _ => None,
            };
    
            let mut v = vec![SerdeCborValue::Integer(i128::from(kind))];
    
            match args {
                None => {}
                Some(SerdeCborValue::Array(a)) => {
                    // Append arguments to resulting array. The format of the resulting array is:
                    // [kind, arg0, arg1, arg2, ...]
                    v.extend(a);
                }
                Some(value) => {
                    // This can only happen if `serialize_args` is called with a non-tuple argument
                    // For example:
                    // `serialize_args(x)` is invalid, it should be `serialize_args((x,))`
                    panic!("Args should be an array, is {:?}", value);
                }
            }
    
            Ok(v)
        }

        pub fn try_into_radon_error_before_wip0028(
            kind: RadonErrors, 
            error_args: Option<Vec<SerdeCborValue>>
        ) -> Result<RadonError<Self>, Self> {
            Ok(RadonError::new(match kind {
    
                RadonErrors::InsufficientCommits => RadError::InsufficientCommits,
                RadonErrors::InsufficientQuorum => RadError::InsufficientQuorum,
                RadonErrors::InsufficientMajority => {
                    let (achieved, required) = deserialize_args(error_args)?;
                    RadError::InsufficientMajority { achieved, required }
                }
                RadonErrors::EncodeReveals => RadError::EncodeReveal,
                RadonErrors::ArrayIndexOutOfBounds => {
                    let (index,) = deserialize_args(error_args)?;
                    RadError::ArrayIndexOutOfBounds { index }
                }
                RadonErrors::BufferIsNotValue => {
                    let (description,) = deserialize_args(error_args)?;
                    RadError::BufferIsNotValue { description }
                }
                RadonErrors::HttpErrors => {
                    let (status_code,) = deserialize_args(error_args)?;
                    RadError::HttpStatus { status_code }
                }
                RadonErrors::MapKeyNotFound => {
                    let (key,) = deserialize_args(error_args)?;
                    RadError::MapKeyNotFound { key }
                }
                RadonErrors::MathDivisionByZero => RadError::MathDivisionByZero,
                RadonErrors::MathOverflow => RadError::MathOverflow,
                RadonErrors::MathUnderflow => RadError::MathUnderflow,
                RadonErrors::RequestTooManySources => RadError::RequestTooManySources,
                RadonErrors::RetrievalsTimeout => RadError::RetrieveTimeout,
                RadonErrors::ScriptTooManyCalls => RadError::ScriptTooManyCalls,
                RadonErrors::SourceScriptNotCBOR => RadError::SourceScriptNotCBOR,
                RadonErrors::SourceScriptNotArray => RadError::SourceScriptNotArray,
                RadonErrors::SourceScriptNotRADON => RadError::SourceScriptNotRADON,
                RadonErrors::TallyExecution => {
                    let (message,) = deserialize_args(error_args)?;
                    RadError::TallyExecution {
                        inner: None,
                        message: Some(message),
                    }
                }
                RadonErrors::MalformedReveals => RadError::MalformedReveal,
                RadonErrors::UnsupportedOperator => {
                    let (input_type, operator, args) = deserialize_args(error_args)?;
                    RadError::UnsupportedOperator {
                        input_type,
                        operator,
                        args,
                    }
                }
                RadonErrors::UnhandledIntercept => {
                    if error_args.is_none() {
                        RadError::UnhandledInterceptV2 { inner: None }
                    } else {
                        let (message,) = deserialize_args(error_args)?;
                        RadError::UnhandledIntercept {
                            inner: None,
                            message: Some(message),
                        }
                    }
                }
                // The only case where a Bridge RadonError could be included in the protocol is that
                // if a witness node report as a reveal, and in that case it would be considered
                // as a MalformedReveal
                RadonErrors::BridgeMalformedRequest
                | RadonErrors::BridgePoorIncentives
                | RadonErrors::BridgeOversizedResult => RadError::MalformedReveal,
                
                _ => RadError::Unknown
            }))
        }

    }
}


#[cfg(test)]
mod tests {
    use num_enum::TryFromPrimitive;
    use serde_cbor::Value;

    use super::*;

    fn rad_error_example(radon_errors: RadonErrors) -> RadError {
        match radon_errors {
            RadonErrors::UnsupportedOperator => RadError::UnsupportedOperator {
                input_type: "RadonString".to_string(),
                operator: "IntegerAdd".to_string(),
                args: Some(vec![SerdeCborValue::Integer(1)]),
            },
            RadonErrors::HttpErrors => RadError::HttpStatus { status_code: 404 },
            RadonErrors::InsufficientMajority => RadError::InsufficientMajority {
                achieved: 49.0,
                required: 51.0,
            },
            RadonErrors::TallyExecution => RadError::TallyExecution {
                inner: None,
                message: Some("Only the message field is serialized".to_string()),
            },
            RadonErrors::ArrayIndexOutOfBounds => RadError::ArrayIndexOutOfBounds { index: 2 },
            RadonErrors::MapKeyNotFound => RadError::MapKeyNotFound {
                key: String::from("value"),
            },
            RadonErrors::UnhandledIntercept => RadError::UnhandledIntercept {
                inner: None,
                message: Some("Only the message field is serialized".to_string()),
            },
            RadonErrors::BufferIsNotValue => RadError::BufferIsNotValue {
                description: "Buffer is no value".to_string(),
            },
            // If this panics after adding a new `RadonTypes`, add a new example above
            _ => panic!("No example for {:?}", radon_errors),
        }
    }

    // Return an iterator that visits all the variants of `RadonErrors`
    // There are some crates that provide this functionality as a derive macro,
    // for example "strum", so if we need more enum iterators in the future,
    // consider using an external crate
    fn all_radon_errors() -> impl Iterator<Item = RadonErrors> {
        // RadonErrors are an enum with `u8` discriminant
        // So just try all the possible `u8` values and return the successful ones
        (0u8..=255).filter_map(|error_code| {
            match RadonErrors::try_from_primitive(error_code) {
                Ok(x)
                    if x == RadonErrors::BridgeMalformedRequest
                        || x == RadonErrors::BridgePoorIncentives
                        || x == RadonErrors::BridgeOversizedResult =>
                {
                    // We skip these RadonErrors because they don't belong to the core witnessing protocol
                    None
                }
                Ok(x) => Some(x),
                // If this error code is not a RadonErrors, try the next one
                Err(_) => None,
            }
        })
    }

    #[test]
    fn all_radon_errors_can_be_converted_to_rad_error() {
        for radon_errors in all_radon_errors() {
            // Try to convert RadonErrors to RadError with no arguments
            let maybe_rad_error =
                RadError::try_into_radon_error_before_wip0028(radon_errors, None).map(|r| r.into_inner());
            let rad_error = match maybe_rad_error {
                Ok(x) => {
                    // Good
                    x
                }
                Err(RadError::DecodeRadonErrorMissingArguments) => {
                    // Good, but we need some test arguments
                    rad_error_example(radon_errors)
                }
                Err(e) => panic!("RadonErrors::{:?}: {}", radon_errors, e),
            };

            // Now try the inverse: convert from RadError to RadonErrors
            assert_eq!(rad_error.into_error_code(), radon_errors);
        }
    }

    #[test]
    fn all_radon_errors_can_be_serialized() {
        for radon_errors in all_radon_errors() {
            // Try to convert RadonErrors to RadError with no arguments
            let maybe_rad_error =
                RadError::try_into_radon_error_before_wip0028(radon_errors, None).map(|r| r.into_inner());
            let rad_error = match maybe_rad_error {
                Ok(x) => {
                    // Good
                    x
                }
                Err(RadError::DecodeRadonErrorMissingArguments) => {
                    // Good, but we need some test arguments
                    rad_error_example(radon_errors)
                }
                Err(e) => panic!("RadonErrors::{:?}: {}", radon_errors, e),
            };

            // Now try to serialize the resulting rad_error
            let serde_cbor_array = match rad_error.try_into_cbor_array_before_wip0028() {
                Ok(x) => x,
                Err(e) => panic!("RadonErrors::{:?}: {}", radon_errors, e),
            };
            // The first element of the serialized CBOR array is the error code
            // the rest are arguments
            let error_code = u8::from(radon_errors);
            assert_eq!(serde_cbor_array[0], Value::Integer(error_code.into()));

            // Deserialize the result and compare
            println!("{:?}", serde_cbor_array[0]);
            let deserialized_rad_error =
                RadError::try_from_cbor_array(serde_cbor_array).map(|r| r.into_inner());

            match deserialized_rad_error {
                Ok(x) => assert_eq!(x, rad_error),
                Err(e) => panic!("RadonErrors::{:?}: {}", radon_errors, e),
            }
        }
    }

    #[test]
    fn unhandled_intercept_wrong_single_quote_escape() {
        use crate::RadonString;

        // Try to convert RadonErrors to RadError with no arguments
        let rad_error = RadError::UnhandledIntercept {
            inner: Some(Box::new(RadError::ModeTie {
                values: RadonArray::from(vec![
                    RadonTypes::String(RadonString::from("'")),
                    RadonTypes::String(RadonString::from("Cat's")),
                ]),
                max_count: 1,
            })),
            message: None,
        };

        // Now try to serialize the resulting rad_error
        let serde_cbor_array = rad_error.try_into_cbor_array_before_wip0028().unwrap();
        // The first element of the serialized CBOR array is the error code
        // the rest are arguments
        let error_code = u8::from(RadonErrors::UnhandledIntercept);
        assert_eq!(serde_cbor_array[0], Value::Integer(error_code.into()));

        // Deserialize the result and compare
        let deserialized_rad_error =
            RadError::try_from_cbor_array(serde_cbor_array).map(|r| r.into_inner());

        let expected_rad_error = RadError::UnhandledIntercept {
            inner: None,
            message: Some(r#"inner: ModeTie { values: RadonArray { value: [String(RadonString { value: "\'" }), String(RadonString { value: "Cat\'s" })], is_homogeneous: true }, max_count: 1 }"#.to_string()),
        };

        assert_eq!(deserialized_rad_error.unwrap(), expected_rad_error);
    }
}
