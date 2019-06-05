//! # Wallet-specific data types
use jsonrpc_core::Value;
use serde::{Deserialize, Serialize};

pub use witnet_data_structures::chain::RADRequest;

#[derive(Debug, Deserialize, Serialize)]
pub struct WalletInfo {
    pub(crate) id: String,
    pub(crate) caption: String,
}

/// TODO: implemented in PR #432
#[derive(Debug, Deserialize, Serialize)]
pub struct Mnemonics {}

/// TODO: Implement (radon crate)
#[derive(Debug, Deserialize, Serialize)]
pub struct RADType(String);

#[derive(Debug, Deserialize, Serialize)]
pub struct RADRetrieveArgs {
    kind: RADType,
    url: String,
    script: Vec<Value>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct RADAggregateArgs {
    script: Vec<Value>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct RADConsensusArgs {
    script: Vec<Value>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct RADDeliverArgs {
    kind: RADType,
    url: String,
}
