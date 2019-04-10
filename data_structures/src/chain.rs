use failure::Fail;
use partial_struct::PartialStruct;
use protobuf::Message;
use secp256k1::{
    PublicKey as Secp256k1_PublicKey, SecretKey as Secp256k1_SecretKey,
    Signature as Secp256k1_Signature,
};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::{
    cell::Cell,
    collections::{BTreeMap, BTreeSet, HashMap, HashSet},
    convert::AsRef,
    fmt,
    str::FromStr,
};
use witnet_crypto::{
    hash::{calculate_sha256, Sha256},
    key::ExtendedSK,
};
use witnet_util::parser::parse_hex;

use super::{
    data_request::DataRequestPool,
    error::OutputPointerParseError,
    proto::{schema::witnet, ProtobufConvert},
    serializers::decoders::{TryFrom, TryInto},
};

pub trait Hashable {
    fn hash(&self) -> Hash;
}

/// Data structure holding critical information about the chain state and protocol constants
#[derive(Debug, Serialize, Deserialize, PartialEq, Clone)]
pub struct ChainInfo {
    /// Blockchain valid environment
    pub environment: Environment,

    /// Blockchain Protocol constants
    pub consensus_constants: ConsensusConstants,

    /// Checkpoint of the last block in the blockchain
    pub highest_block_checkpoint: CheckpointBeacon,
}

/// Possible values for the "environment" configuration param.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub enum Environment {
    /// "mainnet" environment
    #[serde(rename = "mainnet")]
    Mainnet,
    /// "testnet" environment
    #[serde(rename = "testnet-1")]
    Testnet1,
}

impl Default for Environment {
    fn default() -> Environment {
        Environment::Testnet1
    }
}

/// Consensus-critical configuration
#[derive(PartialStruct, Debug, Clone, PartialEq, Serialize, Deserialize, ProtobufConvert)]
#[partial_struct(derive(Deserialize, Default, Debug, Clone, PartialEq))]
#[protobuf_convert(pb = "witnet::ConsensusConstants")]
pub struct ConsensusConstants {
    /// Timestamp at checkpoint 0 (the start of epoch 0)
    pub checkpoint_zero_timestamp: i64,

    /// Seconds between the start of an epoch and the start of the next one
    pub checkpoints_period: u16,

    /// Genesis block hash value
    // TODO Change to a specific fixed-length hash function's output's digest type once Issue #164
    // is solved
    pub genesis_hash: Hash,

    /// Decay value for reputation demurrage function
    // TODO Use fixed point arithmetic (see Issue #172)
    pub reputation_demurrage: f64,

    /// Punishment value for claims out of the consensus bounds
    // TODO Use fixed point arithmetic (see Issue #172)
    pub reputation_punishment: f64,

    /// Maximum weight a block can have, this affects the number of
    /// transactions a block can contain: there will be as many
    /// transactions as the sum of _their_ weights is less than, or
    /// equal to, this maximum block weight parameter.
    ///
    /// Currently, a weight of 1 is equivalent to 1 byte.
    /// This is only configurable in testnet, in mainnet the default
    /// will be used.
    pub max_block_weight: u32,
}

/// Checkpoint beacon structure
#[derive(
    Copy, Clone, Debug, Default, Eq, Hash, PartialEq, Serialize, Deserialize, ProtobufConvert,
)]
#[protobuf_convert(pb = "witnet::CheckpointBeacon")]
pub struct CheckpointBeacon {
    /// The serial number for an epoch
    pub checkpoint: Epoch,
    /// The 256-bit hash of the previous block header
    pub hash_prev_block: Hash,
}

/// Epoch id (starting from 0)
pub type Epoch = u32;

/// Block data structure
#[derive(Debug, Eq, PartialEq, Clone, Serialize, Deserialize, ProtobufConvert)]
#[protobuf_convert(pb = "witnet::Block")]
pub struct Block {
    /// The header of the block
    pub block_header: BlockHeader,
    /// A miner-provided proof of leadership
    pub proof: LeadershipProof,
    /// A non-empty list of signed transactions
    pub txns: Vec<Transaction>,
}

impl<T: AsRef<[u8]>> Hashable for T {
    fn hash(&self) -> Hash {
        calculate_sha256(self.as_ref()).into()
    }
}

impl Hashable for Block {
    fn hash(&self) -> Hash {
        calculate_sha256(&self.block_header.to_pb_bytes().unwrap()).into()
    }
}

impl Hashable for CheckpointBeacon {
    fn hash(&self) -> Hash {
        calculate_sha256(&self.to_pb_bytes().unwrap()).into()
    }
}

impl Hashable for TransactionBody {
    fn hash(&self) -> Hash {
        self.cached_hash()
    }
}

impl Hashable for Transaction {
    fn hash(&self) -> Hash {
        self.body.hash()
    }
}

impl Hashable for PublicKey {
    fn hash(&self) -> Hash {
        let mut v = vec![];
        v.extend(&[self.compressed]);
        v.extend(&self.bytes);

        calculate_sha256(&v).into()
    }
}

/// Block header structure
#[derive(Debug, Eq, PartialEq, Clone, Serialize, Deserialize, ProtobufConvert, Default)]
#[protobuf_convert(pb = "witnet::Block_BlockHeader")]
pub struct BlockHeader {
    /// The block version number indicating the block validation rules
    pub version: u32,
    /// A checkpoint beacon for the epoch that this block is closing
    pub beacon: CheckpointBeacon,
    /// A 256-bit hash based on all of the transactions committed to this block
    pub hash_merkle_root: Hash,
}

/// Proof of leadership structure
#[derive(Debug, Eq, PartialEq, Clone, Serialize, Deserialize, ProtobufConvert, Default)]
#[protobuf_convert(pb = "witnet::Block_LeadershipProof")]
pub struct LeadershipProof {
    /// An enveloped signature of the block header except the `proof` part
    pub block_sig: KeyedSignature,
}

/// Digital signatures structure (based on supported cryptosystems)
#[derive(Debug, Eq, PartialEq, Clone, Serialize, Deserialize, ProtobufConvert)]
#[protobuf_convert(pb = "witnet::Signature")]
pub enum Signature {
    /// ECDSA over secp256k1
    Secp256k1(Secp256k1Signature),
}

impl Default for Signature {
    fn default() -> Self {
        Signature::Secp256k1(Secp256k1Signature::default())
    }
}

