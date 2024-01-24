use std::io::Cursor;

use cbor::{types::Tag, value::Value as CborValue, GenericEncoder};
use failure::Fail;
use num_enum::{IntoPrimitive, TryFromPrimitive};
use serde::Serialize;
use serde_cbor::Value as SerdeCborValue;

use crate::chain::tapi::ActiveWips;

#[derive(Clone, Copy, Debug, Eq, IntoPrimitive, PartialEq, Serialize, TryFromPrimitive)]
#[repr(u8)]
/// List of RADON-level errors.
/// **WARNING: these codes are consensus-critical.** They can be renamed but they cannot be
/// re-assigned without causing a non-backwards-compatible protocol upgrade.
pub enum RadonErrors {
    /// Unknown error. Something went really bad!
    Unknown = 0x00,
    
    ///////////////////////////////////////////////////////////////////////////
    // Script format error sub-codes
    /// At least one of the source scripts is not a valid CBOR-encoded value.
    SourceScriptNotCBOR = 0x01,
    /// The CBOR value decoded from a source script is not an Array.
    SourceScriptNotArray = 0x02,
    /// The Array value decoded form a source script is not a valid RADON script.
    SourceScriptNotRADON = 0x03,
    /// The request body of at least one data source was not properly formated.
    SourceRequestBody = 0x04,
    /// The request headers of at least one data source was not properly formated.
    SourceRequestHeaders = 0x05,
    /// The request URL of at least one data source was not properly formated.
    SourceRequestURL = 0x06,
    
    ///////////////////////////////////////////////////////////////////////////
    // Complexity error sub-codes
    /// The request contains too many sources.
    RequestTooManySources = 0x10,
    /// The script contains too many calls.
    ScriptTooManyCalls = 0x11,
    
    ///////////////////////////////////////////////////////////////////////////
    // Lack of support error sub-codes
    /// Some Radon operator opcode is not currently supported.
    UnsupportedOperator = 0x20,
    /// Some Radon filter opcode is not currently supported.
    UnsupportedFilter = 0x21,
    /// Some Radon hash function is not currently supported.
    UnsupportedHashFunction = 0x22,
    /// Some Radon reducer opcode is not currently supported.
    UnsupportedReducer = 0x23,
    /// Some Radon request type is not currently supported.
    UnsupportedRequestType = 0x24,
    /// Some Radon encoding function is not currently supported.
    UnsupportedEncodingFunction = 0x25,
    /// Wrong number (or type) of arguments were passed to some Radon operator.
    WrongArguments = 0x28,

    ///////////////////////////////////////////////////////////////////////////
    // Retrieval-specific error sub-codes
    /// A majority of data sources returned an HTTP status code other than 200.
    HttpErrors = 0x30,
    /// A majority of data sources timed out.
    RetrievalsTimeout = 0x31,

    ///////////////////////////////////////////////////////////////////////////
    // Script-specific error sub-codes
    /// Math operator caused an underflow.
    MathUnderflow = 0x40,
    /// Math operator caused an overflow.
    MathOverflow = 0x41,
    /// Tried to divide by zero.
    MathDivisionByZero = 0x42,
    /// Wrong input to subscript call.
    WrongSubscriptInput = 0x43,
    /// Value cannot be extracted from input binary buffer.
    BufferIsNotValue = 0x44,
    /// Value cannot be decoded from expected type.
    Decode = 0x45,
    /// Unexpected empty array.
    EmptyArray = 0x46, 
    /// Value cannot be encoded to expected type.
    Encode = 0x47,
    /// Failed to filter input values.
    Filter = 0x48, 
    /// Failed to hash input value.
    Hash = 0x49,
    /// Mismatching array ranks.
    MismatchingArrays = 0x4A,
    /// Failed to process non-homogenous array.
    NonHomogeneousArrays = 0x4B, 
    /// Failed to parse syntax of some input value, or argument.
    Parse = 0x4C,
    /// Parsing logic limits were exceeded.
    ParseOverflow = 0x4D,
    /// Failed to reduce input values.
    Reduce = 0x4E,
    
    ///////////////////////////////////////////////////////////////////////////
    // Actual result first-order error codes that can be included in a Tally.
    /// Not enough reveal quorum was reached on tally stage.
    InsufficientQuorum = 0x50,
    /// No actual reveal majority was reached on tally stage.
    InsufficientMajority = 0x51,
    /// Not enough commits were received before tally stage.
    InsufficientCommits = 0x52,
    /// Generic error during tally execution.
    TallyExecution = 0x53,
    /// Some data sources could either be temporarily unresponsive or failing to report the requested data:
    CircumstantialFailure = 0x54,
    /// At least one data source is inconsistent when queried through multiple transports at once:
    InconsistentSources = 0x55,
    /// Values returned from a majority of data sources did not match the expected schema:
    MalformedResponses = 0x56,
    /// The data request was not properly formated:
    MalformedDataRequest = 0x57,
    /// The size of serialized tally result exceeds allowance:
    OversizedTallyResult = 0x5F,
    

