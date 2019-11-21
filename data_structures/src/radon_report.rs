use std::convert::TryFrom;
use std::time::{Duration, Instant};

use crate::radon_error::{ErrorLike, RadonError};

/// A high level data structure aimed to be used as the return type of RAD executor methods:
///
/// > fn run_xxxxx_stage(input: RadonTypes, script: &[Value]) -> Result<RadonReport, RadError> {}
///
/// It contains a RAD result paired with metadata specific to the stage of the script being executed.
///
/// `RT` is the generalization of `RadonTypes`, and `IE` is the generalization of `RadError`
#[derive(Clone, Debug)]
pub struct RadonReport<RT>
where
    RT: TypeLike,
{
    /// Stage-specific metadata.
    pub metadata: Stage,
    /// This is raw result: either a RadonTypes or a RadonError.
    pub result: Result<RT, RadonError<RT::Error>>,
    /// Keep track of how many milliseconds did the execution take to complete or fail.
    pub running_time: Duration,
}

/// Implementations, factories and convenience methods for `RadonReport`.
impl<RT> RadonReport<RT>
where
    RT: TypeLike,
{
    /// Factory for constructing a `RadonReport` from the `Result` of something that could be
    /// `ErrorLike` plus a `ReportContext`.
    pub fn from_result(
        result: Result<RT, RT::Error>,
        context: &ReportContext,
    ) -> Result<Self, RT::Error> {
        let result = RT::Error::intercept(result);

        Ok(RadonReport {
            result,
            metadata: context.stage.clone(),
            running_time: context.duration(),
        })
    }

    /// Recover a `Result` in the likes of `Result<RadonTypes, RadError>` from a `RadonReport`.
    pub fn into_inner(self) -> Result<RT, RT::Error> {
        self.result
            .map_err(|radon_error| radon_error.inner.unwrap_or_default())
    }
}

/// This is the main serializer for turning `RadonReport` into a CBOR-encoded byte stream that can be
/// consumed by any Witnet library, e.g. the `UsingWitnet` solidity contract.
impl<RT> TryFrom<&RadonReport<RT>> for Vec<u8>
where
    RT: TypeLike,
{
    type Error = RT::Error;

    fn try_from(report: &RadonReport<RT>) -> Result<Self, Self::Error> {
        match report.result {
            Ok(ref radon_types) => radon_types.encode(),
            Err(ref radon_error) => radon_error.encode(),
        }
    }
}

/// This trait identifies a RADON-compatible type system, i.e. most likely an `enum` with different
/// cases for different data types.
pub trait TypeLike {
    type Error: ErrorLike;

    fn encode(&self) -> Result<Vec<u8>, Self::Error>;
}

/// A generic structure for bubbling up any kind of metadata that may be generated during the
/// execution of a RADON script.
#[derive(Default)]
pub struct ReportContext {
    pub call_arguments: Option<Vec<serde_cbor::Value>>,
    pub call_argument_index: Option<u8>,
    pub call_index: Option<u8>,
    pub call_operator: Option<u8>,
    pub stage: Stage,
    pub completion_time: Option<Instant>,
    pub start_time: Option<Instant>,
    pub script_index: Option<u8>,
}

/// Implementation of convenience methods for `ReportContext`
impl ReportContext {
    /// Set start time.
    pub fn start(&mut self) {
        self.start_time = Some(Instant::now());
    }

    /// Set completion time.
    pub fn complete(&mut self) {
        self.completion_time = Some(Instant::now())
    }

    /// Compute difference between start and completion time.
    fn duration(&self) -> Duration {
        match (self.start_time, self.completion_time) {
            (Some(start_time), Some(completion_time)) => completion_time.duration_since(start_time),
            _ => Duration::default(),
        }
    }
}

/// Tell different stage-specific metadata structures from each other.
#[derive(Clone, Debug)]
pub enum Stage {
    /// Metadata for contextless execution of RADON scripts.
    Contextless,
    /// Metadata for Retrieval stage.
    Retrieval,
    /// Metadata for Aggregation stage.
    Aggregation,
    /// Metadata for Tally stage.
    Tally(TallyMetaData),
}

/// Implementation of the default value of `Stage`.
impl Default for Stage {
    fn default() -> Self {
        Stage::Contextless
    }
}

// This structure is not needed yet but it is here just in case we need it in the future.
///// Retrieval-specific metadata structure.
//pub struct RetrievalMetaData {}

// This structure is not needed yet but it is here just in case we need it in the future.
///// Retrieval-specific metadata structure.
//pub struct AggregationMetaData {}

/// Tally-specific metadata structure.
#[derive(Clone, Debug, Default)]
pub struct TallyMetaData {
    /// A positional vector of "truthers" and "liars", i.e. reveals that passed all the filters vs.
    /// those which were filtered out.
    /// This follows a reverse logic: `false` is truth and `true` is lie.
    liars: Vec<bool>,
    /// Proportion between total reveals and "truthers" count:
    /// `reveals.len() / liars.iter().filter(std::not::Ops).count()`
    consensus: f32,
}

#[test]
fn test_encode_not_cbor() {
    use crate::radon_error::{RadonError, RadonErrors};

    #[derive(Default, Debug)]
    struct Dummy;

    // Satisfy the trait bound `Dummy: radon_error::ErrorLike` required by `radon_error::RadonError`
    impl ErrorLike for Dummy {
        fn intercept<RT>(value: Result<RT, Self>) -> Result<RT, RadonError<Self>> {
            value.map_err(RadonError::from)
        }
    }

    // Satisfy the trait bound `(): std::convert::From<cbor::encoder::EncodeError>`
    impl std::convert::From<cbor::encoder::EncodeError> for Dummy {
        fn from(_: cbor::encoder::EncodeError) -> Self {
            Dummy
        }
    }

    let error = RadonError::<Dummy> {
        kind: RadonErrors::SourceScriptNotCBOR,
        arguments: vec![cbor::value::Value::U8(2)],
        inner: None,
    };

    let encoded = error.encode().unwrap();
    let expected = vec![216, 37, 130, 1, 2];

    assert_eq!(encoded, expected);
}
