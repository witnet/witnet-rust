use std::collections::HashMap;
use witnet_data_structures::chain::{
    CommitOutput, DataRequestOutput, OutputPointer, RevealOutput, TallyOutput,
};
use witnet_data_structures::serializers::decoders::TryFrom;

/// State of data requests in progress (stored in memory)
pub struct DataRequestState {
    /// Data request output (contains all required information to process it)
    pub data_request: DataRequestOutput,
    /// List of outputs related to this data request
    pub info: DataRequestInfo,
    /// Current stage of this data request
    pub stage: DataRequestStage,
}

/// List of outputs related to a data request
#[derive(Debug, Default)]
pub struct DataRequestInfo {
    /// List of commitments to resolve the data request
    pub commits: HashMap<OutputPointer, CommitOutput>,
    /// List of reveals to the commitments (contains the data request witnet result)
    pub reveals: HashMap<OutputPointer, RevealOutput>,
    /// Tally of data request (contains final result)
    pub tally: Option<(OutputPointer, TallyOutput)>,
}

/// Data request information to be persisted into Storage (only for resolved data requests) and
/// using as index the Data Request OutputPointer
pub struct DataRequestInfoStorage {
    /// List of commitment output pointers to resolve the data request
    pub commits: Vec<OutputPointer>,
    /// List of reveal output pointers to the commitments (contains the data request result of the witnet)
    pub reveals: Vec<OutputPointer>,
    /// Tally output pointer (contains final result)
    pub tally: OutputPointer,
}

impl TryFrom<DataRequestInfo> for DataRequestInfoStorage {
    type Error = &'static str;

    fn try_from(x: DataRequestInfo) -> Result<Self, &'static str> {
        if let Some(tally) = x.tally {
            Ok(DataRequestInfoStorage {
                commits: x.commits.keys().cloned().collect(),
                reveals: x.reveals.keys().cloned().collect(),
                tally: tally.0,
            })
        } else {
            Err("Cannot persist unfinished data request (with no Tally)")
        }
    }
}

/// Data request current stage
pub enum DataRequestStage {
    /// Expecting commitments for data request
    _COMMIT,
    /// Expecting reveals to previously published commitments
    _REVEAL,
    /// Expecting tally to be included in block
    _TALLY,
}
