use super::serializers::encoders::{
    build_block_flatbuffer, build_checkpoint_beacon_flatbuffer, build_transaction_flatbuffer,
    BlockArgs, CheckpointBeaconArgs, TransactionArgs,
};
use crate::serializers::decoders::TryFrom;
use partial_struct::PartialStruct;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::convert::AsRef;
use std::fmt;
use std::num::ParseIntError;
use std::str::FromStr;
use witnet_crypto::hash::{calculate_sha256, Sha256};
use witnet_util::parser::parse_hex;

use failure::Fail;

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
#[derive(PartialStruct, Debug, Clone, PartialEq, Serialize, Deserialize)]
#[partial_struct(derive(Deserialize, Default, Debug, Clone, PartialEq))]
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
#[derive(Debug, Default, Eq, PartialEq, Copy, Clone, Serialize, Deserialize)]
pub struct CheckpointBeacon {
    /// The serial number for an epoch
    pub checkpoint: Epoch,
    /// The 256-bit hash of the previous block header
    pub hash_prev_block: Hash,
}

/// Epoch id (starting from 0)
pub type Epoch = u32;

/// Block data structure
#[derive(Debug, Eq, PartialEq, Clone, Serialize, Deserialize)]
pub struct Block {
    /// The header of the block
    pub block_header: BlockHeader,
    /// A miner-provided proof of leadership
    pub proof: LeadershipProof,
    /// A non-empty list of transactions
    pub txns: Vec<Transaction>,
}

/// The error type for operations on a [`Block`](Block)
#[derive(Debug, PartialEq, Fail)]
pub enum BlockError {
    /// The block has no transactions in it.
    #[fail(display = "The block has no transactions")]
    Empty,
    /// The first transaction of the block is no mint.
    #[fail(display = "The block first transaction is not a mint transactions")]
    NoMint,
    /// There are multiple mint transactions in the block.
    #[fail(display = "The block has more than one mint transaction")]
    MultipleMint,
    /// The total value created by the mint transaction of the block,
    /// and the output value of the rest of the transactions, plus the
    /// block reward, don't add up
    #[fail(
        display = "The value of the mint transaction does not match the fess + reward of the blok"
    )]
    MismatchedMintValue,
}

impl Block {
    /// Check if the block is valid.
    ///
    /// The conditions for a block being valid are:
    /// * First transaction is a mint transaction
    /// * Rest of transactions in the block are valid and are not mint
    /// * The output of the mint transaction must be equal to the sum
    /// of the fees of the rest of the transactions in the block plus
    /// the block reward
    pub fn validate(
        &self,
        block_reward: u64,
        pool: &UnspentOutputsPool,
    ) -> Result<(), failure::Error> {
        let mint_transaction = self.txns.first().ok_or_else(|| BlockError::Empty)?;

        if !mint_transaction.is_mint() {
            Err(BlockError::NoMint)?
        }

        let mut total_fees = 0;
        for transaction in self.txns.iter().skip(1) {
            if transaction.is_mint() {
                Err(BlockError::MultipleMint)?;
            }
            total_fees += transaction.fee(pool)?;
        }

        let mint_fee = mint_transaction.outputs_sum();
        if mint_fee != total_fees + block_reward {
            Err(BlockError::MismatchedMintValue)?
        }

        Ok(())
    }
}

/// Any reference to a Hashable type is also Hashable
impl<'a, T: Hashable> Hashable for &'a T {
    fn hash(&self) -> Hash {
        (*self).hash()
    }
}

impl Hashable for Block {
    fn hash(&self) -> Hash {
        let block_ftb = build_block_flatbuffer(
            None,
            &BlockArgs {
                block_header: self.block_header.clone(),
                proof: self.proof.clone(),
                txns: &self.txns,
            },
        );
        calculate_sha256(&block_ftb).into()
    }
}

impl Hashable for CheckpointBeacon {
    fn hash(&self) -> Hash {
        let CheckpointBeacon {
            checkpoint,
            hash_prev_block,
        } = *self;
        let args = CheckpointBeaconArgs {
            checkpoint,
            hash_prev_block,
        };
        let beacon_ftb = build_checkpoint_beacon_flatbuffer(None, &args);
        calculate_sha256(&beacon_ftb).into()
    }
}

