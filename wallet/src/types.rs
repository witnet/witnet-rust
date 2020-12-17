use std::convert::TryFrom;
use std::fmt;
use std::sync::{Arc, RwLock};

pub use std::str::FromStr;

use crate::model::{number_from_string, u16_to_string, u32_to_string, u64_to_string};

use failure::Fail;
pub use jsonrpc_core::{Params as RpcParams, Value as RpcValue};
pub use jsonrpc_pubsub::{Sink, SinkResult, Subscriber, SubscriptionId};
use serde::{Deserialize, Deserializer, Serialize};
pub use serde_json::Value as Json;

pub use witnet_crypto::{
    hash::HashFunction,
    key::{CryptoEngine, ExtendedPK, ExtendedSK, KeyDerivationError, KeyPath, ONE_KEY, PK, SK},
    mnemonic::{Length as MnemonicLength, Mnemonic, MnemonicGen},
    signature,
};
pub use witnet_data_structures::{
    chain::{
        Block as ChainBlock, CheckpointBeacon, DataRequestInfo, DataRequestOutput, Epoch, Hash,
        HashParseError, Hashable, Input as TransactionInput, KeyedSignature, OutputPointer,
        PublicKey, PublicKeyHash, PublicKeyHashParseError, RADAggregate, RADRequest, RADRetrieve,
        RADTally, StateMachine, SuperBlock, SyncStatus, ValueTransferOutput,
    },
    error::EpochCalculationError,
    proto::ProtobufConvert,
    radon_error::{RadonError, RadonErrors},
    radon_report::RadonReport,
    transaction::{
        DRTransaction, DRTransactionBody, TallyTransaction, Transaction, VTTransaction,
        VTTransactionBody, ALPHA, BETA, COMMIT_WEIGHT, GAMMA, INPUT_SIZE, OUTPUT_SIZE,
        REVEAL_WEIGHT, TALLY_WEIGHT,
    },
    transaction_factory::FeeType,
};

pub use witnet_net::client::tcp::jsonrpc::Request as RpcRequest;
use witnet_protected::{Protected, ProtectedString};
pub use witnet_rad::{error::RadError, types::RadonTypes, RADRequestExecutionReport};

use crate::{model, types::signature::Signature};

use super::{db, repository};

pub type Password = ProtectedString;

pub type Secret = Protected;

pub type SessionWallet = Arc<repository::Wallet<db::EncryptedDb>>;

pub type Wallet = repository::Wallet<db::EncryptedDb>;

#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct SessionId(String);

#[derive(Debug, Fail)]
pub enum Errors {
    #[fail(
        display = "Tried to construct a `SessionId` from a `SubscriptionId` that is not a `String`"
    )]
    SubscriptionIdIsNotValidSessionId,
}

/// Convenient conversion from `SessionId` to `SubscriptionId`
impl From<&SessionId> for SubscriptionId {
    fn from(id: &SessionId) -> Self {
        SubscriptionId::String(String::from(id.clone()))
    }
}

/// Convenient conversion from `SubscriptionId` to `SessionId`
impl TryFrom<&SubscriptionId> for SessionId {
    type Error = crate::actors::app::Error;

    fn try_from(id: &SubscriptionId) -> Result<Self, Self::Error> {
        match id {
            SubscriptionId::String(string) => Ok(SessionId::from(string.clone())),
            _ => Err(crate::actors::app::error::internal_error(
                Errors::SubscriptionIdIsNotValidSessionId,
            )),
        }
    }
}

impl fmt::Display for SessionId {
    #[inline]
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}...", &self.0[..5])
    }
}

impl fmt::Debug for SessionId {
    #[inline]
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:?}...", &self.0[..5])
    }
}

impl From<SessionId> for String {
    fn from(id: SessionId) -> Self {
        id.0
    }
}

impl From<String> for SessionId {
    fn from(id: String) -> Self {
        SessionId(id)
    }
}

pub enum SeedSource {
    Mnemonics(Mnemonic),
    Xprv(ProtectedString),
    XprvDouble((ProtectedString, ProtectedString)),
}