/// ECDSA (over secp256k1) signature
#[derive(Debug, Default, Eq, PartialEq, Clone, Serialize, Deserialize, ProtobufConvert)]
#[protobuf_convert(pb = "witnet::Secp256k1Signature")]
pub struct Secp256k1Signature {
    /// The signature serialized in DER
    pub der: Vec<u8>,
}

/// The error type for operations on a [`Secp256k1Signature`](Secp256k1Signature)
#[derive(Debug, PartialEq, Fail)]
pub enum Secp256k1ConversionError {
    #[fail(
        display = "Failed to convert `witnet_data_structures::Signature` into `secp256k1::Signature`"
    )]
    FailSignatureConversion,
    #[fail(
        display = " Failed to convert `witnet_data_structures::PublicKey` into `secp256k1::PublicKey`"
    )]
    FailPublicKeyConversion,
    #[fail(
        display = " Failed to convert `witnet_data_structures::SecretKey` into `secp256k1::SecretKey`"
    )]
    FailSecretKeyConversion,
}

impl From<Secp256k1_Signature> for Signature {
    fn from(secp256k1_signature: Secp256k1_Signature) -> Self {
        Signature::Secp256k1(Secp256k1Signature::from(secp256k1_signature))
    }
}

impl TryInto<Secp256k1_Signature> for Signature {
    type Error = failure::Error;

    fn try_into(self) -> Result<Secp256k1_Signature, Self::Error> {
        let x = match self {
            Signature::Secp256k1(y) => Secp256k1Signature::try_into(y)?,
        };
        Ok(x)
    }
}

impl From<Secp256k1_Signature> for Secp256k1Signature {
    fn from(secp256k1_signature: Secp256k1_Signature) -> Self {
        let der = secp256k1_signature.serialize_der();

        Secp256k1Signature { der }
    }
}

impl TryInto<Secp256k1_Signature> for Secp256k1Signature {
    type Error = failure::Error;

    fn try_into(self) -> Result<Secp256k1_Signature, Self::Error> {
        Secp256k1_Signature::from_der(&self.der)
            .map_err(|_| Secp256k1ConversionError::FailSignatureConversion.into())
    }
}

impl From<Secp256k1_PublicKey> for PublicKey {
    fn from(secp256k1_pk: Secp256k1_PublicKey) -> Self {
        let serialize = secp256k1_pk.serialize();
        let mut bytes: [u8; 32] = [0; 32];
        bytes.copy_from_slice(&serialize[1..]);

        PublicKey {
            compressed: serialize[0],
            bytes,
        }
    }
}

impl TryInto<Secp256k1_PublicKey> for PublicKey {
    type Error = failure::Error;

    fn try_into(self) -> Result<Secp256k1_PublicKey, Self::Error> {
        let mut pk_ser = vec![];

        pk_ser.extend_from_slice(&[self.compressed]);
        pk_ser.extend_from_slice(&self.bytes);

        Secp256k1_PublicKey::from_slice(&pk_ser)
            .map_err(|_| Secp256k1ConversionError::FailPublicKeyConversion.into())
    }
}

impl From<Secp256k1_SecretKey> for SecretKey {
    fn from(secp256k1_sk: Secp256k1_SecretKey) -> Self {
        let mut bytes: [u8; 32] = [0; 32];
        bytes.copy_from_slice(&secp256k1_sk[..]);

        SecretKey { bytes }
    }
}

impl Into<Secp256k1_SecretKey> for SecretKey {
    fn into(self) -> Secp256k1_SecretKey {
        Secp256k1_SecretKey::from_slice(&self.bytes).unwrap()
    }
}

impl From<ExtendedSK> for ExtendedSecretKey {
    fn from(extended_sk: ExtendedSK) -> Self {
        ExtendedSecretKey {
            secret_key: SecretKey::from(extended_sk.secret_key),
            chain_code: extended_sk.chain_code,
        }
    }
}

impl Into<ExtendedSK> for ExtendedSecretKey {
    fn into(self) -> ExtendedSK {
        let secret_key = self.secret_key.into();

        ExtendedSK {
            secret_key,
            chain_code: self.chain_code,
        }
    }
}

/// Hash
#[derive(Eq, PartialEq, Ord, PartialOrd, Copy, Clone, Serialize, Hash, ProtobufConvert)]
#[protobuf_convert(pb = "witnet::Hash")]
pub enum Hash {
    /// SHA-256 Hash
    SHA256(SHA256),
}

impl Default for Hash {
    fn default() -> Hash {
        Hash::SHA256([0; 32])
    }
}

/// Conversion between witnet_crypto::Sha256 and witnet_data_structures::Hash
impl From<Sha256> for Hash {
    fn from(x: Sha256) -> Self {
        Hash::SHA256(x.0)
    }
}

impl fmt::Display for Hash {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Hash::SHA256(h) => f.write_str(
                h.iter()
                    .fold(String::new(), |acc, x| format!("{}{:02x}", acc, x))
                    .as_str(),
            )?,
        };

        Ok(())
    }
}

impl fmt::Debug for Hash {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self)
    }
}

/// Helper type to allow deserialization of hashes in string format:
/// "01234abcd..." instead of [0x01, 0x23, 0x4a, 0xbc, ...]
#[derive(Deserialize)]
#[serde(untagged)]
enum HashSerializationHelper {
    Normal {
        #[serde(rename = "SHA256")]
        hash: SHA256,
    },
    AsString(String),
}

impl<'de> Deserialize<'de> for Hash {
    fn deserialize<D>(deserializer: D) -> Result<Hash, D::Error>
    where
        D: Deserializer<'de>,
    {
        let hash_deser = HashSerializationHelper::deserialize(deserializer)?;

        match hash_deser {
            HashSerializationHelper::Normal { hash } => Ok(Hash::SHA256(hash)),
            HashSerializationHelper::AsString(hash_str) => {
                let mut sha256: SHA256 = [0; 32];
                let sha256_bytes = parse_hex(&hash_str);
                if sha256_bytes.len() != 32 {
                    Err(<D::Error as serde::de::Error>::invalid_length(
                        sha256_bytes.len(),
                        &"32",
                    ))
                } else {
                    sha256.copy_from_slice(&sha256_bytes);

                    Ok(Hash::SHA256(sha256))
                }
            }
        }
    }
}

/// SHA-256 Hash
pub type SHA256 = [u8; 32];