impl Hashable for Transaction {
    fn hash(&self) -> Hash {
        let transaction_ftb = build_transaction_flatbuffer(
            None,
            &TransactionArgs {
                version: self.version,
                inputs: &self.inputs,
                outputs: &self.outputs,
                signatures: &self.signatures,
            },
        );

        calculate_sha256(&transaction_ftb).into()
    }
}

/// Block header structure
#[derive(Debug, Eq, PartialEq, Clone, Serialize, Deserialize)]
pub struct BlockHeader {
    /// The block version number indicating the block validation rules
    pub version: u32,
    /// A checkpoint beacon for the epoch that this block is closing
    pub beacon: CheckpointBeacon,
    /// A 256-bit hash based on all of the transactions committed to this block
    pub hash_merkle_root: Hash,
}

/// Proof of leadership structure
#[derive(Debug, Eq, PartialEq, Clone, Serialize, Deserialize)]
pub struct LeadershipProof {
    /// An enveloped signature of the block header except the `proof` part
    pub block_sig: Option<Signature>,
    /// The alleged miner influence as of last checkpoint
    pub influence: u64,
}

/// Digital signatures structure (based on supported cryptosystems)
#[derive(Debug, Eq, PartialEq, Clone, Serialize, Deserialize)]
pub enum Signature {
    /// ECDSA over secp256k1
    Secp256k1(Secp256k1Signature),
}

/// ECDSA (over secp256k1) signature
#[derive(Debug, Eq, PartialEq, Clone, Serialize, Deserialize)]
pub struct Secp256k1Signature {
    /// The signature value R
    pub r: [u8; 32],
    /// The signature value S
    pub s: [u8; 32],
    /// 1 byte prefix of value S
    pub v: u8,
}

/// Hash
#[derive(Debug, Eq, PartialEq, Ord, PartialOrd, Copy, Clone, Serialize, Deserialize, Hash)]
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
                &h.iter()
                    .fold(String::new(), |acc, x| format!("{}{:02x}", acc, x)),
            )?,
        };

        Ok(())
    }
}

/// SHA-256 Hash
pub type SHA256 = [u8; 32];

/// Public Key Hash: slice of the digest of a public key (20 bytes)
pub type PublicKeyHash = [u8; 20];

/// Transaction data structure
#[derive(Debug, Default, Eq, PartialEq, Clone, Serialize, Deserialize)]
pub struct Transaction {
    pub version: u32,
    pub inputs: Vec<Input>,
    pub outputs: Vec<Output>,
    pub signatures: Vec<KeyedSignature>,
}

/// The error type for operations on a [`Transaction`](Transaction)
#[derive(Debug, Fail)]
pub enum TransactionError {
    /// The transaction creates value
    #[fail(display = "Transaction creates value (its fee is negative)")]
    NegativeFee,
    /// A transaction with the given hash wasn't found in a pool.
    #[fail(display = "A hash is missing in the pool {}", 0)]
    PoolMiss(Hash),
    /// An output with the given index wasn't found in a transaction.
    #[fail(
        display = "An output with index {} was not found in transaction {}",
        1, 0
    )]
    OutputNotFound(Hash, usize),
}

impl Transaction {
    /// Creates a new transaction from inputs and outputs.
    // TODO: Transaction::new is missing the signatures which depend
    // on a sign key that needs to be implemented first in
    // witnet_crypto.
    pub fn new(version: u32, inputs: Vec<Input>, outputs: Vec<Output>, _sign_key: ()) -> Self {
        let signatures = inputs
            .iter()
            .map(|_input| KeyedSignature {
                signature: Signature::Secp256k1(Secp256k1Signature {
                    r: [0; 32],
                    s: [0; 32],
                    v: 0,
                }),
                public_key: [0; 32],
            })
            .collect();

        Transaction {
            version,
            inputs,
            outputs,
            signatures,
        }
    }

    /// Return the value of the output with index `index`.
    pub fn get_output_value(&self, index: usize) -> Option<u64> {
        self.outputs.get(index).map(Output::value)
    }