pub struct UnlockedSessionWallet {
    pub wallet: SessionWallet,
    pub data: WalletData,
    pub session_id: SessionId,
}

pub struct UnlockedWallet {
    pub data: WalletData,
    pub session_id: SessionId,
}

pub struct Account {
    pub index: u32,
    pub external: ExtendedSK,
    pub internal: ExtendedSK,
}

pub struct WalletData {
    pub id: String,
    pub name: Option<String>,
    pub description: Option<String>,
    pub balance: model::WalletBalance,
    pub current_account: u32,
    pub available_accounts: Vec<u32>,
    pub last_sync: CheckpointBeacon,
    pub last_confirmed: CheckpointBeacon,
}

pub struct CreateWalletData<'a> {
    pub id: &'a str,
    pub name: Option<String>,
    pub description: Option<String>,
    pub iv: Vec<u8>,
    pub salt: Vec<u8>,
    pub account: &'a Account,
    pub master_key: Option<ExtendedSK>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct VttParams {
    #[serde(
        serialize_with = "u64_to_string",
        deserialize_with = "number_from_string"
    )]
    pub fee: u64,
    #[serde(
        serialize_with = "into_generic_type_vec::<_, ValueTransferOutputHelper, _>",
        deserialize_with = "from_generic_type_vec::<_, ValueTransferOutputHelper, _>"
    )]
    pub outputs: Vec<ValueTransferOutput>,
    pub fee_type: FeeType,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DataReqParams {
    #[serde(
        serialize_with = "u64_to_string",
        deserialize_with = "number_from_string"
    )]
    pub fee: u64,
    #[serde(
        serialize_with = "into_generic_type::<_, DataRequestOutputHelper, _>",
        deserialize_with = "from_generic_type::<_, DataRequestOutputHelper, _>"
    )]
    pub request: DataRequestOutput,
    pub fee_type: FeeType,
}

#[derive(Debug, PartialEq)]
pub struct TransactionComponents {
    pub value: u64,
    pub change: u64,
    pub balance: model::BalanceInfo,
    pub inputs: Vec<TransactionInput>,
    pub outputs: Vec<ValueTransferOutput>,
    pub sign_keys: Vec<SK>,
    pub used_utxos: Vec<model::OutPtr>,
}

/// Builds a `ValueTransferTransaction` from a list of `ValueTransferOutput`s
#[derive(Clone, Debug, Default, Hash, Eq, PartialEq, Serialize, Deserialize)]
pub struct BuildVtt {
    /// List of `ValueTransferOutput`s
    pub vto: Vec<ValueTransferOutput>,
    /// Fee
    pub fee: u64,
}

/// Params of getBlockChain method
#[derive(Debug, Default, Deserialize, Serialize)]
pub struct GetBlockChainParams {
    /// First epoch for which to return block hashes.
    /// If negative, return block hashes from the last n epochs.
    #[serde(default)] // default to 0
    pub epoch: i64,
    /// Number of block hashes to return.
    /// If negative, return the last n block hashes from this epoch range.
    /// If zero, unlimited.
    #[serde(default)] // default to 0
    pub limit: i64,
}

#[derive(Debug, Serialize)]
pub struct ExtendedKeyedSignature {
    pub chaincode: Protected,
    pub public_key: PublicKey,
    pub signature: Signature,
}

#[derive(Clone, Debug, Deserialize)]
pub struct ChainEntry(pub u32, pub String);

impl TryFrom<&ChainEntry> for CheckpointBeacon {
    type Error = hex::FromHexError;

    fn try_from(entry: &ChainEntry) -> Result<Self, Self::Error> {
        let bytes = hex::decode(entry.1.clone())?;
        let hash = Hash::from(bytes);

        Ok(CheckpointBeacon {
            checkpoint: entry.0,
            hash_prev_block: hash,
        })
    }
}

/// A reference-counted atomic read/write lock over the `Option` of a `Sink`.
/// Allows swapping, adding and removing sinks in runtime through interior mutability of any
/// structures that may include this type.
pub type DynamicSink = Arc<RwLock<Option<Sink>>>;