/// Public Key Hash: slice of the digest of a public key (20 bytes).
///
/// It is the first 20 bytes of the SHA256 hash of the PublicKey.
#[derive(
    Copy, Clone, Debug, Default, Eq, PartialEq, Hash, Serialize, Deserialize, ProtobufConvert,
)]
#[protobuf_convert(pb = "witnet::PublicKeyHash")]
pub struct PublicKeyHash {
    hash: [u8; 20],
}

impl PublicKeyHash {
    /// Calculate the hash of the provided public key
    pub fn from_public_key(pk: &PublicKey) -> Self {
        let mut pkh = [0; 20];
        let Hash::SHA256(h) = pk.hash();
        pkh.copy_from_slice(&h[..20]);

        Self { hash: pkh }
    }
}

/// Transaction data structure
#[derive(Debug, Default, Eq, PartialEq, Clone, Serialize, Deserialize, ProtobufConvert)]
#[protobuf_convert(pb = "witnet::TransactionBody")]
pub struct TransactionBody {
    pub version: u32,
    pub inputs: Vec<Input>,
    pub outputs: Vec<Output>,
    #[protobuf_convert(skip)]
    hash: Cell<Option<Hash>>,
}

/// Signed transaction data structure
#[derive(Debug, Default, Eq, PartialEq, Clone, Serialize, Deserialize, ProtobufConvert)]
#[protobuf_convert(pb = "witnet::Transaction")]
pub struct Transaction {
    pub body: TransactionBody,
    pub signatures: Vec<KeyedSignature>,
}

/// Transaction tags for validation process
pub enum TransactionType {
    InvalidType,
    ValueTransfer,
    DataRequest,
    Commit,
    Reveal,
    Tally,
    Mint,
}

impl TransactionBody {
    /// Creates a new transaction from inputs and outputs.
    pub fn new(version: u32, inputs: Vec<Input>, outputs: Vec<Output>) -> Self {
        TransactionBody {
            version,
            inputs,
            outputs,
            hash: Cell::new(None),
        }
    }
    /// Returns the size a transaction body will have on the wire in bytes
    pub fn size(&self) -> u32 {
        self.to_pb().write_to_bytes().unwrap().len() as u32
    }

    /// Return the value of the output with index `index`.
    pub fn get_output_value(&self, index: usize) -> Option<u64> {
        self.outputs.get(index).map(Output::value)
    }

    /// Return the cached hash of the transaction or compute it on-the-fly
    pub fn cached_hash(&self) -> Hash {
        match self.hash.get() {
            Some(hash) => hash,
            None => {
                let hash = self
                    .to_pb_bytes()
                    .map(|bytes| calculate_sha256(&*bytes).into())
                    .ok();
                self.hash.set(hash);

                hash.unwrap()
            }
        }
    }
}

impl AsRef<TransactionBody> for TransactionBody {
    fn as_ref(&self) -> &Self {
        self
    }
}

impl Transaction {
    /// Attaches signatures to a transaction data structure and returns the result as a SignedTransaction structure
    pub fn new(transaction: TransactionBody, signatures: Vec<KeyedSignature>) -> Self {
        Transaction {
            body: transaction,
            signatures,
        }
    }

    /// Returns the size a transaction will have on the wire in bytes
    pub fn size(&self) -> u32 {
        self.to_pb().write_to_bytes().unwrap().len() as u32
    }
}

impl AsRef<Transaction> for Transaction {
    fn as_ref(&self) -> &Self {
        self
    }
}

/// Input data structure
#[derive(Debug, Eq, PartialEq, Clone, Serialize, Deserialize, ProtobufConvert)]
#[protobuf_convert(pb = "witnet::TransactionBody_Input")]
pub enum Input {
    Commit(CommitInput),
    DataRequest(DataRequestInput),
    Reveal(RevealInput),
    ValueTransfer(ValueTransferInput),
}

impl Input {
    /// Return the [`OutputPointer`](OutputPointer) of an input.
    pub fn output_pointer(&self) -> OutputPointer {
        // TODO: Potential refactor (redundant `match`)
        match self {
            Input::Commit(input) => OutputPointer {
                transaction_id: input.transaction_id,
                output_index: input.output_index,
            },
            Input::DataRequest(input) => OutputPointer {
                transaction_id: input.transaction_id,
                output_index: input.output_index,
            },
            Input::Reveal(input) => OutputPointer {
                transaction_id: input.transaction_id,
                output_index: input.output_index,
            },
            Input::ValueTransfer(input) => OutputPointer {
                transaction_id: input.transaction_id,
                output_index: input.output_index,
            },
        }
    }
}

/// Value transfer input transaction data structure
#[derive(Debug, Eq, PartialEq, Clone, Serialize, Deserialize, ProtobufConvert, Default)]
#[protobuf_convert(pb = "witnet::TransactionBody_Input_ValueTransferInput")]
pub struct ValueTransferInput {
    pub transaction_id: Hash,
    pub output_index: u32,
}

/// Commit input transaction data structure
#[derive(Debug, Eq, PartialEq, Clone, Serialize, Deserialize, ProtobufConvert, Default)]
#[protobuf_convert(pb = "witnet::TransactionBody_Input_CommitInput")]
pub struct CommitInput {
    pub transaction_id: Hash,
    pub output_index: u32,
    pub nonce: u64,
}

/// Commit input transaction data structure
#[derive(Debug, Eq, PartialEq, Clone, Serialize, Deserialize, ProtobufConvert, Default)]
#[protobuf_convert(pb = "witnet::TransactionBody_Input_DataRequestInput")]
pub struct DataRequestInput {
    pub transaction_id: Hash,
    pub output_index: u32,
    pub poe: [u8; 32],
}

/// Reveal input transaction data structure
#[derive(Debug, Eq, PartialEq, Clone, Serialize, Deserialize, ProtobufConvert, Default)]
#[protobuf_convert(pb = "witnet::TransactionBody_Input_RevealInput")]
pub struct RevealInput {
    pub transaction_id: Hash,
    pub output_index: u32,
}

/// Output data structure
#[derive(Debug, Eq, PartialEq, Clone, Serialize, Deserialize, ProtobufConvert)]
#[protobuf_convert(pb = "witnet::TransactionBody_Output")]
pub enum Output {
    ValueTransfer(ValueTransferOutput),
    DataRequest(DataRequestOutput),
    Commit(CommitOutput),
    Reveal(RevealOutput),
    Tally(TallyOutput),
}

