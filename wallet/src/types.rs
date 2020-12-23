use std::{
    convert::TryFrom,
    fmt,
    sync::{Arc, RwLock},
};

use core::fmt::Display;
pub use std::str::FromStr;

use crate::app::VttOutputParams;
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

use witnet_data_structures::{
    transaction::{
        CommitTransaction, DRTransaction, DRTransactionBody, MintTransaction, RevealTransaction,
        TallyTransaction, Transaction, VTTransaction, VTTransactionBody,
    },
    transaction_factory::FeeType,
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

pub struct VttParams {
    pub fee: u64,
    pub outputs: Vec<ValueTransferOutput>,
    pub fee_type: FeeType,
}

pub struct DataReqParams {
    pub fee: u64,
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

/// Transaction data structure
#[derive(Debug, Eq, PartialEq, Clone, Serialize, Deserialize)]
// FIXME(#649): Remove clippy skip error
#[allow(clippy::large_enum_variant)]
pub enum TransactionHelper {
    #[serde(
        serialize_with = "into_generic_type::<_, VTTransactionHelper, _>",
        deserialize_with = "from_generic_type::<_, VTTransactionHelper, _>"
    )]
    ValueTransfer(VTTransaction),
    #[serde(
        serialize_with = "into_generic_type::<_, DRTransactionHelper, _>",
        deserialize_with = "from_generic_type::<_, DRTransactionHelper, _>"
    )]
    DataRequest(DRTransaction),
    Commit(CommitTransaction),
    Reveal(RevealTransaction),
    Tally(TallyTransaction),
    Mint(MintTransaction),
}

impl From<Transaction> for TransactionHelper {
    fn from(x: Transaction) -> Self {
        match x {
            Transaction::ValueTransfer(vttransaction) => {
                TransactionHelper::ValueTransfer(vttransaction)
            }
            Transaction::DataRequest(drtransaction) => {
                TransactionHelper::DataRequest(drtransaction)
            }
            Transaction::Commit(committransaction) => TransactionHelper::Commit(committransaction),
            Transaction::Reveal(revealtransaction) => TransactionHelper::Reveal(revealtransaction),
            Transaction::Tally(tallytransaction) => TransactionHelper::Tally(tallytransaction),
            Transaction::Mint(minttransaction) => TransactionHelper::Mint(minttransaction),
        }
    }
}

impl From<TransactionHelper> for Transaction {
    fn from(x: TransactionHelper) -> Self {
        match x {
            TransactionHelper::ValueTransfer(vttransaction) => {
                Transaction::ValueTransfer(vttransaction)
            }
            TransactionHelper::DataRequest(drtransaction) => {
                Transaction::DataRequest(drtransaction)
            }
            TransactionHelper::Commit(committransaction) => Transaction::Commit(committransaction),
            TransactionHelper::Reveal(revealtransaction) => Transaction::Reveal(revealtransaction),
            TransactionHelper::Tally(tallytransaction) => Transaction::Tally(tallytransaction),
            TransactionHelper::Mint(minttransaction) => Transaction::Mint(minttransaction),
        }
    }
}

#[derive(Debug, Default, Eq, PartialEq, Clone, Serialize, Deserialize)]
pub struct DRTransactionHelper {
    #[serde(
        serialize_with = "into_generic_type::<_, DRTransactionBodyHelper, _>",
        deserialize_with = "from_generic_type::<_, DRTransactionBodyHelper, _>"
    )]
    pub body: DRTransactionBody,
    pub signatures: Vec<KeyedSignature>,
}

impl From<DRTransaction> for DRTransactionHelper {
    fn from(x: DRTransaction) -> Self {
        DRTransactionHelper {
            body: x.body,
            signatures: x.signatures,
        }
    }
}

impl From<DRTransactionHelper> for DRTransaction {
    fn from(x: DRTransactionHelper) -> Self {
        DRTransaction {
            body: x.body,
            signatures: x.signatures,
        }
    }
}