/// Friendly events that can be sent to subscribed clients to let them now about significant
/// activity related to their wallets.
#[derive(Debug, Serialize, Clone)]
pub enum Event {
    /// The basic information of a new block that has already been processed but is pending
    /// consolidation (anchoring into a future superblock).
    Block(model::Beacon),
    /// A list of hashes of blocks that are now considered final.
    BlocksConsolidate(Vec<String>),
    /// A list of hashes of blocks that are now considered orphaned.
    BlocksOrphan(Vec<String>),
    /// A new movement (transaction) affecting balance.
    Movement(model::BalanceMovement),
    /// Node status has changed
    NodeStatus(StateMachine),
    /// Node disconnected
    NodeDisconnected,
    /// The end of a synchronization progress.
    SyncFinish(u32, u32),
    /// An update on the progress of a the synchronization progress.
    SyncProgress(u32, u32, u32),
    /// The start of a synchronization progress.
    SyncStart(u32, u32),
    /// An error occurred during the synchronization.
    SyncError(u32, u32),
}

/// Format of the output of getTransaction
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetTransactionResponse {
    /// Transaction
    pub transaction: Transaction,
    /// Hash of the block that contains this transaction in hex format,
    /// or "pending" if the transaction has not been included in any block yet
    pub block_hash: String,
}

/// Notification signaling that a superblock has been consolidated.
///
/// As per current consensus algorithm, "consolidated blocks" implies that there exists at least one
/// superblock in the chain that builds upon the superblock where those blocks were anchored.
#[derive(Clone, Deserialize)]
pub struct SuperBlockNotification {
    /// The superblock that we are signaling as consolidated.
    pub superblock: SuperBlock,
    /// The hashes of the blocks that we are signaling as consolidated.
    pub consolidated_block_hashes: Vec<String>,
}

// Serialization helper

/// Value transfer output transaction data structure
#[derive(Debug, Eq, PartialEq, Clone, Serialize, Deserialize, Hash, Default)]
pub struct ValueTransferOutputHelper {
    pub pkh: PublicKeyHash,
    #[serde(
        serialize_with = "u64_to_string",
        deserialize_with = "number_from_string"
    )]
    pub value: u64,
    /// The value attached to a time-locked output cannot be spent before the specified
    /// timestamp. That is, they cannot be used as an input in any transaction of a
    /// subsequent block proposed for an epoch whose opening timestamp predates the time lock.
    pub time_lock: u64,
}

impl From<ValueTransferOutput> for ValueTransferOutputHelper {
    fn from(x: ValueTransferOutput) -> Self {
        ValueTransferOutputHelper {
            pkh: x.pkh,
            value: x.value,
            time_lock: x.time_lock,
        }
    }
}

impl From<ValueTransferOutputHelper> for ValueTransferOutput {
    fn from(x: ValueTransferOutputHelper) -> Self {
        ValueTransferOutput {
            pkh: x.pkh,
            value: x.value,
            time_lock: x.time_lock,
        }
    }
}

fn from_generic_type_vec<'de, D, T, U>(deserializer: D) -> Result<Vec<U>, D::Error>
where
    D: Deserializer<'de>,
    T: Deserialize<'de>,
    U: From<T>,
{
    Ok(Vec::<T>::deserialize(deserializer)?
        .into_iter()
        .map(|x| x.into())
        .collect())
}

fn into_generic_type_vec<S, U, T>(val: &[T], serializer: S) -> Result<S::Ok, S::Error>
where
    S: serde::Serializer,
    T: Clone,
    U: From<T>,
    U: Serialize,
{
    let x: Vec<U> = val.iter().map(|x| x.clone().into()).collect();
    x.serialize(serializer)
}