impl Output {
    /// Return the value of an output.
    pub fn value(&self) -> u64 {
        match self {
            Output::Commit(output) => output.value,
            Output::Tally(output) => output.value,
            Output::DataRequest(output) => {
                output.value + output.commit_fee + output.reveal_fee + output.tally_fee
            }
            Output::Reveal(output) => output.value,
            Output::ValueTransfer(output) => output.value,
        }
    }
}

/// Value transfer output transaction data structure
#[derive(Debug, Eq, PartialEq, Clone, Serialize, Deserialize, ProtobufConvert, Default)]
#[protobuf_convert(pb = "witnet::TransactionBody_Output_ValueTransferOutput")]
pub struct ValueTransferOutput {
    pub pkh: PublicKeyHash,
    pub value: u64,
}

/// Data request output transaction data structure
#[derive(Debug, Eq, PartialEq, Clone, Serialize, Deserialize, ProtobufConvert, Hash, Default)]
#[protobuf_convert(pb = "witnet::TransactionBody_Output_DataRequestOutput")]
pub struct DataRequestOutput {
    pub pkh: PublicKeyHash,
    pub data_request: RADRequest,
    pub value: u64,
    pub witnesses: u16,
    pub backup_witnesses: u16,
    pub commit_fee: u64,
    pub reveal_fee: u64,
    pub tally_fee: u64,
    pub time_lock: u64,
}

/// Commit output transaction data structure
#[derive(Debug, Eq, PartialEq, Clone, Serialize, Deserialize, ProtobufConvert, Default)]
#[protobuf_convert(pb = "witnet::TransactionBody_Output_CommitOutput")]
pub struct CommitOutput {
    pub commitment: Hash,
    pub value: u64,
}

/// Reveal output transaction data structure
#[derive(Debug, Eq, PartialEq, Clone, Serialize, Deserialize, ProtobufConvert, Default)]
#[protobuf_convert(pb = "witnet::TransactionBody_Output_RevealOutput")]
pub struct RevealOutput {
    pub reveal: Vec<u8>,
    pub pkh: PublicKeyHash,
    pub value: u64,
}

#[derive(Debug, Eq, PartialEq, Clone, Serialize, Deserialize, ProtobufConvert, Default)]
#[protobuf_convert(pb = "witnet::TransactionBody_Output_TallyOutput")]
pub struct TallyOutput {
    pub result: Vec<u8>,
    pub pkh: PublicKeyHash,
    pub value: u64,
}

/// Keyed signature data structure
#[derive(Debug, Default, Eq, PartialEq, Clone, Serialize, Deserialize, ProtobufConvert)]
#[protobuf_convert(pb = "witnet::KeyedSignature")]
pub struct KeyedSignature {
    pub signature: Signature,
    pub public_key: PublicKey,
}

/// Public Key data structure
#[derive(Debug, Default, Eq, PartialEq, Clone, Serialize, Deserialize)]
pub struct PublicKey {
    pub compressed: u8,
    pub bytes: [u8; 32],
}

/// Secret Key data structure
#[derive(Debug, Default, Eq, PartialEq, Clone, Serialize, Deserialize)]
pub struct SecretKey {
    // TODO(#560): Use Protected type
    pub bytes: [u8; 32],
}

/// Extended Secret Key data structure
#[derive(Debug, Default, Eq, PartialEq, Clone, Serialize, Deserialize)]
pub struct ExtendedSecretKey {
    /// Secret key
    pub secret_key: SecretKey,
    /// Chain code
    // TODO(#560): Use Protected type
    pub chain_code: [u8; 32],
}

#[derive(Debug, Eq, PartialEq, Clone, Serialize, Deserialize, Hash)]
pub enum RADType {
    #[serde(rename = "HTTP-GET")]
    HttpGet,
}

impl Default for RADType {
    fn default() -> Self {
        RADType::HttpGet
    }
}

/// RAD request data structure
#[derive(Debug, Eq, PartialEq, Clone, Serialize, Deserialize, ProtobufConvert, Hash, Default)]
#[protobuf_convert(
    pb = "witnet::TransactionBody_Output_DataRequestOutput_RADRequest",
    crate = "crate"
)]
pub struct RADRequest {
    pub not_before: u64,
    pub retrieve: Vec<RADRetrieve>,
    pub aggregate: RADAggregate,
    pub consensus: RADConsensus,
    pub deliver: Vec<RADDeliver>,
}

#[derive(Debug, Eq, PartialEq, Clone, Serialize, Deserialize, ProtobufConvert, Hash, Default)]
#[protobuf_convert(
    pb = "witnet::TransactionBody_Output_DataRequestOutput_RADRequest_RADRetrieve",
    crate = "crate"
)]
pub struct RADRetrieve {
    pub kind: RADType,
    pub url: String,
    pub script: Vec<u8>,
}

#[derive(Debug, Eq, PartialEq, Clone, Serialize, Deserialize, ProtobufConvert, Hash, Default)]
#[protobuf_convert(
    pb = "witnet::TransactionBody_Output_DataRequestOutput_RADRequest_RADAggregate",
    crate = "crate"
)]
pub struct RADAggregate {
    pub script: Vec<u8>,
}

#[derive(Debug, Eq, PartialEq, Clone, Serialize, Deserialize, ProtobufConvert, Hash, Default)]
#[protobuf_convert(
    pb = "witnet::TransactionBody_Output_DataRequestOutput_RADRequest_RADConsensus",
    crate = "crate"
)]
pub struct RADConsensus {
    pub script: Vec<u8>,
}

#[derive(Debug, Eq, PartialEq, Clone, Serialize, Deserialize, ProtobufConvert, Hash, Default)]
#[protobuf_convert(
    pb = "witnet::TransactionBody_Output_DataRequestOutput_RADRequest_RADDeliver",
    crate = "crate"
)]
pub struct RADDeliver {
    pub kind: RADType,
    pub url: String,
}

type WeightedHash = (u64, Hash);
type WeightedTransaction = (u64, Transaction);

/// Auxiliar methods to get the output pointer from an input

impl CommitInput {
    pub fn output_pointer(&self) -> OutputPointer {
        OutputPointer {
            transaction_id: self.transaction_id,
            output_index: self.output_index,
        }
    }
}

impl DataRequestInput {
    pub fn output_pointer(&self) -> OutputPointer {
        OutputPointer {
            transaction_id: self.transaction_id,
            output_index: self.output_index,
        }
    }
}

impl RevealInput {
    pub fn output_pointer(&self) -> OutputPointer {
        OutputPointer {
            transaction_id: self.transaction_id,
            output_index: self.output_index,
        }
    }
}

