use std::convert::{TryFrom, TryInto};
use std::time::{Duration, Instant};

use cbor::value::Value;
use serde_cbor;

use crate::rad_error::RadError;
use crate::radon_error::{RadonError, RadonErrors};
use crate::types::RadonTypes;

/// A high level data structure aimed to be used as the return type of RAD executor methods:
///
/// > fn run_xxxxx_stage(input: RadonTypes, script: &[Value]) -> Result<Report, RadError> {}
///
/// It contains a RAD result paired with metadata specific to the stage of the script being executed.
#[derive(Clone, Debug)]
pub struct Report {
    /// Stage-specific metadata.
    pub metadata: Stage,
    /// This is raw result: either a RadonTypes or a RadonError.
    pub result: Result<RadonTypes, RadonError>,
    /// Keep track of how many milliseconds did the execution take to complete or fail.
    pub running_time: Duration,
}

/// Implementation of convenience methods for `Report`.
impl Report {
    /// Construct a `Report` structure from a `Result` and a `ReportContext`.
    pub fn from_result(
        raw_result: Result<RadonTypes, RadError>,
        context: &mut ReportContext,
    ) -> Result<Self, RadError> {
        let result = match raw_result {
            Err(error) => Err(match error {
                // TODO: support all cases of `RadError`
                RadError::HttpStatus { status_code } => RadonError {
                    kind: RadonErrors::HTTPError,
                    arguments: vec![Value::U8(status_code as u8)],
                    inner: Some(error.clone()),
                },
                _ => return Err(error),
            }),
            Ok(ok) => Ok(ok),
        };

        Ok(Report {
            result,
            metadata: context.stage.clone(),
            running_time: context.duration(),
        })
    }
}

/// Try to extract the result from a `Report`
impl TryFrom<Report> for RadonTypes {
    type Error = RadError;

    fn try_from(value: Report) -> Result<Self, Self::Error> {
        value.result.clone().map_err(Self::Error::from)
    }
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

/// This is the main serializer for turning `Report` into a CBOR-encoded byte stream that can be
/// consumed by any Witnet library, e.g. the `UsingWitnet` solidity contract.
impl TryInto<Vec<u8>> for Report {
    type Error = RadError;

    fn try_into(self) -> Result<Vec<u8>, Self::Error> {
        match self.result {
            Ok(radon_types) => radon_types.try_into(),
            Err(error_code) => error_code.encode(),
        }
    }
}

#[test]
fn test_encode_not_cbor() {
    use crate::radon_error::RadonErrors;
    use cbor::value::Value;

    let error = RadonError {
        kind: RadonErrors::SourceScriptNotCBOR,
        arguments: vec![Value::U8(2)],
        inner: None,
    };

    let encoded = error.encode().unwrap();
    let expected = vec![216, 37, 130, 1, 2];

    assert_eq!(encoded, expected);
}