#[derive(Debug, Eq, PartialEq, Clone, Serialize, Deserialize)]
pub struct DRTransactionBodyHelper {
    pub inputs: Vec<TransactionInput>,
    #[serde(
        serialize_with = "into_generic_type_vec::<_, ValueTransferOutputHelper, _>",
        deserialize_with = "from_generic_type_vec::<_, ValueTransferOutputHelper, _>"
    )]
    pub outputs: Vec<ValueTransferOutput>,
    #[serde(
        serialize_with = "into_generic_type::<_, DataRequestOutputHelper, _>",
        deserialize_with = "from_generic_type::<_, DataRequestOutputHelper, _>"
    )]
    pub dr_output: DataRequestOutput,
}

impl From<DRTransactionBody> for DRTransactionBodyHelper {
    fn from(x: DRTransactionBody) -> Self {
        DRTransactionBodyHelper {
            inputs: x.inputs,
            outputs: x.outputs,
            dr_output: x.dr_output,
        }
    }
}

impl From<DRTransactionBodyHelper> for DRTransactionBody {
    fn from(x: DRTransactionBodyHelper) -> Self {
        DRTransactionBody::new(x.inputs, x.outputs, x.dr_output)
    }
}

#[derive(Debug, Eq, PartialEq, Clone, Serialize, Deserialize, Hash, Default)]
pub struct DataRequestOutputHelper {
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

#[derive(Debug, Eq, PartialEq, Clone, Serialize, Deserialize)]
pub struct VTTransactionHelper {
    #[serde(
        serialize_with = "into_generic_type::<_, VTTransactionBodyHelper, _>",
        deserialize_with = "from_generic_type::<_, VTTransactionBodyHelper, _>"
    )]
    pub body: VTTransactionBody,
    pub signatures: Vec<KeyedSignature>,
}

impl From<VTTransaction> for VTTransactionHelper {
    fn from(x: VTTransaction) -> Self {
        VTTransactionHelper {
            body: x.body,
            signatures: x.signatures,
        }
    }
}

impl From<VTTransactionHelper> for VTTransaction {
    fn from(x: VTTransactionHelper) -> Self {
        VTTransaction {
            body: x.body,
            signatures: x.signatures,
        }
    }
}

#[derive(Debug, Eq, PartialEq, Clone, Serialize, Deserialize)]
pub struct VTTransactionBodyHelper {
    pub inputs: Vec<TransactionInput>,
    #[serde(
        serialize_with = "into_generic_type_vec::<_, ValueTransferOutputHelper, _>",
        deserialize_with = "from_generic_type_vec::<_, ValueTransferOutputHelper, _>"
    )]
    pub outputs: Vec<ValueTransferOutput>,
}

impl From<VTTransactionBody> for VTTransactionBodyHelper {
    fn from(x: VTTransactionBody) -> Self {
        VTTransactionBodyHelper {
            inputs: x.inputs,
            outputs: x.outputs,
        }
    }
}

impl From<VTTransactionBodyHelper> for VTTransactionBody {
    fn from(x: VTTransactionBodyHelper) -> Self {
        VTTransactionBody::new(x.inputs, x.outputs)
    }
}

/// Value transfer output transaction data structure
#[derive(Debug, Eq, PartialEq, Clone, Serialize, Deserialize, Hash)]
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

/// Value transfer output transaction data structure
#[derive(Debug, Eq, PartialEq, Clone, Serialize, Deserialize, Hash, Default)]
pub struct VttOutputParamsHelper {
    pub address: PublicKeyHash,
    #[serde(
        serialize_with = "u64_to_string",
        deserialize_with = "number_from_string"
    )]
    pub amount: u64,
    pub time_lock: Option<u64>,
}

impl From<VttOutputParams> for VttOutputParamsHelper {
    fn from(x: VttOutputParams) -> Self {
        VttOutputParamsHelper {
            address: x.address.parse().unwrap(),
            amount: x.amount,
            time_lock: x.time_lock,
        }
    }
}

impl From<VttOutputParamsHelper> for VttOutputParams {
    fn from(x: VttOutputParamsHelper) -> Self {
        VttOutputParams {
            address: x.address.to_string(),
            amount: x.amount,
            time_lock: x.time_lock,
        }
    }
}