impl ValueTransferInput {
    pub fn output_pointer(&self) -> OutputPointer {
        OutputPointer {
            transaction_id: self.transaction_id,
            output_index: self.output_index,
        }
    }
}

/// A pool of validated transactions that supports constant access by
/// [`Hash`](Hash) and iteration over the
/// transactions sorted from by transactions with bigger fees to
/// transactions with smaller fees.
#[derive(Debug, Default, Clone)]
pub struct TransactionsPool {
    transactions: HashMap<Hash, WeightedTransaction>,
    sorted_index: BTreeSet<WeightedHash>,
}

impl TransactionsPool {
    /// Makes a new empty pool of transactions.
    ///
    /// # Examples:
    ///
    /// ```
    /// # use witnet_data_structures::chain::TransactionsPool;
    /// let pool = TransactionsPool::new();
    /// ```
    pub fn new() -> Self {
        TransactionsPool {
            transactions: HashMap::new(),
            sorted_index: BTreeSet::new(),
        }
    }

    /// Makes a new pool of transactions with the specified capacity.
    ///
    /// # Examples:
    ///
    /// ```
    /// # use witnet_data_structures::chain::TransactionsPool;
    /// let pool = TransactionsPool::with_capacity(20);
    /// ```
    pub fn with_capacity(capacity: usize) -> Self {
        TransactionsPool {
            transactions: HashMap::with_capacity(capacity),
            sorted_index: BTreeSet::new(),
        }
    }

    /// Returns the number of transactions the pool can hold without
    /// reallocating.
    ///
    /// This number is a lower bound; the pool might be able to hold
    /// more, but is guaranteed to be able to hold at least this many.
    ///
    /// # Examples:
    ///
    /// ```
    /// # use witnet_data_structures::chain::TransactionsPool;
    /// let pool = TransactionsPool::with_capacity(20);
    ///
    /// assert!(pool.capacity() >= 20);
    /// ```
    pub fn capacity(&self) -> usize {
        self.transactions.capacity()
    }

    /// Returns `true` if the pool contains no transactions.
    ///
    /// # Examples:
    ///
    /// ```
    /// # use witnet_data_structures::chain::TransactionsPool;
    /// let mut pool = TransactionsPool::new();
    ///
    /// assert!(pool.is_empty());
    /// ```
    pub fn is_empty(&self) -> bool {
        self.transactions.is_empty()
    }

    /// Returns the number of transactions in the pool.
    ///
    /// # Examples:
    ///
    /// ```
    /// # use witnet_data_structures::chain::{TransactionsPool, TransactionBody, Hash, Transaction};
    /// let mut pool = TransactionsPool::new();
    ///
    /// let transaction = Transaction::default();
    ///
    /// assert_eq!(pool.len(), 0);
    ///
    /// pool.insert(Hash::SHA256([0 as u8; 32]), transaction);
    ///
    /// assert_eq!(pool.len(), 1);
    /// ```
    pub fn len(&self) -> usize {
        self.transactions.len()
    }

    /// Returns `true` if the pool contains a transaction for the specified hash.
    ///
    /// The `key` may be any borrowed form of the hash, but `Hash` and
    /// `Eq` on the borrowed form must match those for the key type.
    ///
    /// # Examples:
    /// ```
    /// # use witnet_data_structures::chain::{TransactionsPool, TransactionBody, Hash, Transaction};
    /// let mut pool = TransactionsPool::new();
    /// let hash = Hash::SHA256([0 as u8; 32]);
    /// let transaction = Transaction::default();
    /// assert!(!pool.contains(&hash));
    ///
    /// pool.insert(hash, transaction);
    ///
    /// assert!(pool.contains(&hash));
    /// ```
    pub fn contains(&self, key: &Hash) -> bool {
        self.transactions.contains_key(key)
    }

    /// Returns an `Option` with the transaction for the specified hash or `None` if not exist.
    ///
    /// The `key` may be any borrowed form of the hash, but `Hash` and
    /// `Eq` on the borrowed form must match those for the key type.
    ///
    /// # Examples:
    /// ```
    /// # use witnet_data_structures::chain::{TransactionsPool, TransactionBody, Hash, Transaction};
    /// let mut pool = TransactionsPool::new();
    /// let hash = Hash::SHA256([0 as u8; 32]);
    /// let transaction = Transaction::default();
    /// pool.insert(hash, transaction.clone());
    ///
    /// assert!(pool.contains(&hash));
    ///
    /// let op_transaction_removed = pool.remove(&hash);
    ///
    /// assert_eq!(Some(transaction), op_transaction_removed);
    /// assert!(!pool.contains(&hash));
    /// ```
    pub fn remove(&mut self, key: &Hash) -> Option<Transaction> {
        self.transactions.remove(key).map(|(weight, transaction)| {
            self.sorted_index.remove(&(weight, *key));
            transaction
        })
    }

    /// Insert a transaction identified by `key` into the pool.
    ///
    /// # Examples:
    ///
    /// ```
    /// # use witnet_data_structures::chain::{TransactionsPool, TransactionBody, Hash, Transaction};
    /// let mut pool = TransactionsPool::new();
    /// let transaction = Transaction::default();
    /// pool.insert(Hash::SHA256([0 as u8; 32]), transaction);
    ///
    /// assert!(!pool.is_empty());
    /// ```
    pub fn insert(&mut self, key: Hash, transaction: Transaction) {
        let weight = 0; // TODO: weight = transaction-fee / transaction-weight
        self.transactions.insert(key, (weight, transaction));
        self.sorted_index.insert((weight, key));
    }

    /// An iterator visiting all the transactions in the pool in
    /// descending-fee order, that is, transactions with bigger fees
    /// come first.
    ///
    /// Examples:
    ///
    /// ```
    /// # use witnet_data_structures::chain::{TransactionsPool, TransactionBody, Hash, Transaction};
    /// let mut pool = TransactionsPool::new();
    ///
    /// let transaction = Transaction::default();
    ///
    /// pool.insert(Hash::SHA256([0 as u8; 32]), transaction.clone());
    /// pool.insert(Hash::SHA256([0 as u8; 32]), transaction);
    ///
    /// let mut iter = pool.iter();
    /// let tx1 = iter.next();
    /// let tx2 = iter.next();
    ///
    /// // TODO: assert!(tx1.weight() >= tx2.weight());
    /// ```
    pub fn iter(&self) -> impl Iterator<Item = &Transaction> {
        self.sorted_index
            .iter()
            .rev()
            .filter_map(move |(_, h)| self.transactions.get(h).map(|(_, t)| t))
    }