    ///////////////////////////////////////////////////////////////////////////
    // Inter-stage runtime error sub-codes
    /// Data aggregation reveals could not get decoded on tally stage:
    MalformedReveals = 0x60,
    /// The result to data aggregation could not get encoded:
    EncodeReveals = 0x61,
    /// A mode tie ocurred when calculating the mode value on aggregation stage:
    ModeTie = 0x62, 

    ///////////////////////////////////////////////////////////////////////////
    // Runtime access error sub-codes
    /// Tried to access a value from an index using an index that is out of bounds.
    ArrayIndexOutOfBounds = 0x70,
    /// Tried to access a value from a map using a key that does not exist.
    MapKeyNotFound = 0x71,
    /// Tried to extract value from a map using a JSON Path that returns no values.
    JsonPathNotFound = 0x72,
    
    ///////////////////////////////////////////////////////////////////////////
    // Inter-client first-order error codes.
    /// Requests that cannot be relayed into the Witnet blockchain should be reported
    /// with one of these errors. 
    BridgeMalformedRequest = 0xE0,
    /// The request is rejected on the grounds that it may cause the submitter to spend or stake an
    /// amount of value that is unjustifiably high when compared with the reward they will be getting
    BridgePoorIncentives = 0xE1,
    /// The request result length exceeds a bridge contract defined limit
    BridgeOversizedResult = 0xE2,
    
    // This should never happen:
    /// Some tally error is not intercepted but should
    UnhandledIntercept = 0xFF,
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
    /// Encode the error as an array of `SerdeCborValue`
    fn encode_cbor_array(&self, active_wips: &Option<ActiveWips>) -> Result<Vec<SerdeCborValue>, failure::Error>;
    /// Decode the error from an array of `SerdeCborValue`
    fn decode_cbor_array(serde_cbor_array: Vec<SerdeCborValue>) -> Result<RadonError<Self>, failure::Error>;
}

/// This structure is aimed to be the error type for the `result` field of `witnet_data_structures::radon_report::Report`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
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

    /// Encode `RadonError` as tagged CBOR value with tag 39.
    /// Returns the result as `CborValue`.
    pub fn encode_tagged_value(&self, active_wips: &Option<ActiveWips>) -> Result<CborValue, failure::Error> {
        let values: Vec<CborValue> = self
            .inner
            .encode_cbor_array(active_wips)?
            .into_iter()
            .map(|scv| {
                // FIXME(#953): remove this conversion
                try_from_serde_cbor_value_for_cbor_value(scv)
            })
            .collect();

        Ok(CborValue::Tagged(
            Tag::of(39),
            Box::new(CborValue::Array(values)),
        ))
    }

    /// Encode `RadonErorr` as tagged CBOR value with tag 39.
    /// Returns the result as bytes.
    pub fn encode_tagged_bytes(&self, active_wips: &Option<ActiveWips>) -> Result<Vec<u8>, failure::Error> {
        let mut encoder = GenericEncoder::new(Cursor::new(Vec::new()));
        encoder.value(&self.encode_tagged_value(active_wips)?)?;

        Ok(encoder.into_inner().into_writer().into_inner())
    }

    /// Get a reference to the inner error type
    pub fn inner(&self) -> &IE {
        &self.inner
    }

    /// Unwrap the inner error type
    pub fn into_inner(self) -> IE {
        self.inner
    }
}

impl<IE> std::fmt::Display for RadonError<IE>
where
    IE: ErrorLike,
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "RadonError({:?})", self.inner)
    }
}

/// Convert SerdeCborValue into CborValue
pub fn try_from_serde_cbor_value_for_cbor_value(serde_cbor_value: SerdeCborValue) -> CborValue {
    // FIXME(#953): impl TryFrom<SerdeCborValue> for <CborValue>
    let mut decoder = cbor::decoder::GenericDecoder::new(
        cbor::Config::default(),
        std::io::Cursor::new(serde_cbor::to_vec(&serde_cbor_value).unwrap()),
    );
    decoder.value().unwrap()
}

/// Convert CborValue into SerdeCborValue
pub fn try_from_cbor_value_for_serde_cbor_value(cbor_value: CborValue) -> SerdeCborValue {
    // FIXME(#953): impl TryFrom<CborValue> for <SerdeCborValue>
    let mut encoder = cbor::encoder::GenericEncoder::new(Cursor::new(Vec::new()));
    encoder.value(&cbor_value).unwrap();
    let buffer = encoder.into_inner().into_writer().into_inner();

    serde_cbor::from_slice(&buffer).unwrap()
}
