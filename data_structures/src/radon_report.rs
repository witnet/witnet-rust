use std::convert::TryFrom;
use std::time::{Duration, SystemTime};

use serde::Serialize;

use crate::radon_error::ErrorLike;

/// A high level data structure aimed to be used as the return type of RAD executor methods:
///
/// > fn run_xxxxx_stage(input: RadonTypes, script: &[Value]) -> Result<RadonReport, RadError> {}
///
/// It contains a RAD result paired with metadata specific to the stage of the script being executed.
///
/// `RT` is the generalization of `RadonTypes`, and `IE` is the generalization of `RadError`
#[derive(Clone, Debug, Serialize)]
pub struct RadonReport<RT>
where
    RT: TypeLike,
{
    /// Execution details, including stage-specific metadata.
    pub context: ReportContext<RT>,
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
    pub fn from_result(result: Result<RT, RT::Error>, context: &ReportContext<RT>) -> Self {
        let intercepted = RT::intercept(result);
        RadonReport {
            context: context.clone(),
            partial_results: None,
            result: intercepted,
            running_time: context.duration(),
        }
    }

    /// Factory for constructing a `RadonReport` from a vector of partial results, which could be
    /// `TypeLike` or `ErrorLike`, plus a `ReportContext`.
    pub fn from_partial_results(
        partial_results: Vec<Result<RT, RT::Error>>,
        context: &ReportContext<RT>,
    ) -> Self {
        let intercepted: Vec<RT> = partial_results.into_iter().map(RT::intercept).collect();
        let result = (*intercepted
            .last()
            .expect("Partial result vectors must always contain at least 1 item"))
        .clone();

        RadonReport {
            context: context.clone(),
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
pub trait TypeLike: Clone + Sized {
    /// A structure that can be used as an error type.
    type Error: ErrorLike;

    /// Serialize the `TypeLike` as a `Vec<u8>`.
    fn encode(&self) -> Result<Vec<u8>, Self::Error>;

    /// Eases interception of RADON errors (errors that we want to commit, reveal and tally) so
    /// they can be handled as valid `RadonTypes::RadonError` values, which are subject to
    /// commitment, revealing, tallying, etc.
    fn intercept(result: Result<Self, Self::Error>) -> Self;
}

/// A generic structure for bubbling up any kind of metadata that may be generated during the
/// execution of a RADON script. This is specially useful for tracing errors back to specific calls
/// in scripts.
///
/// The word "processed" in the structure member descriptions implies that the execution was
/// successful up to the preceding call or argument, and that the parsing and execution of the
/// reported call or argument index did at least began.
///
/// Scripts that are executed successfully in their full extent will therefore report here the
/// index of the last call in the script. On the contrary, scripts that fail will report the index
/// of the call that failed.
#[derive(Clone, Debug, Serialize)]
pub struct ReportContext<RT>
where
    RT: TypeLike,
{
    /// The arguments of the last call that has been processed.
    pub call_arguments: Option<Vec<serde_cbor::Value>>,
    /// The index of the last argument in a call that has been processed.
    pub call_argument_index: Option<usize>,
    /// The index of the last call that has been processed.
    pub call_index: Option<usize>,
    /// The operator in the last call that has been processed.
    pub call_operator: Option<usize>,
    /// The timestamp when the execution of the script finished.
    pub completion_time: Option<SystemTime>,
    /// Metadata that is specific to the stage of the script.
    pub stage: Stage<RT>,
    /// The timestamp when the execution of the script began.
    pub start_time: Option<SystemTime>,
    /// The index of the last script or subscript in a stage that has been processed.
    pub script_index: Option<usize>,
}

impl<RT> Default for ReportContext<RT>
where
    RT: TypeLike,
{
    fn default() -> Self {
        Self {
            call_arguments: None,
            call_argument_index: None,
            call_index: None,
            call_operator: None,
            completion_time: None,
            stage: Stage::Contextless,
            start_time: None,
            script_index: None,
        }
    }
}

/// Implementation of convenience methods for `ReportContext`
impl<RT> ReportContext<RT>
where
    RT: TypeLike,
{
    /// Set start time.
    pub fn start(&mut self) {
        self.start_time = Some(SystemTime::now());
    }

    /// Set completion time.
    pub fn complete(&mut self) {
        self.completion_time = Some(SystemTime::now())
    }

    /// Compute difference between start and completion time.
    fn duration(&self) -> Duration {
        match (self.start_time, self.completion_time) {
            (Some(start_time), Some(completion_time)) => completion_time
                .duration_since(start_time)
                .expect("Completion time should always be equal or greater than start time"),
            _ => Duration::default(),
        }
    }

    /// Create a context that is initialized for a particular stage
    pub fn from_stage(stage: Stage<RT>) -> Self {
        Self {
            stage,
            ..Default::default()
        }
    }
}

/// Tell different stage-specific metadata structures from each other.
#[derive(Clone, Debug, Serialize)]
pub enum Stage<RT>
where
    RT: TypeLike,
{
    /// Metadata for Aggregation stage.
    Aggregation,
    /// Metadata for contextless execution of RADON scripts.
    Contextless,
    /// Metadata for Retrieval stage.
    Retrieval(RetrievalMetadata<RT>),
    /// Metadata for Tally stage.
    Tally(TallyMetaData<RT>),
}

/// Implementation of the default value of `Stage`.
impl<RT> Default for Stage<RT>
where
    RT: TypeLike,
{
    fn default() -> Self {
        Stage::Contextless
    }
}

/// Retrieval and aggregation specific metadata structure.
#[derive(Clone, Debug, Serialize)]
pub struct RetrievalMetadata<RT>
where
    RT: TypeLike,
{
    /// Partial results of the subscripts, if enabled. It has 3 dimensions:
    ///
    /// `subscript_partial_results[subscript_index][operator_index][element_index]`
    ///
    /// * `subscript_index` is used to distinguish the different subscripts in one RADON script.
    /// * `operator_index` is the index of the operator inside the subscript: the partial result.
    ///     The first element is always the input value, and the last element is the result of the
    ///     subscript.
    /// * `element_index` is the index of the element inside the array that serves as the input of
    ///     the subscript.
    pub subscript_partial_results: Vec<Vec<Vec<RT>>>,
}

impl<RT> Default for RetrievalMetadata<RT>
where
    RT: TypeLike,
{
    fn default() -> Self {
        Self {
            subscript_partial_results: vec![],
        }
    }
}

// This structure is not needed yet but it is here just in case we need it in the future.
///// Retrieval-specific metadata structure.
//pub struct AggregationMetaData {}

/// Tally-specific metadata structure.
#[derive(Clone, Debug, Serialize)]
pub struct TallyMetaData<RT>
where
    RT: TypeLike,
{
    /// Proportion between total reveals and "truthers" count:
    /// `liars.iter().filter(std::ops::Not).count() / reveals.len()`
    pub consensus: f32,
    /// An error is a RadonError value (or considered as an error due to a RadonError consensus)
    pub errors: Vec<bool>,
    /// A positional vector of "truthers" and "liars", i.e. reveals that passed all the filters vs.
    /// those which were filtered out.
    /// This follows a reverse logic: `false` is truth and `true` is lie.
    /// A liar is an out-of-consensus value
    pub liars: Vec<bool>,
    /// A positional vector of results for each of the operators contained in each of the subscripts
    /// that may exist in a tally function.
    pub subscript_partial_results: Vec<RT>,
}

impl<RT> Default for TallyMetaData<RT>
where
    RT: TypeLike,
{
    fn default() -> Self {
        Self {
            // Consensus is initialized to 100% because it is only updated when there are some lies
            consensus: 1.0,
            errors: vec![],
            liars: vec![],
            subscript_partial_results: vec![],
        }
    }
}

impl<RT> TallyMetaData<RT>
where
    RT: TypeLike,
{
    /// Update liars vector
    /// new_liars length has to be less than false elements in liars
    // FIXME: Allow for now, since there is no safe cast function from a usize to float yet
    #[allow(clippy::cast_precision_loss)]
    pub fn update_liars(&mut self, new_liars: Vec<bool>) {
        if self.liars.is_empty() {
            self.liars = new_liars;
        } else if !new_liars.is_empty() {
            let mut new_iter = new_liars.iter();

            for liar in &mut self.liars {
                if !*liar {
                    *liar = *new_iter.next().unwrap();
                }
            }

            assert!(new_iter.next().is_none());
        }

        // TODO: consensus will be NaN when self.liars.len() == 0
        self.consensus = self.liars.iter().fold(0., |count, liar| match liar {
            true => count,
            false => count + 1.,
        }) / self.liars.len() as f32;
    }
}

#[cfg(test)]
mod tests {
    use std::fmt;

    use failure::Fail;
    use serde_cbor::Value as SerdeCborValue;

    use crate::radon_error::{ErrorLike, RadonError, RadonErrors};

    use super::*;

    #[derive(Clone)]
    struct DummyType;

    #[derive(Clone, Default, Debug, Fail)]
    struct DummyError;

    impl TypeLike for DummyType {
        type Error = DummyError;

        fn encode(&self) -> Result<Vec<u8>, Self::Error> {
            unimplemented!()
        }

        fn intercept(_result: Result<Self, Self::Error>) -> Self {
            unimplemented!()
        }
    }

    // Satisfy the trait bound `Dummy: fmt::Display` required by `failure::Fail`
    impl fmt::Display for DummyError {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            writeln!(f, "Error")
        }
    }

    // Satisfy the trait bound `Dummy: radon_error::ErrorLike` required by `radon_error::RadonError`
    impl ErrorLike for DummyError {
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
    impl std::convert::From<cbor::encoder::EncodeError> for DummyError {
        fn from(_: cbor::encoder::EncodeError) -> Self {
            DummyError
        }
    }

    #[test]
    fn test_encode_not_cbor() {
        let error = RadonError::new(DummyError);

        let encoded: Vec<u8> = error.encode_tagged_bytes().unwrap();
        let expected = vec![216, 39, 130, 1, 2];

        assert_eq!(encoded, expected);
    }

    #[test]
    fn test_update_liars() {
        // [1,1,0,1,0,0,0,1,0,0] => 6 false values
        let liars = vec![
            true, true, false, true, false, false, false, true, false, false,
        ];

        let mut metadata = TallyMetaData::<DummyType> {
            consensus: 0.0,
            errors: vec![],
            liars,
            subscript_partial_results: vec![],
        };

        // [0,1,1,0,0,1]
        let v = vec![false, true, true, false, false, true];

        metadata.update_liars(v);

        // [1,1,0,1,1,1,0,1,0,1] => 3 false values
        let expected = vec![
            true, true, false, true, true, true, false, true, false, true,
        ];

        assert_eq!(metadata.liars, expected);

        // An empty vector of new liars should cause no changes to the existing list of of liars
        metadata.update_liars(vec![]);
        assert_eq!(metadata.liars, expected);
    }
}