    /// Returns a reference to the value corresponding to the key.
    ///
    /// Examples:
    ///
    /// ```
    /// # use witnet_data_structures::chain::{TransactionsPool, TransactionBody, Hash, Transaction};
    /// let mut pool = TransactionsPool::new();
    /// let hash = Hash::SHA256([0 as u8; 32]);
    ///
    /// let transaction = Transaction::default();
    ///
    /// assert!(pool.get(&hash).is_none());
    ///
    /// pool.insert(hash, transaction);
    ///
    /// assert!(pool.get(&hash).is_some());
    /// ```
    pub fn get(&self, key: &Hash) -> Option<&Transaction> {
        self.transactions
            .get(key)
            .map(|(_, transaction)| transaction)
    }

    /// Retains only the elements specified by the predicate.
    ///
    /// In other words, remove all transactions such that
    /// `f(&Hash, &Transaction)` returns `false`.
    ///
    /// # Examples
    ///
    /// ```
    /// # use witnet_data_structures::chain::{TransactionsPool, TransactionBody, Hash, Transaction};
    ///
    /// let mut pool = TransactionsPool::new();
    ///
    /// let transaction = Transaction::default();
    ///
    /// pool.insert(Hash::SHA256([0 as u8; 32]), transaction.clone());
    /// pool.insert(Hash::SHA256([1 as u8; 32]), transaction);
    /// assert_eq!(pool.len(), 2);
    /// pool.retain(|h, _| match h { Hash::SHA256(n) => n[0]== 0 });
    /// assert_eq!(pool.len(), 1);
    /// ```
    pub fn retain<F>(&mut self, mut f: F)
    where
        F: FnMut(&Hash, &Transaction) -> bool,
    {
        let TransactionsPool {
            ref mut transactions,
            ref mut sorted_index,
        } = *self;

        transactions.retain(|hash, (weight, transaction)| {
            let retain = f(hash, transaction);
            if !retain {
                sorted_index.remove(&(*weight, *hash));
            }

            retain
        });
    }
}

/// Unspent output data structure (equivalent of Bitcoin's UTXO)
/// It is used to locate the output by its transaction identifier and its position
#[derive(Default, Hash, Clone, Eq, PartialEq)]
pub struct OutputPointer {
    pub transaction_id: Hash,
    pub output_index: u32,
}

impl fmt::Display for OutputPointer {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str(&format!("{}:{}", &self.transaction_id, &self.output_index))
    }
}

impl fmt::Debug for OutputPointer {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self)
    }
}

impl FromStr for OutputPointer {
    type Err = OutputPointerParseError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if !s.trim().contains(':') {
            return Err(OutputPointerParseError::MissingColon);
        }

        let mut tokens = s.trim().split(':');

        let transaction_id: &str = tokens
            .next()
            .ok_or(OutputPointerParseError::InvalidHashLength)?;
        let output_index = tokens
            .next()
            .ok_or(OutputPointerParseError::MissingColon)?
            .parse::<u32>()
            .map_err(OutputPointerParseError::ParseIntError)?;

        Ok(OutputPointer {
            output_index,
            transaction_id: {
                let mut sha256: SHA256 = [0; 32];
                let sha256_bytes = parse_hex(&transaction_id);
                if sha256_bytes.len() != 32 {
                    return Err(OutputPointerParseError::InvalidHashLength);
                }
                sha256.copy_from_slice(&sha256_bytes);

                Hash::SHA256(sha256)
            },
        })
    }
}

impl Serialize for OutputPointer {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&self.to_string())
    }
}

impl<'de> Deserialize<'de> for OutputPointer {
    fn deserialize<D>(deserializer: D) -> Result<OutputPointer, D::Error>
    where
        D: Deserializer<'de>,
    {
        let output_pointer_as_str = String::deserialize(deserializer)?;

        Self::from_str(&output_pointer_as_str).map_err(<D::Error as serde::de::Error>::custom)
    }
}

/// Inventory entry data structure
#[derive(Debug, Eq, PartialEq, Clone, Serialize, Deserialize, ProtobufConvert)]
#[protobuf_convert(pb = "witnet::InventoryEntry")]
pub enum InventoryEntry {
    Error(Hash),
    Tx(Hash),
    Block(Hash),
    DataRequest(Hash),
    DataResult(Hash),
}

/// Inventory element: block, txns
#[derive(Debug, Eq, PartialEq, Clone, Serialize, Deserialize)]
pub enum InventoryItem {
    #[serde(rename = "transaction")]
    Transaction(Transaction),
    #[serde(rename = "block")]
    Block(Block),
}

/// Data request report to be persisted into Storage and
/// using as index the Data Request OutputPointer
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct DataRequestReport {
    /// List of commitment output pointers to resolve the data request
    pub commits: Vec<OutputPointer>,
    /// List of reveal output pointers to the commitments (contains the data request result of the witnet)
    pub reveals: Vec<OutputPointer>,
    /// Tally output pointer (contains final result)
    pub tally: OutputPointer,
}

impl TryFrom<DataRequestInfo> for DataRequestReport {
    type Error = &'static str;

    fn try_from(x: DataRequestInfo) -> Result<Self, &'static str> {
        if let Some(tally) = x.tally {
            Ok(DataRequestReport {
                commits: x.commits.into_iter().collect(),
                reveals: x.reveals.into_iter().collect(),
                tally,
            })
        } else {
            Err("Cannot persist unfinished data request (with no Tally)")
        }
    }
}

/// List of outputs related to a data request
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct DataRequestInfo {
    /// List of commitments to resolve the data request
    pub commits: HashSet<OutputPointer>,
    /// List of reveals to the commitments (contains the data request witnet result)
    pub reveals: HashSet<OutputPointer>,
    /// Tally of data request (contains final result)
    pub tally: Option<OutputPointer>,
}

/// State of data requests in progress (stored in memory)
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct DataRequestState {
    /// Data request output (contains all required information to process it)
    pub data_request: DataRequestOutput,
    /// List of outputs related to this data request
    pub info: DataRequestInfo,
    /// Current stage of this data request
    pub stage: DataRequestStage,
    /// The epoch on which this data request has been or will be unlocked
    // (necessary for removing from the data_requests_by_epoch map)
    pub epoch: Epoch,
}

