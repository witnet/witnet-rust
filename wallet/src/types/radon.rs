//! RADON-related types.
use jsonrpc_core as rpc;
use serde::{Deserialize, Serialize};

pub use witnet_rad::types::RadonTypes;

/// TODO: Implement (radon crate)
#[derive(Debug, Deserialize, Serialize)]
pub struct RADType(String);

#[derive(Debug, Deserialize, Serialize)]
pub struct RADRetrieveArgs {
    kind: RADType,
    url: String,
    script: Vec<rpc::Value>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct RADAggregateArgs {
    script: Vec<rpc::Value>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct RADConsensusArgs {
    script: Vec<rpc::Value>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct RADDeliverArgs {
    kind: RADType,
    url: String,
}
