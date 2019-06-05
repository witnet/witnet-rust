use serde::Deserialize;

use crate::wallet;

#[derive(Debug, Deserialize)]
pub struct CreateDataReqRequest {
    pub not_before: u64,
    pub retrieve: Vec<wallet::RADRetrieveArgs>,
    pub aggregate: wallet::RADAggregateArgs,
    pub consensus: wallet::RADConsensusArgs,
    pub deliver: Vec<wallet::RADDeliverArgs>,
}