pub fn from_generic_type<'de, D, T, U>(deserializer: D) -> Result<U, D::Error>
where
    D: Deserializer<'de>,
    T: Deserialize<'de>,
    U: From<T>,
{
    Ok(T::deserialize(deserializer)?.into())
}

pub fn into_generic_type<S, U, T>(val: &T, serializer: S) -> Result<S::Ok, S::Error>
where
    S: serde::Serializer,
    T: Clone,
    U: From<T>,
    U: Serialize,
{
    let x = U::from(val.clone());
    x.serialize(serializer)
}

pub fn from_generic_type_vec<'de, D, T, U>(deserializer: D) -> Result<Vec<U>, D::Error>
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

pub fn into_generic_type_vec<S, U, T>(val: &[T], serializer: S) -> Result<S::Ok, S::Error>
where
    S: serde::Serializer,
    T: Clone,
    U: From<T>,
    U: Serialize,
{
    let x: Vec<U> = val.iter().map(|x| x.clone().into()).collect();
    x.serialize(serializer)
}

pub fn u16_to_string<S>(val: &u16, serializer: S) -> Result<S::Ok, S::Error>
where
    S: serde::Serializer,
{
    if serializer.is_human_readable() {
        serializer.serialize_str(&val.to_string())
    } else {
        serializer.serialize_u16(*val)
    }
}

pub fn u32_to_string<S>(val: &u32, serializer: S) -> Result<S::Ok, S::Error>
where
    S: serde::Serializer,
{
    if serializer.is_human_readable() {
        serializer.serialize_str(&val.to_string())
    } else {
        serializer.serialize_u32(*val)
    }
}

pub fn u64_to_string<S>(val: &u64, serializer: S) -> Result<S::Ok, S::Error>
where
    S: serde::Serializer,
{
    if serializer.is_human_readable() {
        serializer.serialize_str(&val.to_string())
    } else {
        serializer.serialize_u64(*val)
    }
}

pub fn number_from_string<'de, T, D>(deserializer: D) -> Result<T, D::Error>
where
    D: Deserializer<'de>,
    T: FromStr + serde::Deserialize<'de>,
    <T as FromStr>::Err: Display,
{
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum StringOrInt<T> {
        String(String),
        Number(T),
    }
    if deserializer.is_human_readable() {
        match StringOrInt::<T>::deserialize(deserializer)? {
            StringOrInt::String(s) => s.parse::<T>().map_err(serde::de::Error::custom),
            StringOrInt::Number(i) => Ok(i),
        }
    } else {
        T::deserialize(deserializer)
    }
}

#[cfg(test)]
mod tests {
    use crate::app::{
        CreateDataReqRequest, CreateDataReqResponse, CreateVttRequest, CreateVttResponse,
        SendTransactionRequest,
    };

