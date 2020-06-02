use std::convert::TryFrom;
use std::time::{Duration, Instant};

use crate::radon_error::ErrorLike;

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
    /// Vector of partial results (the results in between each of the operators in a script)
    pub partial_results: Option<Vec<RT>>,
    /// This the intercepted result of the script execution: any `IE` raised in runtime has already
    /// been mapped into a `RT` (e.g. `RadError` -> `RadonTypes::RadonError`.
    pub result: RT,
    /// Keep track of how many milliseconds did the execution take to complete or fail.
    pub running_time: Duration,
}

/// Implementations, factories and convenience methods for `RadonReport`.
impl<RT> RadonReport<RT>
where
    RT: TypeLike,
{
    /// Factory for constructing a `RadonReport` from the `Result` of something that could be
    /// `TypeLike` or `ErrorLike` plus a `ReportContext`.
    pub fn from_result(result: Result<RT, RT::Error>, context: &ReportContext) -> Self {
        let intercepted = RT::intercept(result);
        RadonReport {
            metadata: context.stage.clone(),
            partial_results: None,
            result: intercepted,
            running_time: context.duration(),
        }
    }

    /// Factory for constructing a `RadonReport` from a vector of partial results, which could be
    /// `TypeLike` or `ErrorLike`, plus a `ReportContext`.
    pub fn from_partial_results(
        partial_results: Vec<Result<RT, RT::Error>>,
        context: &ReportContext,
    ) -> Self {
        let intercepted: Vec<RT> = partial_results.into_iter().map(RT::intercept).collect();
        let result = match intercepted.last() {
            None => unreachable!("Partial result vectors always contain at least 1 item"),
            Some(x) => (*x).clone(),
        };

        RadonReport {
            metadata: context.stage.clone(),
            partial_results: Some(intercepted),
            result,
            running_time: context.duration(),
        }
    }

    /// Recover the inner result as a `RT` from a `RadonReport`.
    pub fn into_inner(self) -> RT {
        self.result
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
        report.result.encode()
    }
}

/// This trait identifies a RADON-compatible type system, i.e. most likely an `enum` with different
/// cases for different data types.
pub trait TypeLike: std::clone::Clone + std::marker::Sized {
    type Error: ErrorLike;

    fn encode(&self) -> Result<Vec<u8>, Self::Error>;
    fn intercept(result: Result<Self, Self::Error>) -> Self;
}

/// A generic structure for bubbling up any kind of metadata that may be generated during the
/// execution of a RADON script.
#[derive(Default)]
pub struct ReportContext {
    pub call_arguments: Option<Vec<serde_cbor::Value>>,
    pub call_argument_index: Option<u8>,
    pub call_index: Option<u8>,
    pub call_operator: Option<u8>,
    pub completion_time: Option<Instant>,
    pub stage: Stage,
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

    /// Create a context that is initialized for a particular stage
    pub fn from_stage(stage: Stage) -> Self {
        let mut new = Self::default();
        new.stage = stage;

        new
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
    /// A liar is an out-of-consensus value
    pub liars: Vec<bool>,
    /// An error is a RadonError value (or considered as an error due to a RadonError consensus)
    pub errors: Vec<bool>,
    /// Proportion between total reveals and "truthers" count:
    /// `liars.iter().filter(std::ops::Not).count() / reveals.len()`
    pub consensus: f32,
}

impl TallyMetaData {
    /// Update liars vector
    /// new_liars length has to be less than false elements in liars
    // FIXME: Allow for now, since there is no safe cast function from a usize to float yet
    #[allow(clippy::cast_precision_loss)]
    pub fn update_liars(&mut self, new_liars: Vec<bool>) {
        if self.liars.is_empty() {
            self.liars = new_liars;
        } else {
            let mut new_iter = new_liars.iter();

            for liar in &mut self.liars {
                if !*liar {
                    *liar = *new_iter.next().unwrap();
                }
            }

            assert!(new_iter.next().is_none());

            self.consensus = self.liars.iter().fold(0., |count, liar| match liar {
                true => count,
                false => count + 1.,
            }) / self.liars.len() as f32;
        }
    }
}

#[cfg(test)]
mod tests {
    use std::fmt;

    use failure::Fail;

    use crate::radon_error::{ErrorLike, RadonError, RadonErrors};

    use super::*;
    use serde_cbor::Value as SerdeCborValue;

    #[test]
    fn test_encode_not_cbor() {
        #[derive(Clone, Default, Debug, Fail)]
        struct Dummy;

        // Satisfy the trait bound `Dummy: fmt::Display` required by `failure::Fail`
        impl fmt::Display for Dummy {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                writeln!(f, "Error")
            }
        }

        // Satisfy the trait bound `Dummy: radon_error::ErrorLike` required by `radon_error::RadonError`
        impl ErrorLike for Dummy {
            fn encode_cbor_array(&self) -> Result<Vec<SerdeCborValue>, failure::Error> {
                let kind = u8::from(RadonErrors::SourceScriptNotCBOR);
                let arg0 = 2;

                Ok(vec![
                    SerdeCborValue::Integer(kind.into()),
                    SerdeCborValue::Integer(arg0.into()),
                ])
            }

            fn decode_cbor_array(
                _serde_cbor_array: Vec<SerdeCborValue>,
            ) -> Result<RadonError<Self>, failure::Error> {
                unimplemented!()
            }
        }

        // Satisfy the trait bound `(): std::convert::From<cbor::encoder::EncodeError>`
        impl std::convert::From<cbor::encoder::EncodeError> for Dummy {
            fn from(_: cbor::encoder::EncodeError) -> Self {
                Dummy
            }
        }

        let error = RadonError::new(Dummy);

        let encoded: Vec<u8> = error.encode_tagged_bytes().unwrap();
        let expected = vec![216, 39, 130, 1, 2];

        assert_eq!(encoded, expected);
    }

    #[test]
    fn test_update_liars() {
        let mut metadata = TallyMetaData::default();
        // [1,1,0,1,0,0,0,1,0,0] => 6 false values
        metadata.liars = vec![
            true, true, false, true, false, false, false, true, false, false,
        ];

        // [0,1,1,0,0,1]
        let v = vec![false, true, true, false, false, true];

        metadata.update_liars(v);

        // [1,1,0,1,1,1,0,1,0,1] => 3 false values
        let expected = vec![
            true, true, false, true, true, true, false, true, false, true,
        ];

        assert_eq!(metadata.liars, expected);
    }
}
