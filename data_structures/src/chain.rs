use super::serializers::{build_block_flatbuffer, BlockArgs};
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
// TODO Implement Hashable for Transaction in Issue #224
impl Hashable for Transaction {
    fn hash(&self) -> Hash {
        Hash::SHA256([0; 32])
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
#[derive(Debug, Eq, PartialEq, Copy, Clone, Serialize, Deserialize, Hash)]
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
// FIXME(#99): define Transaction as defined in issue
#[derive(Copy, Debug, Eq, PartialEq, Clone, Serialize, Deserialize)]
pub struct Transaction;

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
        let txns: Vec<Transaction> = vec![Transaction];
        let block = Block {
            block_header,
            proof,
            txns,
        };
        let expected = Hash::SHA256([
            222, 143, 136, 148, 115, 96, 203, 166, 243, 118, 165, 93, 124, 25, 218, 124, 196, 150,
            149, 96, 254, 156, 243, 114, 246, 128, 81, 43, 55, 58, 26, 241,
        ]);
        assert_eq!(block.hash(), expected);
    }
}