    /// Calculate the sum of the values of the outputs pointed by the
    /// inputs of a transaction. If an input pointed-output is not
    /// found in `pool`, then an error is returned instead indicating
    /// it.
    pub fn inputs_sum(&self, pool: &UnspentOutputsPool) -> Result<u64, TransactionError> {
        let mut total_value = 0;

        for input in &self.inputs {
            /*
            let OutputPointer {
                transaction_id,
                output_index,
            } = input.output_pointer();
            let index = output_index as usize;
            let pointed_transaction = pool
                .get(&transaction_id)
                .ok_or_else(|| TransactionError::PoolMiss(transaction_id))?;
            let pointed_value = pointed_transaction
                .get_output_value(index)
                .ok_or_else(|| TransactionError::OutputNotFound(transaction_id, index))?;
                */
            let pointed_value = pool.get(&input.output_pointer()).ok_or_else(
                || TransactionError::OutputNotFound(input.output_pointer().transaction_id, input.output_pointer().output_index as usize)
            )?.value();
            total_value += pointed_value;
        }

        Ok(total_value)
    }

    /// Calculate the sum of the values of the outputs of a transaction.
    pub fn outputs_sum(&self) -> u64 {
        self.outputs.iter().map(Output::value).sum()
    }

    /// Returns the size a transaction will have on the wire in bytes
    pub fn size(&self) -> u32 {
        build_transaction_flatbuffer(
            None,
            &TransactionArgs {
                version: self.version,
                inputs: &self.inputs,
                outputs: &self.outputs,
                signatures: &self.signatures,
            },
        )
        .len() as u32
    }

    /// Returns `true` if the transaction classifies as a _mint
    /// transaction_.  A mint transaction is one that has no inputs,
    /// only outputs, thus, is allowed to create new wits.
    pub fn is_mint(&self) -> bool {
        self.inputs.is_empty()
    }

    /// Returns the fee of a transaction.
    ///
    /// The fee is the difference between the outputs and the inputs
    /// of the transaction. The pool parameter is used to find the
    /// outputs pointed by the inputs and that contain the actual
    /// their value.
    pub fn fee(&self, pool: &UnspentOutputsPool) -> Result<u64, TransactionError> {
        let in_value = self.inputs_sum(pool)?;
        let out_value = self.outputs_sum();

        if self.is_mint() {
            Ok(out_value)
        } else if out_value > in_value {
            Err(TransactionError::NegativeFee)?
        } else {
            Ok(in_value - out_value)
        }
    }
}

impl AsRef<Transaction> for Transaction {
    fn as_ref(&self) -> &Self {
        self
    }
}

/// Input data structure
#[derive(Debug, Eq, PartialEq, Clone, Serialize, Deserialize)]
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
#[derive(Debug, Eq, PartialEq, Clone, Serialize, Deserialize)]
pub struct ValueTransferInput {
    pub transaction_id: Hash,
    pub output_index: u32,
}

/// Commit input transaction data structure
#[derive(Debug, Eq, PartialEq, Clone, Serialize, Deserialize)]
pub struct CommitInput {
    pub transaction_id: Hash,
    pub output_index: u32,
    pub reveal: Vec<u8>,
    pub nonce: u64,
}

/// Commit input transaction data structure
#[derive(Debug, Eq, PartialEq, Clone, Serialize, Deserialize)]
pub struct DataRequestInput {
    pub transaction_id: Hash,
    pub output_index: u32,
    pub poe: [u8; 32],
}

/// Reveal input transaction data structure
#[derive(Debug, Eq, PartialEq, Clone, Serialize, Deserialize)]
pub struct RevealInput {
    pub transaction_id: Hash,
    pub output_index: u32,
}

/// Output data structure
#[derive(Debug, Eq, PartialEq, Clone, Serialize, Deserialize)]
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
#[derive(Debug, Eq, PartialEq, Clone, Serialize, Deserialize)]
pub struct ValueTransferOutput {
    pub pkh: PublicKeyHash,
    pub value: u64,
}

