use super::serializers::{
    build_block_flatbuffer, build_transaction_flatbuffer, BlockArgs, CheckpointBeaconArgs,
    TransactionArgs,
};
use std::collections::{BTreeSet, HashMap};
use witnet_crypto::hash::{calculate_sha256, Sha256};

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
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
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
}

/// Checkpoint beacon structure
#[derive(Debug, Eq, PartialEq, Copy, Clone, Serialize, Deserialize)]
pub struct CheckpointBeacon {
    /// The serial number for an epoch
    pub checkpoint: Epoch,
    /// The 256-bit hash of the previous block header
    pub hash_prev_block: Hash,
}

impl Default for CheckpointBeacon {
    fn default() -> CheckpointBeacon {
        CheckpointBeacon {
            checkpoint: 0,
            hash_prev_block: Hash::SHA256([0; 32]),
        }
    }
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

impl Hashable for Block {
    fn hash(&self) -> Hash {
        let block_ftb = build_block_flatbuffer(
            None,
            &BlockArgs {
                block_header: self.block_header,
                proof: self.proof,
                txns: self.txns.clone(),
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
        let beacon_ftb = super::serializers::build_checkpoint_beacon_flatbuffer(None, &args);
        calculate_sha256(&beacon_ftb).into()
    }
}

impl Hashable for Transaction {
    fn hash(&self) -> Hash {
        let transaction_ftb = build_transaction_flatbuffer(
            None,
            &TransactionArgs {
                version: self.version,
                inputs: self.inputs.clone(),
                outputs: self.outputs.clone(),
                signatures: self.signatures.clone(),
            },
        );

        calculate_sha256(&transaction_ftb).into()
    }
}

/// Block header structure
#[derive(Copy, Debug, Eq, PartialEq, Clone, Serialize, Deserialize)]
pub struct BlockHeader {
    /// The block version number indicating the block validation rules
    pub version: u32,
    /// A checkpoint beacon for the epoch that this block is closing
    pub beacon: CheckpointBeacon,
    /// A 256-bit hash based on all of the transactions committed to this block
    pub hash_merkle_root: Hash,
}

/// Proof of leadership structure
#[derive(Copy, Debug, Eq, PartialEq, Clone, Serialize, Deserialize)]
pub struct LeadershipProof {
    /// An enveloped signature of the block header except the `proof` part
    pub block_sig: Option<Signature>,
    /// The alleged miner influence as of last checkpoint
    pub influence: u64,
}

/// Digital signatures structure (based on supported cryptosystems)
#[derive(Copy, Debug, Eq, PartialEq, Clone, Serialize, Deserialize)]
pub enum Signature {
    /// ECDSA over secp256k1
    Secp256k1(Secp256k1Signature),
}

/// ECDSA (over secp256k1) signature
#[derive(Copy, Debug, Eq, PartialEq, Clone, Serialize, Deserialize)]
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

/// Conversion between witnet_crypto::Sha256 and witnet_data_structures::Hash
impl From<Sha256> for Hash {
    fn from(x: Sha256) -> Self {
        Hash::SHA256(x.0)
    }
}

/// SHA-256 Hash
pub type SHA256 = [u8; 32];

/// Transaction data structure
#[derive(Debug, Eq, PartialEq, Clone, Serialize, Deserialize)]
pub struct Transaction {
    pub version: u32,
    pub inputs: Vec<Input>,
    pub outputs: Vec<Output>,
    pub signatures: Vec<KeyedSignature>,
}

/// Input data structure
#[derive(Copy, Debug, Eq, PartialEq, Clone, Serialize, Deserialize)]
pub enum Input {
    ValueTransfer(ValueTransferInput),
    Commit(CommitInput),
    Reveal(RevealInput),
    Tally(TallyInput),
}
/// Value transfer input transaction data structure
#[derive(Copy, Debug, Eq, PartialEq, Clone, Serialize, Deserialize)]
pub struct ValueTransferInput {
    pub transaction_id: [u8; 32],
    pub output_index: u32,
}

/// Commit input transaction data structure
#[derive(Copy, Debug, Eq, PartialEq, Clone, Serialize, Deserialize)]
pub struct CommitInput {
    pub transaction_id: [u8; 32],
    pub output_index: u32,
    pub poe: [u8; 32],
}

/// Reveal input transaction data structure
#[derive(Copy, Debug, Eq, PartialEq, Clone, Serialize, Deserialize)]
pub struct RevealInput {
    pub transaction_id: [u8; 32],
    pub output_index: u32,
    pub reveal: [u8; 32],
    pub nonce: u64,
}

/// Tally input transaction data structure
#[derive(Copy, Debug, Eq, PartialEq, Clone, Serialize, Deserialize)]
pub struct TallyInput {
    pub transaction_id: [u8; 32],
    pub output_index: u32,
}

/// Value transfer output transaction data structure
#[derive(Copy, Debug, Eq, PartialEq, Clone, Serialize, Deserialize)]
pub struct ValueTransferOutput {
    pub pkh: Hash,
    pub value: u64,
}

/// Data request output transaction data structure
#[derive(Copy, Debug, Eq, PartialEq, Clone, Serialize, Deserialize)]
pub struct DataRequestOutput {
    pub data_request: [u8; 32],
    pub value: u64,
    pub witnesses: u8,
    pub backup_witnesses: u8,
    pub commit_fee: u64,
    pub reveal_fee: u64,
    pub tally_fee: u64,
    pub time_lock: u64,
}

/// Commit output transaction data structure
#[derive(Copy, Debug, Eq, PartialEq, Clone, Serialize, Deserialize)]
pub struct CommitOutput {
    pub commitment: Hash,
    pub value: u64,
}

/// Reveal output transaction data structure
#[derive(Copy, Debug, Eq, PartialEq, Clone, Serialize, Deserialize)]
pub struct RevealOutput {
    pub reveal: [u8; 32],
    pub pkh: Hash,
    pub value: u64,
}

#[derive(Copy, Debug, Eq, PartialEq, Clone, Serialize, Deserialize)]
pub struct ConsensusOutput {
    pub result: [u8; 32],
    pub pkh: Hash,
    pub value: u64,
}

/// Output data structure
#[derive(Copy, Debug, Eq, PartialEq, Clone, Serialize, Deserialize)]
pub enum Output {
    ValueTransfer(ValueTransferOutput),
    DataRequest(DataRequestOutput),
    Commit(CommitOutput),
    Reveal(RevealOutput),
    Consensus(ConsensusOutput),
}

/// Keyed signature data structure
#[derive(Copy, Debug, Eq, PartialEq, Clone, Serialize, Deserialize)]
pub struct KeyedSignature {
    pub signature: Signature,
    pub public_key: [u8; 32],
}

type WeightedHash = (u64, Hash);
type WeightedTransaction = (u64, Transaction);

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
            block_sig: Some(signature),
            influence: 0,
        };
        let keyed_signatures = vec![KeyedSignature {
            public_key: [0; 32],
            signature,
        }];
        let value_transfer_input = Input::ValueTransfer(ValueTransferInput {
            output_index: 0,
            transaction_id: [0; 32],
        });
        let reveal_input = Input::Reveal(RevealInput {
            nonce: 0,
            output_index: 0,
            reveal: [0; 32],
            transaction_id: [0; 32],
        });
        let tally_input = Input::Tally(TallyInput {
            output_index: 0,
            transaction_id: [0; 32],
        });
        let commit_input = Input::Commit(CommitInput {
            output_index: 0,
            poe: [0; 32],
            transaction_id: [0; 32],
        });
        let value_transfer_output = Output::ValueTransfer(ValueTransferOutput {
            pkh: Hash::SHA256([0; 32]),
            value: 0,
        });
        let data_request_output = Output::DataRequest(DataRequestOutput {
            backup_witnesses: 0,
            commit_fee: 0,
            data_request: [0; 32],
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
            pkh: Hash::SHA256([0; 32]),
            reveal: [0; 32],
            value: 0,
        });
        let consensus_output = Output::Consensus(ConsensusOutput {
            pkh: Hash::SHA256([0; 32]),
            result: [0; 32],
            value: 0,
        });
        let inputs = vec![
            value_transfer_input,
            reveal_input,
            tally_input,
            commit_input,
        ];
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
            75, 60, 44, 31, 102, 63, 104, 156, 20, 88, 228, 8, 138, 160, 207, 99, 82, 141, 187, 19,
            52, 211, 212, 109, 108, 205, 149, 166, 216, 242, 60, 239,
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
        let value_transfer_input = Input::ValueTransfer(ValueTransferInput {
            output_index: 0,
            transaction_id: [0; 32],
        });
        let reveal_input = Input::Reveal(RevealInput {
            nonce: 0,
            output_index: 0,
            reveal: [0; 32],
            transaction_id: [0; 32],
        });
        let tally_input = Input::Tally(TallyInput {
            output_index: 0,
            transaction_id: [0; 32],
        });
        let commit_input = Input::Commit(CommitInput {
            output_index: 0,
            poe: [0; 32],
            transaction_id: [0; 32],
        });
        let value_transfer_output = Output::ValueTransfer(ValueTransferOutput {
            pkh: Hash::SHA256([0; 32]),
            value: 0,
        });
        let data_request_output = Output::DataRequest(DataRequestOutput {
            backup_witnesses: 0,
            commit_fee: 0,
            data_request: [0; 32],
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
            pkh: Hash::SHA256([0; 32]),
            reveal: [0; 32],
            value: 0,
        });
        let consensus_output = Output::Consensus(ConsensusOutput {
            pkh: Hash::SHA256([0; 32]),
            result: [0; 32],
            value: 0,
        });
        let inputs = vec![
            value_transfer_input,
            reveal_input,
            tally_input,
            commit_input,
        ];
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
            210, 97, 146, 25, 56, 31, 47, 86, 212, 73, 115, 112, 79, 233, 239, 214, 30, 58, 137,
            174, 191, 146, 196, 222, 24, 248, 211, 55, 150, 243, 87, 228,
        ]);
        assert_eq!(transaction.hash(), expected);
    }
}
