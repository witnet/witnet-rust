use std::convert::TryInto;
use std::time::Duration;

use crate::rad_error::RadError;
use crate::radon_error::RadonError;
use crate::types::RadonTypes;

/// A high level data structure aimed to be used as the return type of RAD executor methods:
///
/// > fn run_xxxxx_stage(input: RadonTypes, script: &[Value]) -> Result<Report, RadError> {}
///
/// It contains a RAD result paired with metadata specific to the stage of the script being executed.
pub struct Report {
    /// This is raw result: either a RadonTypes or a RadonError.
    result: Result<RadonTypes, RadonError>,
    /// Keep track of how many milliseconds did the execution take to complete or fail.
    running_time_milliseconds: Duration,
    /// Stage-specific metadata.
    metadata: Metadata,
}

/// Tell different stage-specific metadata structures from each other.
pub enum Metadata {
    /// Metadata for Retrieval stage.
    Retrieval(RetrievalMetaData),
    /// Metadata for Aggregation stage.
    Aggregation(AggregationMetaData),
    /// Metadata for Tally stage.
    Tally(TallyMetaData),
}

/// Retrieval-specific metadata structure.
pub struct RetrievalMetaData {}

/// Retrieval-specific metadata structure.
pub struct AggregationMetaData {}

/// Tally-specific metadata structure.
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
    use cbor::value::Value;
    use crate::radon_error::RadonErrors;

    let error = RadonError {
        kind: RadonErrors::SourceScriptNotCBOR,
        arguments: vec![Value::U8(2)],
    };

    let encoded = error.encode().unwrap();
    let expected = vec![216, 37, 130, 1, 2];

    assert_eq!(encoded, expected);
}