impl DataRequestState {
    /// Add a new data request state
    pub fn new(data_request: DataRequestOutput, epoch: Epoch) -> Self {
        let info = DataRequestInfo::default();
        let stage = DataRequestStage::COMMIT;

        Self {
            data_request,
            info,
            stage,
            epoch,
        }
    }

    /// Add commit
    pub fn add_commit(&mut self, output_pointer: OutputPointer) {
        assert_eq!(self.stage, DataRequestStage::COMMIT);
        self.info.commits.insert(output_pointer);
    }

    /// Add reveal
    pub fn add_reveal(&mut self, output_pointer: OutputPointer) {
        assert_eq!(self.stage, DataRequestStage::REVEAL);
        self.info.reveals.insert(output_pointer);
    }

    /// Add tally and return the data request report
    pub fn add_tally(
        mut self,
        output_pointer: OutputPointer,
    ) -> (DataRequestOutput, DataRequestReport) {
        assert_eq!(self.stage, DataRequestStage::TALLY);
        self.info.tally = Some(output_pointer);
        // This try_from can only fail if the tally is None, and we have just set it to Some
        (
            self.data_request,
            DataRequestReport::try_from(self.info).unwrap(),
        )
    }

    /// Advance to the next stage, returning true on success.
    /// Since the data requests are updated by looking at the transactions from a valid block,
    /// the only issue would be that there were no commits in that block.
    pub fn update_stage(&mut self) -> bool {
        let old_stage = self.stage;

        self.stage = match self.stage {
            DataRequestStage::COMMIT => {
                if self.info.commits.is_empty() {
                    DataRequestStage::COMMIT
                } else {
                    DataRequestStage::REVEAL
                }
            }
            DataRequestStage::REVEAL => {
                if self.info.reveals.is_empty() {
                    DataRequestStage::REVEAL
                } else {
                    DataRequestStage::TALLY
                }
            }
            DataRequestStage::TALLY => {
                if self.info.tally.is_none() {
                    DataRequestStage::TALLY
                } else {
                    panic!("Data request in tally stage should have been removed from the pool");
                }
            }
        };

        self.stage != old_stage
    }
}

/// Data request current stage
#[derive(Copy, Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum DataRequestStage {
    /// Expecting commitments for data request
    COMMIT,
    /// Expecting reveals to previously published commitments
    REVEAL,
    /// Expecting tally to be included in block
    TALLY,
}

pub type UnspentOutputsPool = HashMap<OutputPointer, Output>;

pub type Blockchain = BTreeMap<Epoch, Hash>;

/// Blockchain state (valid at a certain epoch)
#[derive(Debug, Default, Serialize, Deserialize, PartialEq, Clone)]
pub struct ChainState {
    /// Blockchain information data structure
    pub chain_info: Option<ChainInfo>,
    /// Unspent Outputs Pool
    pub unspent_outputs_pool: UnspentOutputsPool,
    /// Collection of state structures for active data requests
    pub data_request_pool: DataRequestPool,
    /// List of consolidated blocks by epoch
    pub block_chain: Blockchain,
}

impl ChainState {
    /// Method to check that all inputs point to unspent outputs
    pub fn find_unspent_outputs(&self, inputs: &[Input]) -> bool {
        inputs.iter().all(|tx_input| {
            let output_pointer = tx_input.output_pointer();

            self.unspent_outputs_pool.contains_key(&output_pointer)
        })
    }
    /// Retrieve the output pointed by the output pointer in an input
    pub fn get_output_from_input(&self, input: &Input) -> Option<&Output> {
        let output_pointer = input.output_pointer();

        self.unspent_outputs_pool.get(&output_pointer)
    }
    /// Map a vector of inputs to a the vector of outputs pointed by the inputs' output pointers
    pub fn get_outputs_from_inputs(&self, inputs: &[Input]) -> Result<Vec<Output>, Input> {
        let v = inputs
            .iter()
            .map(|i| self.get_output_from_input(i))
            .fuse()
            .flatten()
            .cloned()
            .collect();

        Ok(v)
    }
    /// Method to update the unspent outputs pool
    pub fn generate_unspent_outputs_pool(&self, block: &Block) -> UnspentOutputsPool {
        // Create a copy of the state "unspent_outputs_pool"
        let mut unspent_outputs = self.unspent_outputs_pool.clone();

        let transactions = &block.txns;

        for transaction in transactions {
            let txn_hash = transaction.hash();
            for input in &transaction.body.inputs {
                // Obtain the OuputPointer of each input and remove it from the utxo_set
                let output_pointer = input.output_pointer();

                unspent_outputs.remove(&output_pointer);
            }

            for (index, output) in transaction.body.outputs.iter().enumerate() {
                // Add the new outputs to the utxo_set
                let output_pointer = OutputPointer {
                    transaction_id: txn_hash,
                    output_index: index as u32,
                };

                unspent_outputs.insert(output_pointer, output.clone());
            }
        }

        unspent_outputs
    }
}

// Auxiliar functions for test
pub fn transaction_example() -> Transaction {
    let keyed_signature = vec![KeyedSignature::default()];
    let reveal_input = Input::Reveal(RevealInput::default());
    let commit_input = Input::Commit(CommitInput::default());
    let data_request_input = Input::DataRequest(DataRequestInput::default());
    let value_transfer_output = Output::ValueTransfer(ValueTransferOutput::default());

    let rad_retrieve = RADRetrieve {
        url: "https://openweathermap.org/data/2.5/weather?id=2950159&appid=b6907d289e10d714a6e88b30761fae22".to_string(),
        ..RADRetrieve::default()
    };

    let rad_deliver_1 = RADDeliver {
        kind: RADType::HttpGet,
        url: "https://hooks.zapier.com/hooks/catch/3860543/l2awcd/".to_string(),
    };

    let rad_deliver_2 = RADDeliver {
        kind: RADType::HttpGet,
        url: "https://hooks.zapier.com/hooks/catch/3860543/l1awcw/".to_string(),
    };

    let rad_request = RADRequest {
        retrieve: vec![rad_retrieve.clone(), rad_retrieve],
        deliver: vec![rad_deliver_1, rad_deliver_2],
        ..RADRequest::default()
    };
    let data_request_output = Output::DataRequest(DataRequestOutput {
        data_request: rad_request,
        ..DataRequestOutput::default()
    });
    let commit_output = Output::Commit(CommitOutput::default());
    let reveal_output = Output::Reveal(RevealOutput::default());
    let consensus_output = Output::Tally(TallyOutput::default());

    let inputs = vec![commit_input, data_request_input, reveal_input];
    let outputs = vec![
        value_transfer_output,
        data_request_output,
        commit_output,
        reveal_output,
        consensus_output,
    ];

    Transaction::new(TransactionBody::new(0, inputs, outputs), keyed_signature)
}

