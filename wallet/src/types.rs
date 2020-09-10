use std::convert::TryFrom;
use std::fmt;
use std::sync::{Arc, RwLock};

use failure::Fail;
pub use jsonrpc_core::{Params as RpcParams, Value as RpcValue};
pub use jsonrpc_pubsub::{Sink, SinkResult, Subscriber, SubscriptionId};
use serde::{Deserialize, Serialize};
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
        RADTally, SuperBlock, ValueTransferOutput as VttOutput,
    },
    error::EpochCalculationError,
    proto::ProtobufConvert,
    radon_error::{RadonError, RadonErrors},
    radon_report::RadonReport,
    transaction::{
        DRTransaction, DRTransactionBody, TallyTransaction, Transaction, VTTransaction,
        VTTransactionBody,
    },
};
pub use witnet_net::client::tcp::jsonrpc::Request as RpcRequest;
use witnet_protected::{Protected, ProtectedString};
pub use witnet_rad::{error::RadError, types::RadonTypes, RADRequestExecutionReport};

use crate::model;
use crate::types::signature::Signature;

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
    pub caption: Option<String>,
    pub balance: u64,
    pub current_account: u32,
    pub available_accounts: Vec<u32>,
    pub last_sync: CheckpointBeacon,
    pub last_confirmed: CheckpointBeacon,
}

pub struct CreateWalletData<'a> {
    pub id: &'a str,
    pub name: Option<String>,
    pub caption: Option<String>,
    pub iv: Vec<u8>,
    pub salt: Vec<u8>,
    pub account: &'a Account,
}

pub struct VttParams {
    pub pkh: PublicKeyHash,
    pub value: u64,
    pub fee: u64,
    pub time_lock: u64,
}

pub struct DataReqParams {
    pub fee: u64,
    pub request: DataRequestOutput,
}

pub struct Balance {
    pub account: u32,
    pub amount: u64,
}

#[derive(Debug)]
pub struct TransactionComponents {
    pub value: u64,
    pub change: u64,
    pub balance: u64,
    pub inputs: Vec<TransactionInput>,
    pub outputs: Vec<VttOutput>,
    pub sign_keys: Vec<SK>,
    pub used_utxos: Vec<model::OutPtr>,
}

/// Builds a `ValueTransferTransaction` from a list of `ValueTransferOutput`s
#[derive(Clone, Debug, Default, Hash, Eq, PartialEq, Serialize, Deserialize)]
pub struct BuildVtt {
    /// List of `ValueTransferOutput`s
    pub vto: Vec<VttOutput>,
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
#[derive(Debug, Serialize)]
pub enum Event {
    /// The basic information of a new block that has already been processed
    Block(model::Beacon),
    /// A new movement (transaction) affecting balance.
    Movement(model::BalanceMovement),
    /// The end of a synchronization progress.
    SyncFinish(u32, u32),
    /// An update on the progress of a the synchronization progress.
    SyncProgress(u32, u32, u32),
    /// The start of a synchronization progress.
    SyncStart(u32, u32),
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