/// Data request output transaction data structure
#[derive(Debug, Eq, PartialEq, Clone, Serialize, Deserialize)]
pub struct DataRequestOutput {
    pub pkh: PublicKeyHash,
    pub data_request: RADRequest,
    pub value: u64,
    pub witnesses: u8,
    pub backup_witnesses: u8,
    pub commit_fee: u64,
    pub reveal_fee: u64,
    pub tally_fee: u64,
    pub time_lock: u64,
}

/// Commit output transaction data structure
#[derive(Debug, Eq, PartialEq, Clone, Serialize, Deserialize)]
pub struct CommitOutput {
    pub commitment: Hash,
    pub value: u64,
}

/// Reveal output transaction data structure
#[derive(Debug, Eq, PartialEq, Clone, Serialize, Deserialize)]
pub struct RevealOutput {
    pub reveal: Vec<u8>,
    pub pkh: PublicKeyHash,
    pub value: u64,
}

#[derive(Debug, Eq, PartialEq, Clone, Serialize, Deserialize)]
pub struct TallyOutput {
    pub result: Vec<u8>,
    pub pkh: PublicKeyHash,
    pub value: u64,
}

/// Keyed signature data structure
#[derive(Debug, Eq, PartialEq, Clone, Serialize, Deserialize)]
pub struct KeyedSignature {
    pub signature: Signature,
    pub public_key: [u8; 32],
}

#[derive(Debug, Eq, PartialEq, Clone, Serialize, Deserialize)]
pub enum RADType {
    #[serde(rename = "HTTP-GET")]
    HttpGet,
}

/// RAD request data structure
#[derive(Debug, Eq, PartialEq, Clone, Serialize, Deserialize)]
pub struct RADRequest {
    pub not_before: u64,
    pub retrieve: Vec<RADRetrieve>,
    pub aggregate: RADAggregate,
    pub consensus: RADConsensus,
    pub deliver: Vec<RADDeliver>,
}

#[derive(Debug, Eq, PartialEq, Clone, Serialize, Deserialize)]
pub struct RADRetrieve {
    pub kind: RADType,
    pub url: String,
    pub script: Vec<u8>,
}

#[derive(Debug, Eq, PartialEq, Clone, Serialize, Deserialize)]
pub struct RADAggregate {
    pub script: Vec<u8>,
}

#[derive(Debug, Eq, PartialEq, Clone, Serialize, Deserialize)]
pub struct RADConsensus {
    pub script: Vec<u8>,
}