    #[test]
    fn test_deserialize_create_vtt() {
        let _e1: CreateVttRequest = serde_json::from_str(r#"{"session_id":"f29f2c88b8239fef509ccaaa504f943c3badcaff71d78d908d34cd461ea09f15","wallet_id":"87575c9031c01cf84dffc33fe2d28474d620dacd673f06990dc0318079ddfde7","outputs":[{"address":"wit1hrgcchsxezpf4cqy73djdelp03c9duwd4gx6yx","amount":"1"}],"fee":"1","label":""}"#).unwrap();
    }

    #[test]
    fn test_deserialize_vtt_response() {
        let _e1: CreateVttResponse = serde_json::from_str(r#"{"bytes":"0ad9010a630a260a240a220a20aa27cffcdc685c99c030e67a4d65b49803870f4277a7620e83b2e55fc6ed5bc2121a0a160a14153f9050e14523266b9e500bb667e1ae4ad188421001121d0a160a141de42db8d29472378197eaf88b9dfeba4ec0bf5e10b18be80312720a4b0a490a47304502210081ffe5c085a8aa95b71b91ba987f97265054b58a742bcebcc1a3a2bba21f4e3202206d7d82b5519b82063b6aeb962ec2fde7fc602c870cbbf150750f652dddaf28fa12230a2102fe4a2f859572fed6076fceb66ea8f56daac889616c72e5c2cd1ade5a0784fb2f","metadata":{"fee":"853","outputs":[{"address":"wit1hrgcchsxezpf4cqy73djdelp03c9duwd4gx6yx","amount":"1","time_lock":null}]},"transaction":{"ValueTransfer":{"body":{"inputs":[{"output_pointer":"aa27cffcdc685c99c030e67a4d65b49803870f4277a7620e83b2e55fc6ed5bc2:0"}],"outputs":[{"pkh":"wit1hrgcchsxezpf4cqy73djdelp03c9duwd4gx6yx","time_lock":0,"value":"1"},{"pkh":"wit1dm0rm5hc2uqa5japlpc0n2adfu0tmyx95h3nec","time_lock":0,"value":"7996849"}]},"signatures":[{"public_key":{"bytes":[254,74,47,133,149,114,254,214,7,111,206,182,110,168,245,109,170,200,137,97,108,114,229,194,205,26,222,90,7,132,251,47],"compressed":2},"signature":{"Secp256k1":{"der":[48,69,2,33,0,129,255,229,192,133,168,170,149,183,27,145,186,152,127,151,38,80,84,181,138,116,43,206,188,193,163,162,187,162,31,78,50,2,32,109,125,130,181,81,155,130,6,59,106,235,150,46,194,253,231,252,96,44,135,12,187,241,80,117,15,101,45,221,175,40,250]}}}]}},"transaction_id":"dbb328838b6dc40ba6fabe007f97d6f948532e68590db0ff8ea6a75b11ae432d"}"#).unwrap();
    }

    #[test]
    fn test_deserialize_create_data_request() {
        let _e1: CreateDataReqRequest = serde_json::from_str(r#"{"session_id":"a273d76ab9870976ab82a5dddced02fcd3ab5ff4fac51a1cf9437c762f317101","wallet_id":"87575c9031c01cf84dffc33fe2d28474d620dacd673f06990dc0318079ddfde7","fee":"1","request":{"data_request":{"timeLock":0,"time_lock":0,"retrieve":[{"contentType":"JSON API","kind":"HTTP-GET","script":[128],"url":"https://blockchain.info/q/latesthash"},{"contentType":"JSON API","kind":"HTTP-GET","script":[130,24,119,130,24,103,100,104,97,115,104],"url":"https://api-r.bitcoinchain.com/v1/status"},{"contentType":"JSON API","kind":"HTTP-GET","script":[131,24,119,130,24,102,100,100,97,116,97,130,24,103,111,98,101,115,116,95,98,108,111,99,107,95,104,97,115,104],"url":"https://api.blockchair.com/bitcoin/stats"}],"aggregate":{"filters":[],"reducer":2},"tally":{"filters":[{"op":8,"args":[]}],"reducer":2}},"collateral":"1000000000","witness_reward":"1","witnesses":"3","commit_and_reveal_fee":"1","min_consensus_percentage":"51"}}"#).unwrap();
    }

    #[test]
    fn test_deserialize_dr_response() {
        let _e1: CreateDataReqResponse = serde_json::from_str(r#"{"bytes":"12df060a8f030a260a240a220a207db2cb25996c606f3a13e8f581b6112a09acc0d13dc1f444fa36cf645c798c340a260a240a220a20b864fb1c00a3a9217c9a90cf9e570a46544356e39b4abe2b73e929c23934d7230a260a240a220a202517e3982ee9a16db1c86277ec47d61173943a84933c6b9d1be47ce1dbddcbca0a260a240a220a200f56d5a2bdc1c17554f8475b1655aad32e6880a532171fa33b12422d84fb7397121d0a160a149b5d00edd5eb3110f9bca45e7f07f8e30644fb90109f8ee8031acd010abc011229122468747470733a2f2f626c6f636b636861696e2e696e666f2f712f6c6174657374686173681a01801237122868747470733a2f2f6170692d722e626974636f696e636861696e2e636f6d2f76312f7374617475731a0b8218778218676468617368124a122868747470733a2f2f6170692e626c6f636b63686169722e636f6d2f626974636f696e2f73746174731a1e83187782186664646174618218676f626573745f626c6f636b5f686173681a02100222060a02080810021001180320012833308094ebdc0312710a4a0a480a46304402207b0ca4534d14f60a70ce73fdcf43db55c749c1561e6be77ee284e90e2997fb690220799caeb94454cfe534ecd76a67a80f87d8675f6339dbced49b8d8131fb28de3212230a21029e695972bdea86e45c1beddd61101d645c90afb7a0fc2786b1e8f5bac877f88e12710a4a0a480a46304402203b87facb60f5be700d9d851f854cf556235a44a63dbdf81f390378613b8f94eb0220455c59089b732a5dda77011b53457a591cdd69cbcf8dda4f5f465d644c012daa12230a2102fe4a2f859572fed6076fceb66ea8f56daac889616c72e5c2cd1ade5a0784fb2f12720a4b0a490a473045022100c6d56d42b66a2a588abe8f5c794536984dcd2617b571069afa4fbcbec0a9586d02207ec0eb8c931fc556ac8ef2e038be3ce79c9ff3e3a04a96cf30dcf4c337b893be12230a2102f72d93e5dbe24fc5f0b563516ed64062ff7f883f2169c04b3aca3d13fee7538e12710a4a0a480a46304402203b87facb60f5be700d9d851f854cf556235a44a63dbdf81f390378613b8f94eb0220455c59089b732a5dda77011b53457a591cdd69cbcf8dda4f5f465d644c012daa12230a2102fe4a2f859572fed6076fceb66ea8f56daac889616c72e5c2cd1ade5a0784fb2f","fee":"2787","transaction":{"DataRequest":{"body":{"dr_output":{"collateral":"1000000000","commit_and_reveal_fee":"1","data_request":{"aggregate":{"filters":[],"reducer":2},"retrieve":[{"kind":"HTTP-GET","script":[128],"url":"https://blockchain.info/q/latesthash"},{"kind":"HTTP-GET","script":[130,24,119,130,24,103,100,104,97,115,104],"url":"https://api-r.bitcoinchain.com/v1/status"},{"kind":"HTTP-GET","script":[131,24,119,130,24,102,100,100,97,116,97,130,24,103,111,98,101,115,116,95,98,108,111,99,107,95,104,97,115,104],"url":"https://api.blockchair.com/bitcoin/stats"}],"tally":{"filters":[{"args":[],"op":8}],"reducer":2},"time_lock":0},"min_consensus_percentage":"51","witness_reward":"1","witnesses":"3"},"inputs":[{"output_pointer":"7db2cb25996c606f3a13e8f581b6112a09acc0d13dc1f444fa36cf645c798c34:0"},{"output_pointer":"b864fb1c00a3a9217c9a90cf9e570a46544356e39b4abe2b73e929c23934d723:0"},{"output_pointer":"2517e3982ee9a16db1c86277ec47d61173943a84933c6b9d1be47ce1dbddcbca:0"},{"output_pointer":"0f56d5a2bdc1c17554f8475b1655aad32e6880a532171fa33b12422d84fb7397:0"}],"outputs":[{"pkh":"wit1hrgcchsxezpf4cqy73djdelp03c9duwd4gx6yx","time_lock":0,"value":"7997215"}]},"signatures":[{"public_key":{"bytes":[158,105,89,114,189,234,134,228,92,27,237,221,97,16,29,100,92,144,175,183,160,252,39,134,177,232,245,186,200,119,248,142],"compressed":2},"signature":{"Secp256k1":{"der":[48,68,2,32,123,12,164,83,77,20,246,10,112,206,115,253,207,67,219,85,199,73,193,86,30,107,231,126,226,132,233,14,41,151,251,105,2,32,121,156,174,185,68,84,207,229,52,236,215,106,103,168,15,135,216,103,95,99,57,219,206,212,155,141,129,49,251,40,222,50]}}},{"public_key":{"bytes":[254,74,47,133,149,114,254,214,7,111,206,182,110,168,245,109,170,200,137,97,108,114,229,194,205,26,222,90,7,132,251,47],"compressed":2},"signature":{"Secp256k1":{"der":[48,68,2,32,59,135,250,203,96,245,190,112,13,157,133,31,133,76,245,86,35,90,68,166,61,189,248,31,57,3,120,97,59,143,148,235,2,32,69,92,89,8,155,115,42,93,218,119,1,27,83,69,122,89,28,221,105,203,207,141,218,79,95,70,93,100,76,1,45,170]}}},{"public_key":{"bytes":[247,45,147,229,219,226,79,197,240,181,99,81,110,214,64,98,255,127,136,63,33,105,192,75,58,202,61,19,254,231,83,142],"compressed":2},"signature":{"Secp256k1":{"der":[48,69,2,33,0,198,213,109,66,182,106,42,88,138,190,143,92,121,69,54,152,77,205,38,23,181,113,6,154,250,79,188,190,192,169,88,109,2,32,126,192,235,140,147,31,197,86,172,142,242,224,56,190,60,231,156,159,243,227,160,74,150,207,48,220,244,195,55,184,147,190]}}},{"public_key":{"bytes":[254,74,47,133,149,114,254,214,7,111,206,182,110,168,245,109,170,200,137,97,108,114,229,194,205,26,222,90,7,132,251,47],"compressed":2},"signature":{"Secp256k1":{"der":[48,68,2,32,59,135,250,203,96,245,190,112,13,157,133,31,133,76,245,86,35,90,68,166,61,189,248,31,57,3,120,97,59,143,148,235,2,32,69,92,89,8,155,115,42,93,218,119,1,27,83,69,122,89,28,221,105,203,207,141,218,79,95,70,93,100,76,1,45,170]}}}]}},"transaction_id":"9aad7a47c2df4ed774a8e220fa1cadb286000931dcbf3ce567fef5eafeebd945"}"#).unwrap();
    }

    #[test]
    fn test_deserialize_send_txn_vtt() {
        let _e1: SendTransactionRequest = serde_json::from_str(r#"{"wallet_id":"87575c9031c01cf84dffc33fe2d28474d620dacd673f06990dc0318079ddfde7","session_id":"079b703d4f8935789772651b79326150d1014c92a95e2d02266df1f575abb1fb","transaction":{"ValueTransfer":{"body":{"inputs":[{"output_pointer":"aa27cffcdc685c99c030e67a4d65b49803870f4277a7620e83b2e55fc6ed5bc2:0"}],"outputs":[{"pkh":"wit1dm0rm5hc2uqa5japlpc0n2adfu0tmyx95h3nec","time_lock":0,"value":"1"},{"pkh":"wit1hrgcchsxezpf4cqy73djdelp03c9duwd4gx6yx","time_lock":0,"value":"7996849"}]},"signatures":[{"public_key":{"bytes":[254,74,47,133,149,114,254,214,7,111,206,182,110,168,245,109,170,200,137,97,108,114,229,194,205,26,222,90,7,132,251,47],"compressed":2},"signature":{"Secp256k1":{"der":[48,69,2,33,0,129,255,229,192,133,168,170,149,183,27,145,186,152,127,151,38,80,84,181,138,116,43,206,188,193,163,162,187,162,31,78,50,2,32,109,125,130,181,81,155,130,6,59,106,235,150,46,194,253,231,252,96,44,135,12,187,241,80,117,15,101,45,221,175,40,250]}}}]}}}"#).unwrap();
    }

    #[test]
    fn test_deserialize_send_txn_dr() {
        let _e1: SendTransactionRequest = serde_json::from_str(r#"{"wallet_id":"87575c9031c01cf84dffc33fe2d28474d620dacd673f06990dc0318079ddfde7","session_id":"079b703d4f8935789772651b79326150d1014c92a95e2d02266df1f575abb1fb","transaction":{"DataRequest":{"body":{"dr_output":{"collateral":"1000000000","commit_and_reveal_fee":"1","data_request":{"aggregate":{"filters":[],"reducer":2},"retrieve":[{"kind":"HTTP-GET","script":[128],"url":"https://blockchain.info/q/latesthash"},{"kind":"HTTP-GET","script":[130,24,119,130,24,103,100,104,97,115,104],"url":"https://api-r.bitcoinchain.com/v1/status"},{"kind":"HTTP-GET","script":[131,24,119,130,24,102,100,100,97,116,97,130,24,103,111,98,101,115,116,95,98,108,111,99,107,95,104,97,115,104],"url":"https://api.blockchair.com/bitcoin/stats"}],"tally":{"filters":[{"args":[],"op":8}],"reducer":2},"time_lock":0},"min_consensus_percentage":"51","witness_reward":"1","witnesses":"3"},"inputs":[{"output_pointer":"7db2cb25996c606f3a13e8f581b6112a09acc0d13dc1f444fa36cf645c798c34:0"},{"output_pointer":"b864fb1c00a3a9217c9a90cf9e570a46544356e39b4abe2b73e929c23934d723:0"},{"output_pointer":"2517e3982ee9a16db1c86277ec47d61173943a84933c6b9d1be47ce1dbddcbca:0"},{"output_pointer":"0f56d5a2bdc1c17554f8475b1655aad32e6880a532171fa33b12422d84fb7397:0"}],"outputs":[{"pkh":"wit1dm0rm5hc2uqa5japlpc0n2adfu0tmyx95h3nec","time_lock":0,"value":"7997215"}]},"signatures":[{"public_key":{"bytes":[158,105,89,114,189,234,134,228,92,27,237,221,97,16,29,100,92,144,175,183,160,252,39,134,177,232,245,186,200,119,248,142],"compressed":2},"signature":{"Secp256k1":{"der":[48,68,2,32,123,12,164,83,77,20,246,10,112,206,115,253,207,67,219,85,199,73,193,86,30,107,231,126,226,132,233,14,41,151,251,105,2,32,121,156,174,185,68,84,207,229,52,236,215,106,103,168,15,135,216,103,95,99,57,219,206,212,155,141,129,49,251,40,222,50]}}},{"public_key":{"bytes":[254,74,47,133,149,114,254,214,7,111,206,182,110,168,245,109,170,200,137,97,108,114,229,194,205,26,222,90,7,132,251,47],"compressed":2},"signature":{"Secp256k1":{"der":[48,68,2,32,59,135,250,203,96,245,190,112,13,157,133,31,133,76,245,86,35,90,68,166,61,189,248,31,57,3,120,97,59,143,148,235,2,32,69,92,89,8,155,115,42,93,218,119,1,27,83,69,122,89,28,221,105,203,207,141,218,79,95,70,93,100,76,1,45,170]}}},{"public_key":{"bytes":[247,45,147,229,219,226,79,197,240,181,99,81,110,214,64,98,255,127,136,63,33,105,192,75,58,202,61,19,254,231,83,142],"compressed":2},"signature":{"Secp256k1":{"der":[48,69,2,33,0,198,213,109,66,182,106,42,88,138,190,143,92,121,69,54,152,77,205,38,23,181,113,6,154,250,79,188,190,192,169,88,109,2,32,126,192,235,140,147,31,197,86,172,142,242,224,56,190,60,231,156,159,243,227,160,74,150,207,48,220,244,195,55,184,147,190]}}},{"public_key":{"bytes":[254,74,47,133,149,114,254,214,7,111,206,182,110,168,245,109,170,200,137,97,108,114,229,194,205,26,222,90,7,132,251,47],"compressed":2},"signature":{"Secp256k1":{"der":[48,68,2,32,59,135,250,203,96,245,190,112,13,157,133,31,133,76,245,86,35,90,68,166,61,189,248,31,57,3,120,97,59,143,148,235,2,32,69,92,89,8,155,115,42,93,218,119,1,27,83,69,122,89,28,221,105,203,207,141,218,79,95,70,93,100,76,1,45,170]}}}]}}}"#).unwrap();
    }
}