#[derive(Debug, Eq, PartialEq, Clone, Serialize, Deserialize, Hash, Default)]
struct DataRequestOutputHelper {
    pub data_request: RADRequest,
    #[serde(
        serialize_with = "u64_to_string",
        deserialize_with = "number_from_string"
    )]
    pub witness_reward: u64,
    #[serde(
        serialize_with = "u16_to_string",
        deserialize_with = "number_from_string"
    )]
    pub witnesses: u16,
    // This fee will be earn by the miner when include commits and/or reveals in the block
    #[serde(
        serialize_with = "u64_to_string",
        deserialize_with = "number_from_string"
    )]
    pub commit_and_reveal_fee: u64,
    // This field must be >50 and <100.
    // >50 because simple majority
    // <100 because a 100% consensus encourages to commit a RadError for free
    #[serde(
        serialize_with = "u32_to_string",
        deserialize_with = "number_from_string"
    )]
    pub min_consensus_percentage: u32,
    // This field must be >= collateral_minimum, or zero
    // If zero, it will be treated as collateral_minimum
    #[serde(
        serialize_with = "u64_to_string",
        deserialize_with = "number_from_string"
    )]
    pub collateral: u64,
}

impl From<DataRequestOutput> for DataRequestOutputHelper {
    fn from(x: DataRequestOutput) -> Self {
        DataRequestOutputHelper {
            data_request: x.data_request,
            witness_reward: x.witness_reward,
            witnesses: x.witnesses,
            commit_and_reveal_fee: x.commit_and_reveal_fee,
            min_consensus_percentage: x.min_consensus_percentage,
            collateral: x.collateral,
        }
    }
}

impl From<DataRequestOutputHelper> for DataRequestOutput {
    fn from(x: DataRequestOutputHelper) -> Self {
        DataRequestOutput {
            data_request: x.data_request,
            witness_reward: x.witness_reward,
            witnesses: x.witnesses,
            commit_and_reveal_fee: x.commit_and_reveal_fee,
            min_consensus_percentage: x.min_consensus_percentage,
            collateral: x.collateral,
        }
    }
}

fn from_generic_type<'de, D, T, U>(deserializer: D) -> Result<U, D::Error>
where
    D: Deserializer<'de>,
    T: Deserialize<'de>,
    U: From<T>,
{
    Ok(T::deserialize(deserializer)?.into())
}

fn into_generic_type<S, U, T>(val: &T, serializer: S) -> Result<S::Ok, S::Error>
where
    S: serde::Serializer,
    T: Clone,
    U: From<T>,
    U: Serialize,
{
    let x = U::from(val.clone());
    x.serialize(serializer)
}

#[cfg(test)]
mod tests {
    use crate::app::CreateDataReqRequest;
    use crate::app::CreateVttRequest;

    #[test]
    fn test_desarialize_vtt() {
        let _e1: CreateVttRequest = serde_json::from_str(r#"{"session_id":"f29f2c88b8239fef509ccaaa504f943c3badcaff71d78d908d34cd461ea09f15","wallet_id":"87575c9031c01cf84dffc33fe2d28474d620dacd673f06990dc0318079ddfde7","outputs":[{"address":"twit1t4asz498h980446cfps0cpqatgvvkxv5zgll9e","amount":"1"}],"fee":"1","label":""}"#).unwrap();
    }

    #[test]
    fn test_desarialize_create_dr() {
        let _e1: CreateDataReqRequest = serde_json::from_str(r#"{"session_id":"a273d76ab9870976ab82a5dddced02fcd3ab5ff4fac51a1cf9437c762f317101","wallet_id":"87575c9031c01cf84dffc33fe2d28474d620dacd673f06990dc0318079ddfde7","fee":"1","request":{"data_request":{"timeLock":0,"time_lock":0,"retrieve":[{"contentType":"JSON API","kind":"HTTP-GET","script":[128],"url":"https://blockchain.info/q/latesthash"},{"contentType":"JSON API","kind":"HTTP-GET","script":[130,24,119,130,24,103,100,104,97,115,104],"url":"https://api-r.bitcoinchain.com/v1/status"},{"contentType":"JSON API","kind":"HTTP-GET","script":[131,24,119,130,24,102,100,100,97,116,97,130,24,103,111,98,101,115,116,95,98,108,111,99,107,95,104,97,115,104],"url":"https://api.blockchair.com/bitcoin/stats"}],"aggregate":{"filters":[],"reducer":2},"tally":{"filters":[{"op":8,"args":[]}],"reducer":2}},"collateral":"1000000000","witness_reward":"1","witnesses":"3","commit_and_reveal_fee":"1","min_consensus_percentage":"51"}}"#).unwrap();
    }
}