#[derive(Debug, Eq, PartialEq, Clone, Serialize, Deserialize)]
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
    /// # use witnet_data_structures::chain::{TransactionsPool, Transaction, Hash};
    /// let mut pool = TransactionsPool::new();
    ///
    /// let transaction = Transaction {
    ///     inputs: [].to_vec(),
    ///     signatures: [].to_vec(),
    ///     outputs: [].to_vec(),
    ///     version: 0
    /// };
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
    /// # use witnet_data_structures::chain::{TransactionsPool, Transaction, Hash};
    /// let mut pool = TransactionsPool::new();
    /// let hash = Hash::SHA256([0 as u8; 32]);
    /// let transaction = Transaction {
    ///     inputs: [].to_vec(),
    ///     signatures: [].to_vec(),
    ///     outputs: [].to_vec(),
    ///     version: 0
    /// };
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
    /// # use witnet_data_structures::chain::{TransactionsPool, Transaction, Hash};
    /// let mut pool = TransactionsPool::new();
    /// let hash = Hash::SHA256([0 as u8; 32]);
    /// let transaction = Transaction {
    ///     inputs: [].to_vec(),
    ///     signatures: [].to_vec(),
    ///     outputs: [].to_vec(),
    ///     version: 0
    /// };
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
    /// # use witnet_data_structures::chain::{TransactionsPool, Transaction, Hash};
    /// let mut pool = TransactionsPool::new();
    /// let transaction = Transaction {
    ///     inputs: [].to_vec(),
    ///     signatures: [].to_vec(),
    ///     outputs: [].to_vec(),
    ///     version: 0
    /// };
    /// pool.insert(Hash::SHA256([0 as u8; 32]), transaction);
    ///
    /// assert!(!pool.is_empty());
    /// ```
    pub fn insert(&mut self, key: Hash, transaction: Transaction) {
        let weight = 0; // TODO: weight = transaction-fee / transaction-weight
        self.transactions.insert(key, (weight, transaction));
        self.sorted_index.insert((weight, key));
    }

    /// Returns a reference to the value corresponding to the key.
    ///
    /// Examples:
    ///
    /// ```
    /// # use witnet_data_structures::chain::{TransactionsPool, Transaction, Hash};
    /// let mut pool = TransactionsPool::new();
    /// let hash = Hash::SHA256([0 as u8; 32]);
    /// let transaction = Transaction {
    ///     inputs: [].to_vec(),
    ///     signatures: [].to_vec(),
    ///     outputs: [].to_vec(),
    ///     version: 0
    /// };
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

    /// An iterator visiting all the transactions in the pool in
    /// descending-fee order, that is, transactions with bigger fees
    /// come first.
    ///
    /// Examples:
    ///
    /// ```
    /// # use witnet_data_structures::chain::{TransactionsPool, Transaction, Hash};
    /// let mut pool = TransactionsPool::new();
    ///
    /// let transaction = Transaction {
    ///     inputs: [].to_vec(),
    ///     signatures: [].to_vec(),
    ///     outputs: [].to_vec(),
    ///     version: 0
    /// };
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

    /// Retains only the elements specified by the predicate.
    ///
    /// In other words, remove all transactions such that
    /// `f(&Hash, &Transaction)` returns `false`.
    ///
    /// # Examples
    ///
    /// ```
    /// # use witnet_data_structures::chain::{TransactionsPool, Transaction, Hash};
    ///
    /// let mut pool = TransactionsPool::new();
    ///
    /// let transaction = Transaction {
    ///     inputs: [].to_vec(),
    ///     signatures: [].to_vec(),
    ///     outputs: [].to_vec(),
    ///     version: 0
    /// };
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
#[derive(Debug, Default, Hash, Clone, Eq, PartialEq)]
pub struct OutputPointer {
    pub transaction_id: Hash,
    pub output_index: u32,
}

#[derive(Debug)]
pub enum OutputPointerParseError {
    InvalidHashLength,
    MissingColon,
    ParseIntError(ParseIntError),
    ParseHex(ParseIntError),
}

impl fmt::Display for OutputPointer {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str(&format!("{}:{}", &self.transaction_id, &self.output_index))
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

        // TODO: handle unwrap
        Ok(Self::from_str(&output_pointer_as_str).unwrap())
    }
}

/// Inventory entry data structure
#[derive(Debug, Eq, PartialEq, Clone, Serialize, Deserialize)]
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

/// Pool of active data requests
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct ActiveDataRequestPool {
    /// Currently active data requests for which this node has announced commitments
    /// Key: Data Request Pointer, Value: Reveal Transaction
    pub waiting_for_reveal: HashMap<OutputPointer, Transaction>,
    /// Indexes the output pointers of every active data request by epoch (only for mining purposes)
    pub data_requests_by_epoch: BTreeMap<Epoch, HashSet<OutputPointer>>,
    /// Indexes every active data request by its original output pointer
    pub data_request_pool: HashMap<OutputPointer, DataRequestState>,
    /// Lists data request reports that need to be persisted into storage
    pub to_be_stored: Vec<(OutputPointer, DataRequestReport)>,
    /// Maps `commit_pointer` and `reveal_pointer` to `data_request_pointer`
    pub dr_pointer_cache: HashMap<OutputPointer, OutputPointer>,
}

pub type UnspentOutputsPool = HashMap<OutputPointer, Output>;

pub type Blockchain = BTreeMap<Epoch, HashSet<Hash>>;

/// Blockchain state (valid at a certain epoch)
#[derive(Debug, Default, Serialize, Deserialize, PartialEq, Clone)]
pub struct ChainState {
    /// Blockchain information data structure
    pub chain_info: Option<ChainInfo>,
    /// Unspent Outputs Pool
    pub unspent_outputs_pool: UnspentOutputsPool,
    /// Collection of state structures for active data requests
    pub data_request_pool: ActiveDataRequestPool,
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
            for input in &transaction.inputs {
                // Obtain the OuputPointer of each input and remove it from the utxo_set
                let output_pointer = input.output_pointer();

                unspent_outputs.remove(&output_pointer);
            }

