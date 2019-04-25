//! Wallet type definitions.
use jsonrpc_core::Value;
use serde::{Deserialize, Serialize};

/// TODO: doc
#[derive(Debug, Deserialize)]
pub struct LockWalletParams {
    wallet_id: String,
    #[serde(default)]
    wipe: bool,
}

/// TODO: Implement (radon crate)
#[derive(Serialize)]
pub struct RadonValue {}

/// TODO: doc
#[derive(Debug, Deserialize)]
pub struct CreateDataRequestParams {
    not_before: u64,
    retrieve: Vec<RADRetrieveArgs>,
    aggregate: RADAggregateArgs,
    consensus: RADConsensusArgs,
    deliver: Vec<RADDeliverArgs>,
}

/// TODO: Implement (radon crate)
#[derive(Debug, Deserialize, Serialize)]
pub struct RADType(String);

/// TODO: doc
#[derive(Debug, Deserialize, Serialize)]
pub struct RADRetrieveArgs {
    kind: RADType,
    url: String,
    script: Vec<Value>,
}

/// TODO: doc
#[derive(Debug, Deserialize, Serialize)]
pub struct RADAggregateArgs {
    script: Vec<Value>,
}

/// TODO: doc
#[derive(Debug, Deserialize, Serialize)]
pub struct RADConsensusArgs {
    script: Vec<Value>,
}

/// TODO: doc
#[derive(Debug, Deserialize, Serialize)]
pub struct RADDeliverArgs {
    kind: RADType,
    url: String,
}

/// TODO: Implement (data_structures crate)
#[derive(Debug, Deserialize, Serialize)]
pub struct DataRequest {}

/// TODO: Question. Is this pkh?
#[derive(Serialize)]
pub struct Address {}

/// TODO: doc
#[derive(Debug, Deserialize)]
pub struct GenerateAddressParams {
    wallet_id: String,
}

/// TODO: doc
#[derive(Debug, Deserialize)]
pub struct SendVttParams {
    wallet_id: String,
    to_address: Vec<u8>,
    amount: u64,
    fee: u64,
    subject: String,
}

/// TODO: doc
#[derive(Debug, Deserialize)]
pub struct GetTransactionsParams {
    wallet_id: String,
    limit: u32,
    page: u32,
}

/// TODO: Implement (import data_structures crate)
#[derive(Serialize)]
pub struct Transaction {}

/// TODO: doc
#[derive(Debug, Deserialize, Serialize)]
pub struct UnlockWalletParams {
    id: String,
    password: String,
}

/// TODO: doc
#[derive(Debug, Deserialize, Serialize)]
pub struct Wallet {
    pub(crate) version: u32,
    pub(crate) info: WalletInfo,
    pub(crate) seed: SeedInfo,
    pub(crate) epochs: EpochsInfo,
    pub(crate) purpose: DerivationPath,
    pub(crate) accounts: Vec<Account>,
}

/// TODO: doc
#[derive(Debug, Deserialize, Serialize)]
pub enum SeedInfo {
    Wip3(Seed),
}

/// TODO: doc
#[derive(Debug, Deserialize, Serialize)]
pub struct Seed(pub(crate) Vec<u8>);

/// TODO: doc
#[derive(Debug, Deserialize, Serialize)]
pub struct EpochsInfo {
    pub(crate) last: u32,
    pub(crate) born: u32,
}

/// TODO: doc
#[derive(Debug, Deserialize, Serialize)]
pub struct DerivationPath(pub(crate) String);

/// TODO: doc
#[derive(Debug, Deserialize, Serialize)]
pub struct Account {
    key_path: KeyPath,
    key_chains: Vec<KeyChain>,
    balance: u64,
}

/// TODO: doc
#[derive(Debug, Deserialize, Serialize)]
pub struct KeyPath(Vec<ChildNumber>);

/// TODO: doc
#[derive(Debug, Deserialize, Serialize)]
pub struct ChildNumber(u32);

/// TODO: doc
#[derive(Debug, Deserialize, Serialize)]
pub enum KeyChain {
    External,
    Internal,
    Rad,
}

/// TODO: doc
#[derive(Debug, Deserialize)]
pub struct CreateWalletParams {
    name: String,
    password: String,
}

/// TODO: doc
#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum ImportSeedParams {
    Mnemonics { mnemonics: Mnemonics },
    Seed { seed: String },
}

/// TODO: implemented in PR #432
#[derive(Debug, Deserialize, Serialize)]
pub struct Mnemonics {}

/// TODO: doc
#[derive(Debug, Deserialize, Serialize)]
pub struct WalletInfo {
    pub(crate) id: String,
    pub(crate) caption: String,
}