pub fn block_example() -> Block {
    let block_header = BlockHeader::default();
    let proof = LeadershipProof::default();

    let txns: Vec<Transaction> = vec![transaction_example()];

    Block {
        block_header,
        proof,
        txns,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_block_hashable_trait() {
        let block = block_example();
        let expected = "41d36ff16318f17350b0f0a74afb907bda00b89035d12ccede8ca404a4afb1c0";
        assert_eq!(block.hash().to_string(), expected);
    }

    #[test]
    fn test_transaction_hashable_trait() {
        let transaction = transaction_example();
        let expected = "c6c3f9dd0f9522ff1b994424401448ce79e96b66b9f88741b534810631b51bcf";

        // Signatures don't affect the hash of a transaction (SegWit style), thus both must be equal
        assert_eq!(transaction.body.hash().to_string(), expected);
        assert_eq!(transaction.hash().to_string(), expected);
    }

    #[test]
    fn test_output_pointer_from_str() {
        let result_success = OutputPointer::from_str(
            "1111111111111111111111111111111111111111111111111111111111111111:1",
        );
        let result_error_short = OutputPointer::from_str("11111");

        let result_error_long = OutputPointer::from_str(
            "1111111111111111111111111111111111111111111111111111111111111111111:1",
        );

        let result_error_format_1 = OutputPointer::from_str(":");

        let result_error_format_2 = OutputPointer::from_str(
            "1111111111111111111111111111111111111111111111111111111111111111:b",
        );

        let result_error_format_3 = OutputPointer::from_str(
            "1111111111111111111111111111111111111111111111111111111111111111:1a",
        );

        let expected = OutputPointer {
            transaction_id: Hash::SHA256([
                17, 17, 17, 17, 17, 17, 17, 17, 17, 17, 17, 17, 17, 17, 17, 17, 17, 17, 17, 17, 17,
                17, 17, 17, 17, 17, 17, 17, 17, 17, 17, 17,
            ]),
            output_index: 1,
        };

        assert_eq!(result_success.unwrap(), expected);
        assert!(result_error_short.is_err());
        assert!(result_error_long.is_err());
        assert!(result_error_format_1.is_err());
        assert!(result_error_format_2.is_err());
        assert!(result_error_format_3.is_err());
    }

    #[test]
    fn secp256k1_from_into_secpk256k1_signatures() {
        use crate::chain::Secp256k1Signature;
        use secp256k1::{
            Message as Secp256k1_Message, Secp256k1, SecretKey as Secp256k1_SecretKey,
            Signature as Secp256k1_Signature,
        };

        let data = [0xab; 32];
        let secp = Secp256k1::new();
        let secret_key =
            Secp256k1_SecretKey::from_slice(&[0xcd; 32]).expect("32 bytes, within curve order");
        let msg = Secp256k1_Message::from_slice(&data).unwrap();
        let signature = secp.sign(&msg, &secret_key);

        let witnet_signature = Secp256k1Signature::from(signature);
        let signature_into: Secp256k1_Signature = witnet_signature.try_into().unwrap();

        assert_eq!(signature.to_string(), signature_into.to_string());
    }

    #[test]
    fn secp256k1_from_into_signatures() {
        use crate::chain::Signature;
        use secp256k1::{
            Message as Secp256k1_Message, Secp256k1, SecretKey as Secp256k1_SecretKey,
            Signature as Secp256k1_Signature,
        };

        let data = [0xab; 32];
        let secp = Secp256k1::new();
        let secret_key =
            Secp256k1_SecretKey::from_slice(&[0xcd; 32]).expect("32 bytes, within curve order");
        let msg = Secp256k1_Message::from_slice(&data).unwrap();
        let signature = secp.sign(&msg, &secret_key);

        let witnet_signature = Signature::from(signature);
        let signature_into: Secp256k1_Signature = witnet_signature.try_into().unwrap();

        assert_eq!(signature.to_string(), signature_into.to_string());
    }

    #[test]
    fn secp256k1_from_into_public_keys() {
        use crate::chain::PublicKey;
        use secp256k1::{
            PublicKey as Secp256k1_PublicKey, Secp256k1, SecretKey as Secp256k1_SecretKey,
        };

        let secp = Secp256k1::new();
        let secret_key =
            Secp256k1_SecretKey::from_slice(&[0xcd; 32]).expect("32 bytes, within curve order");
        let public_key = Secp256k1_PublicKey::from_secret_key(&secp, &secret_key);

        let witnet_pk = PublicKey::from(public_key);
        let pk_into: Secp256k1_PublicKey = witnet_pk.try_into().unwrap();

        assert_eq!(public_key, pk_into);
    }

    #[test]
    fn secp256k1_from_into_secret_keys() {
        use crate::chain::SecretKey;
        use secp256k1::{Secp256k1, SecretKey as Secp256k1_SecretKey};

        let secret_key =
            Secp256k1_SecretKey::from_slice(&[0xcd; 32]).expect("32 bytes, within curve order");

        let witnet_sk = SecretKey::from(secret_key);
        let sk_into: Secp256k1_SecretKey = witnet_sk.into();

        assert_eq!(secret_key, sk_into);
    }

    #[test]
    fn secp256k1_from_into_extended_sk() {
        use witnet_crypto::key::MasterKeyGen;

        let seed = [
            62, 6, 109, 125, 238, 45, 191, 143, 205, 63, 226, 64, 163, 151, 86, 88, 202, 17, 138,
            143, 111, 76, 168, 28, 249, 145, 4, 148, 70, 4, 176, 90, 80, 144, 167, 157, 153, 229,
            69, 112, 75, 145, 76, 160, 57, 127, 237, 184, 47, 208, 15, 214, 167, 32, 152, 112, 55,
            9, 200, 145, 160, 101, 238, 73,
        ];

        let extended_sk = MasterKeyGen::new(&seed[..]).generate().unwrap();

        let witnet_extended_sk = ExtendedSecretKey::from(extended_sk.clone());
        let extended_sk_into = witnet_extended_sk.into();

        assert_eq!(extended_sk, extended_sk_into);
    }

}