            for (index, output) in transaction.outputs.iter().enumerate() {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_block_hashable_trait() {
        let block_header = BlockHeader {
            version: 0,
            beacon: CheckpointBeacon {
                checkpoint: 0,
                hash_prev_block: Hash::SHA256([0; 32]),
            },
            hash_merkle_root: Hash::SHA256([0; 32]),
        };
        let signature = Signature::Secp256k1(Secp256k1Signature {
            r: [0; 32],
            s: [0; 32],
            v: 0,
        });
        let proof = LeadershipProof {
            block_sig: Some(signature.clone()),
            influence: 0,
        };
        let keyed_signatures = vec![KeyedSignature {
            public_key: [0; 32],
            signature,
        }];
        let commit_input = Input::Commit(CommitInput {
            nonce: 0,
            output_index: 0,
            reveal: [0; 32].to_vec(),
            transaction_id: Hash::SHA256([0; 32]),
        });
        let reveal_input = Input::Reveal(RevealInput {
            output_index: 0,
            transaction_id: Hash::SHA256([0; 32]),
        });
        let data_request_input = Input::DataRequest(DataRequestInput {
            output_index: 0,
            poe: [0; 32],
            transaction_id: Hash::SHA256([0; 32]),
        });
        let value_transfer_output = Output::ValueTransfer(ValueTransferOutput {
            pkh: [0; 20],
            value: 0,
        });

        let rad_aggregate = RADAggregate { script: vec![0] };

        let rad_retrieve_1 = RADRetrieve {
            kind: RADType::HttpGet,
            url: "https://openweathermap.org/data/2.5/weather?id=2950159&appid=b6907d289e10d714a6e88b30761fae22".to_string(),
            script: vec![0],
        };

        let rad_retrieve_2 = RADRetrieve {
            kind: RADType::HttpGet,
            url: "https://openweathermap.org/data/2.5/weather?id=2950159&appid=b6907d289e10d714a6e88b30761fae22".to_string(),
            script: vec![0],
        };

        let rad_consensus = RADConsensus { script: vec![0] };

        let rad_deliver_1 = RADDeliver {
            kind: RADType::HttpGet,
            url: "https://hooks.zapier.com/hooks/catch/3860543/l2awcd/".to_string(),
        };

        let rad_deliver_2 = RADDeliver {
            kind: RADType::HttpGet,
            url: "https://hooks.zapier.com/hooks/catch/3860543/l1awcw/".to_string(),
        };

        let rad_request = RADRequest {
            aggregate: rad_aggregate,
            not_before: 0,
            retrieve: vec![rad_retrieve_1, rad_retrieve_2],
            consensus: rad_consensus,
            deliver: vec![rad_deliver_1, rad_deliver_2],
        };

        let data_request_output = Output::DataRequest(DataRequestOutput {
            backup_witnesses: 0,
            commit_fee: 0,
            data_request: rad_request,
            pkh: [0; 20],
            reveal_fee: 0,
            tally_fee: 0,
            time_lock: 0,
            value: 0,
            witnesses: 0,
        });
        let commit_output = Output::Commit(CommitOutput {
            commitment: Hash::SHA256([0; 32]),
            value: 0,
        });
        let reveal_output = Output::Reveal(RevealOutput {
            pkh: [0; 20],
            reveal: [0; 32].to_vec(),
            value: 0,
        });
        let consensus_output = Output::Tally(TallyOutput {
            pkh: [0; 20],
            result: vec![0],
            value: 0,
        });
        let inputs = vec![commit_input, data_request_input, reveal_input];
        let outputs = vec![
            value_transfer_output,
            data_request_output,
            commit_output,
            reveal_output,
            consensus_output,
        ];
        let txns: Vec<Transaction> = vec![Transaction {
            inputs,
            signatures: keyed_signatures,
            outputs,
            version: 0,
        }];
        let block = Block {
            block_header,
            proof,
            txns,
        };
        let expected = Hash::SHA256([
            204, 111, 204, 123, 50, 100, 176, 227, 102, 35, 195, 223, 178, 106, 185, 156, 160, 24,
            18, 210, 236, 116, 217, 170, 103, 95, 92, 236, 208, 52, 134, 63,
        ]);
        assert_eq!(block.hash(), expected);
    }

    #[test]
    fn test_transaction_hashable_trait() {
        let signature = Signature::Secp256k1(Secp256k1Signature {
            r: [0; 32],
            s: [0; 32],
            v: 0,
        });
        let signatures = vec![KeyedSignature {
            public_key: [0; 32],
            signature,
        }];
        let commit_input = Input::Commit(CommitInput {
            nonce: 0,
            output_index: 0,
            reveal: [0; 32].to_vec(),
            transaction_id: Hash::SHA256([0; 32]),
        });
        let reveal_input = Input::Reveal(RevealInput {
            output_index: 0,
            transaction_id: Hash::SHA256([0; 32]),
        });
        let data_request_input = Input::DataRequest(DataRequestInput {
            output_index: 0,
            poe: [0; 32],
            transaction_id: Hash::SHA256([0; 32]),
        });
        let value_transfer_output = Output::ValueTransfer(ValueTransferOutput {
            pkh: [0; 20],
            value: 0,
        });

        let rad_aggregate = RADAggregate { script: vec![0] };

        let rad_retrieve_1 = RADRetrieve {
            kind: RADType::HttpGet,
            url: "https://openweathermap.org/data/2.5/weather?id=2950159&appid=b6907d289e10d714a6e88b30761fae22".to_string(),
            script: vec![0],
        };

        let rad_retrieve_2 = RADRetrieve {
            kind: RADType::HttpGet,
            url: "https://openweathermap.org/data/2.5/weather?id=2950159&appid=b6907d289e10d714a6e88b30761fae22".to_string(),
            script: vec![0],
        };

        let rad_consensus = RADConsensus { script: vec![0] };
        let rad_deliver_1 = RADDeliver {
            kind: RADType::HttpGet,
            url: "https://hooks.zapier.com/hooks/catch/3860543/l2awcd/".to_string(),
        };

        let rad_deliver_2 = RADDeliver {
            kind: RADType::HttpGet,
            url: "https://hooks.zapier.com/hooks/catch/3860543/l1awcw/".to_string(),
        };

        let rad_request = RADRequest {
            aggregate: rad_aggregate,
            not_before: 0,
            retrieve: vec![rad_retrieve_1, rad_retrieve_2],
            consensus: rad_consensus,
            deliver: vec![rad_deliver_1, rad_deliver_2],
        };
        let data_request_output = Output::DataRequest(DataRequestOutput {
            backup_witnesses: 0,
            commit_fee: 0,
            data_request: rad_request,
            pkh: [0; 20],
            reveal_fee: 0,
            tally_fee: 0,
            time_lock: 0,
            value: 0,
            witnesses: 0,
        });
        let commit_output = Output::Commit(CommitOutput {
            commitment: Hash::SHA256([0; 32]),
            value: 0,
        });
        let reveal_output = Output::Reveal(RevealOutput {
            pkh: [0; 20],
            reveal: [0; 32].to_vec(),
            value: 0,
        });
        let consensus_output = Output::Tally(TallyOutput {
            pkh: [0; 20],
            result: [0; 32].to_vec(),
            value: 0,
        });
        let inputs = vec![commit_input, data_request_input, reveal_input];
        let outputs = vec![
            value_transfer_output,
            data_request_output,
            commit_output,
            reveal_output,
            consensus_output,
        ];
        let transaction: Transaction = Transaction {
            inputs,
            outputs,
            signatures,
            version: 0,
        };
        let expected = Hash::SHA256([
            10, 241, 147, 199, 165, 174, 93, 237, 233, 213, 202, 27, 217, 126, 244, 196, 189, 74,
            84, 243, 4, 214, 2, 34, 22, 0, 118, 115, 137, 32, 203, 237,
        ]);
        assert_eq!(transaction.hash(), expected);
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
}
