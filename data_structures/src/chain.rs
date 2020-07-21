use bech32::{FromBase32, ToBase32};
use bls_signatures_rs::{bn256, bn256::Bn256, MultiSignature};
use failure::Fail;
use itertools::Itertools;
use ordered_float::OrderedFloat;
use partial_struct::PartialStruct;
use secp256k1::{
    PublicKey as Secp256k1_PublicKey, SecretKey as Secp256k1_SecretKey,
    Signature as Secp256k1_Signature,
};
use serde::{Deserialize, Serialize};
use std::{
    cell::RefCell,
    cmp::Ordering,
    collections::{BTreeMap, BTreeSet, HashMap, HashSet},
    convert::{TryFrom, TryInto},
    fmt,
    ops::{AddAssign, SubAssign},
    str::FromStr,
};
use witnet_crypto::{
    hash::{calculate_sha256, Sha256},
    key::ExtendedSK,
    merkle::merkle_tree_root as crypto_merkle_tree_root,
};
use witnet_protected::Protected;
use witnet_reputation::{ActiveReputationSet, TotalReputationSet};
use witnet_util::timestamp::get_timestamp;

use crate::{
    chain::Signature::Secp256k1,
    data_request::DataRequestPool,
    error::{
        DataRequestError, EpochCalculationError, OutputPointerParseError, Secp256k1ConversionError,
        TransactionError,
    },
    get_environment,
    proto::{schema::witnet, ProtobufConvert},
    superblock::SuperBlockState,
    transaction::{
        CommitTransaction, DRTransaction, DRTransactionBody, MintTransaction, RevealTransaction,
        TallyTransaction, Transaction, VTTransaction,
    },
    vrf::{BlockEligibilityClaim, DataRequestEligibilityClaim},
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

    /// Checkpoint hash of the highest superblock in the blockchain
    pub highest_superblock_checkpoint: CheckpointBeacon,

    /// Checkpoint and VRF hash of the highest block in the blockchain
    pub highest_vrf_output: CheckpointVRF,
}

/// Possible values for the "environment" configuration param.
// The variants are explicitly tagged so that bincode serialization does not break
#[derive(Serialize, Deserialize, Copy, Clone, Debug, PartialEq)]
pub enum Environment {
    /// "mainnet" environment
    #[serde(rename = "mainnet")]
    Mainnet = 0,
    /// "testnet" environment
    #[serde(rename = "testnet")]
    Testnet = 1,
    /// "development" environment
    #[serde(rename = "development")]
    Development = 2,
}

impl Default for Environment {
    fn default() -> Environment {
        Environment::Testnet
    }
}

impl fmt::Display for Environment {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let s = match self {
            Environment::Development => "development",
            Environment::Mainnet => "mainnet",
            Environment::Testnet => "testnet",
        };

        f.write_str(s)
    }
}

impl Environment {
    /// Returns the Bech32 prefix used in this environment: "wit" in mainnet and "twit" elsewhere
    pub fn bech32_prefix(&self) -> &str {
        match self {
            Environment::Mainnet => "wit",
            _ => "twit",
        }
    }

    /// Returns true if the consensus constants can be overriden in this environment.
    /// This is only allowed in a development environment.
    pub fn can_override_consensus_constants(self) -> bool {
        match self {
            Environment::Development => true,
            _ => false,
        }
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

    /// Auxiliary bootstrap block hash value
    pub bootstrap_hash: Hash,

    /// Genesis block hash value
    pub genesis_hash: Hash,

    /// Maximum weight a block can have, this affects the number of
    /// transactions a block can contain: there will be as many
    /// transactions as the sum of _their_ weights is less than, or
    /// equal to, this maximum block weight parameter.
    ///
    /// Maximum aggregated weight of all the value transfer transactions in one block
    pub max_vt_weight: u32,
    /// Maximum aggregated weight of all the data requests transactions in one block
    pub max_dr_weight: u32,

    /// An identity is considered active if it participated in the witnessing protocol at least once in the last `activity_period` epochs
    pub activity_period: u32,

    /// Reputation will expire after N witnessing acts
    pub reputation_expire_alpha_diff: u32,

    /// Reputation issuance
    pub reputation_issuance: u32,

    /// When to stop issuing new reputation
    pub reputation_issuance_stop: u32,

    /// Penalization factor: fraction of reputation lost by liars for out of consensus claims
    // FIXME(#172): Use fixed point arithmetic
    pub reputation_penalization_factor: f64,

    /// Backup factor for mining: valid VRFs under this factor will result in broadcasting a block
    pub mining_backup_factor: u32,

    /// Replication factor for mining: valid VRFs under this factor will have priority
    pub mining_replication_factor: u32,

    /// Minimum value in nanowits for a collateral value
    pub collateral_minimum: u64,

    /// Minimum input age of an UTXO for being a valid collateral
    pub collateral_age: u32,

    /// Build a superblock every `superblock_period` epochs
    pub superblock_period: u16,

    /// Extra rounds for commitments and reveals
    pub extra_rounds: u16,

    /// Initial difficulty
    /// (That difficulty is enforced by code so it means that it also ignore the backup factor)
    pub initial_difficulty: u32,

    /// Number of epochs with the initial difficulty active
    /// (This number represent the last epoch where the initial difficulty is active)
    pub epochs_with_initial_difficulty: u32,

    /// Superblock signing committee for the first superblocks
    pub bootstrapping_committee: Vec<String>,

    /// Size of the superblock signing committee
    pub superblock_signing_committee_size: u32,
}

impl ConsensusConstants {
    pub fn get_magic(&self) -> u16 {
        let magic = calculate_sha256(&self.to_pb_bytes().unwrap());
        u16::from(magic.0[0]) << 8 | (u16::from(magic.0[1]))
    }
}

#[derive(Debug, Fail)]
/// The various reasons creating a genesis block can fail
pub enum GenesisBlockInfoError {
    /// Failed to read file
    #[fail(display = "Failed to read file `{}`: {}", path, inner)]
    Read {
        /// Path of the `genesis_block.json` file
        path: String,
        /// Inner io error
        inner: std::io::Error,
    },
    /// Failed to deserialize
    #[fail(display = "Failed to deserialize `GenesisBlockInfo`: {}", _0)]
    Deserialize(serde_json::Error),
    /// The hash of the created genesis block does not match the hash set in the configuration
    #[fail(
        display = "Genesis block hash mismatch\nExpected: {}\nFound:    {}",
        expected, created
    )]
    HashMismatch {
        /// Hash of the genesis block as created by reading the `genesis_block.json` file
        created: Hash,
        /// Hash of the genesis block specified in the configuration
        expected: Hash,
    },
}

/// Information needed to create the genesis block.
///
/// To prevent deserialization issues, the JSON file only contains string types: all integers
/// must be surrounded by quotes.
///
/// This is the format of `genesis_block.json`:
///
/// ```ignore
/// {
///     "alloc":[
///         [
///             {
///                 "address": "twit1adgt8t2h3xnu358f76zxlph0urf2ev7cd78ggc",
///                 "value": "500000000000",
///                 "timelock": "0"
///             },
///         ]
///     ]
/// }
/// ```
///
/// The `alloc` field has two levels of arrays: the outer array is the list of transactions,
/// and the inner arrays are lists of `ValueTransferOutput` inside that transaction.
///
/// Note that in order to be valid:
/// * All transactions must have at least one output (but there can be 0 transactions)
/// * All the outputs must have some value (value cannot be 0)
/// * The sum of the value of all the outputs and the total block reward must be below 2^64
#[derive(Clone, Debug, Deserialize)]
#[serde(from = "crate::serialization_helpers::GenesisBlock")]
pub struct GenesisBlockInfo {
    /// The outer array is the list of transactions
    /// and the inner arrays are lists of `ValueTransferOutput` inside that transaction.
    pub alloc: Vec<Vec<ValueTransferOutput>>,
}

impl GenesisBlockInfo {
    /// Create a genesis block with the transactions from `self.alloc`.
    pub fn build_genesis_block(self, bootstrap_hash: Hash) -> Block {
        Block::genesis(
            bootstrap_hash,
            self.alloc.into_iter().map(VTTransaction::genesis).collect(),
        )
    }

    /// Read `GenesisBlockInfo` from `genesis_block.json` file
    pub fn from_path(
        path: &str,
        bootstrap_hash: Hash,
        genesis_block_hash: Hash,
    ) -> Result<Self, GenesisBlockInfoError> {
        let response = std::fs::read_to_string(path).map_err(|e| GenesisBlockInfoError::Read {
            path: path.to_string(),
            inner: e,
        })?;

        let genesis_block: GenesisBlockInfo =
            serde_json::from_str(&response).map_err(GenesisBlockInfoError::Deserialize)?;

        // TODO: the genesis block should only be created once
        let built_genesis_block_hash = genesis_block
            .clone()
            .build_genesis_block(bootstrap_hash)
            .hash();

        if built_genesis_block_hash != genesis_block_hash {
            Err(GenesisBlockInfoError::HashMismatch {
                created: built_genesis_block_hash,
                expected: genesis_block_hash,
            })
        } else {
            Ok(genesis_block)
        }
    }
}

/// Checkpoint beacon structure
#[derive(
    Copy, Clone, Debug, Default, Eq, Hash, PartialEq, Serialize, Deserialize, ProtobufConvert,
)]
#[protobuf_convert(pb = "witnet::CheckpointBeacon")]
#[serde(rename_all = "camelCase")]
pub struct CheckpointBeacon {
    /// The serial number for an epoch
    pub checkpoint: Epoch,
    /// The 256-bit hash of the previous block header
    pub hash_prev_block: Hash,
}

/// Checkpoint VRF structure
#[derive(
    Copy, Clone, Debug, Default, Eq, Hash, PartialEq, Serialize, Deserialize, ProtobufConvert,
)]
#[protobuf_convert(pb = "witnet::CheckpointVRF")]
#[serde(rename_all = "camelCase")]
pub struct CheckpointVRF {
    /// The serial number for an epoch
    pub checkpoint: Epoch,
    /// The 256-bit hash of the VRF used in the previous block
    pub hash_prev_vrf: Hash,
}

/// Epoch id (starting from 0)
pub type Epoch = u32;

/// Block data structure
#[derive(Debug, Eq, PartialEq, Clone, Serialize, Deserialize, ProtobufConvert)]
#[protobuf_convert(pb = "witnet::Block")]
pub struct Block {
    /// The header of the block
    pub block_header: BlockHeader,
    /// A miner-provided signature of the block header, for the sake of integrity
    pub block_sig: KeyedSignature,
    /// A non-empty list of signed transactions
    pub txns: BlockTransactions,
}

/// Block transactions
#[derive(Debug, Default, Eq, PartialEq, Clone, Serialize, Deserialize, ProtobufConvert)]
#[protobuf_convert(pb = "witnet::Block_BlockTransactions")]
pub struct BlockTransactions {
    /// Mint transaction,
    pub mint: MintTransaction,
    /// A list of signed value transfer transactions
    pub value_transfer_txns: Vec<VTTransaction>,
    /// A list of signed data request transactions
    pub data_request_txns: Vec<DRTransaction>,
    /// A list of signed commit transactions
    pub commit_txns: Vec<CommitTransaction>,
    /// A list of signed reveal transactions
    pub reveal_txns: Vec<RevealTransaction>,
    /// A list of signed tally transactions
    pub tally_txns: Vec<TallyTransaction>,
}

impl Block {
    pub fn genesis(bootstrap_hash: Hash, value_transfer_txns: Vec<VTTransaction>) -> Block {
        let txns = BlockTransactions {
            mint: MintTransaction::default(),
            value_transfer_txns,
            data_request_txns: vec![],
            commit_txns: vec![],
            reveal_txns: vec![],
            tally_txns: vec![],
        };

        /// Function to calculate a merkle tree from a transaction vector
        pub fn merkle_tree_root<T>(transactions: &[T]) -> Hash
        where
            T: Hashable,
        {
            let transactions_hashes: Vec<Sha256> = transactions
                .iter()
                .map(|x| match x.hash() {
                    Hash::SHA256(x) => Sha256(x),
                })
                .collect();

            Hash::from(crypto_merkle_tree_root(&transactions_hashes))
        }

        let merkle_roots = BlockMerkleRoots {
            mint_hash: txns.mint.hash(),
            vt_hash_merkle_root: merkle_tree_root(&txns.value_transfer_txns),
            dr_hash_merkle_root: merkle_tree_root(&txns.data_request_txns),
            commit_hash_merkle_root: merkle_tree_root(&txns.commit_txns),
            reveal_hash_merkle_root: merkle_tree_root(&txns.reveal_txns),
            tally_hash_merkle_root: merkle_tree_root(&txns.tally_txns),
        };

        Block {
            block_header: BlockHeader {
                version: 1,
                beacon: CheckpointBeacon {
                    checkpoint: 0,
                    hash_prev_block: bootstrap_hash,
                },
                merkle_roots,
                proof: Default::default(),
                bn256_public_key: Default::default(),
            },
            block_sig: Default::default(),
            txns,
        }
    }
}

impl BlockTransactions {
    pub fn len(&self) -> usize {
        self.mint.len()
            + self.value_transfer_txns.len()
            + self.data_request_txns.len()
            + self.commit_txns.len()
            + self.reveal_txns.len()
            + self.tally_txns.len()
    }

    pub fn is_empty(&self) -> bool {
        self.mint.is_empty()
            && self.value_transfer_txns.is_empty()
            && self.data_request_txns.is_empty()
            && self.commit_txns.is_empty()
            && self.reveal_txns.is_empty()
            && self.tally_txns.is_empty()
    }

    pub fn get(&self, index: TransactionPointer) -> Option<Transaction> {
        match index {
            TransactionPointer::Mint => Some(&self.mint).cloned().map(Transaction::Mint),
            TransactionPointer::ValueTransfer(i) => self
                .value_transfer_txns
                .get(i as usize)
                .cloned()
                .map(Transaction::ValueTransfer),
            TransactionPointer::DataRequest(i) => self
                .data_request_txns
                .get(i as usize)
                .cloned()
                .map(Transaction::DataRequest),
            TransactionPointer::Commit(i) => self
                .commit_txns
                .get(i as usize)
                .cloned()
                .map(Transaction::Commit),
            TransactionPointer::Reveal(i) => self
                .reveal_txns
                .get(i as usize)
                .cloned()
                .map(Transaction::Reveal),
            TransactionPointer::Tally(i) => self
                .tally_txns
                .get(i as usize)
                .cloned()
                .map(Transaction::Tally),
        }
    }

    pub fn create_pointers_to_transactions(&self, block_hash: Hash) -> Vec<(Hash, PointerToBlock)> {
        // Store all the transactions as well
        let mut pointer_to_block = PointerToBlock {
            block_hash,
            transaction_index: TransactionPointer::Mint,
        };
        let mut items_to_add = Vec::with_capacity(self.len());
        // Push mint transaction
        {
            let tx_hash = self.mint.hash();
            items_to_add.push((tx_hash, pointer_to_block.clone()));
        }
        for (i, tx) in self.value_transfer_txns.iter().enumerate() {
            pointer_to_block.transaction_index =
                TransactionPointer::ValueTransfer(u32::try_from(i).unwrap());
            items_to_add.push((tx.hash(), pointer_to_block.clone()));
        }
        for (i, tx) in self.data_request_txns.iter().enumerate() {
            pointer_to_block.transaction_index =
                TransactionPointer::DataRequest(u32::try_from(i).unwrap());
            items_to_add.push((tx.hash(), pointer_to_block.clone()));
        }
        for (i, tx) in self.commit_txns.iter().enumerate() {
            pointer_to_block.transaction_index =
                TransactionPointer::Commit(u32::try_from(i).unwrap());
            items_to_add.push((tx.hash(), pointer_to_block.clone()));
        }
        for (i, tx) in self.reveal_txns.iter().enumerate() {
            pointer_to_block.transaction_index =
                TransactionPointer::Reveal(u32::try_from(i).unwrap());
            items_to_add.push((tx.hash(), pointer_to_block.clone()));
        }
        for (i, tx) in self.tally_txns.iter().enumerate() {
            pointer_to_block.transaction_index =
                TransactionPointer::Tally(u32::try_from(i).unwrap());
            items_to_add.push((tx.hash(), pointer_to_block.clone()));
        }

        items_to_add
    }
}

impl Hashable for BlockHeader {
    fn hash(&self) -> Hash {
        calculate_sha256(&self.to_pb_bytes().unwrap()).into()
    }
}

impl Hashable for Block {
    fn hash(&self) -> Hash {
        self.block_header.hash()
    }
}

impl Hashable for CheckpointBeacon {
    fn hash(&self) -> Hash {
        calculate_sha256(&self.to_pb_bytes().unwrap()).into()
    }
}

impl Hashable for CheckpointVRF {
    fn hash(&self) -> Hash {
        calculate_sha256(&self.to_pb_bytes().unwrap()).into()
    }
}

impl Hashable for DataRequestOutput {
    fn hash(&self) -> Hash {
        calculate_sha256(&self.to_pb_bytes().unwrap()).into()
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
    /// 256-bit hashes of all of the transactions committed to this block, so as to prove their belonging and integrity
    pub merkle_roots: BlockMerkleRoots,
    /// A miner-provided proof of leadership
    pub proof: BlockEligibilityClaim,
    /// The Bn256 public key
    pub bn256_public_key: Option<Bn256PublicKey>,
}
/// Block merkle tree roots
#[derive(Debug, Eq, PartialEq, Clone, Serialize, Deserialize, ProtobufConvert, Default)]
#[protobuf_convert(pb = "witnet::Block_BlockHeader_BlockMerkleRoots")]
pub struct BlockMerkleRoots {
    /// A 256-bit hash based on the mint transaction committed to this block
    pub mint_hash: Hash,
    /// A 256-bit hash based on all of the value transfer transactions committed to this block
    pub vt_hash_merkle_root: Hash,
    /// A 256-bit hash based on all of the data request transactions committed to this block
    pub dr_hash_merkle_root: Hash,
    /// A 256-bit hash based on all of the commit transactions committed to this block
    pub commit_hash_merkle_root: Hash,
    /// A 256-bit hash based on all of the reveal transactions committed to this block
    pub reveal_hash_merkle_root: Hash,
    /// A 256-bit hash based on all of the tally transactions committed to this block
    pub tally_hash_merkle_root: Hash,
}

/// `SuperBlock` abridges the tally and data request information that happened during a
/// `superblock_period` number of Witnet epochs as well the ARS members merkle root
/// as of the last block in that period.
/// This is needed to ensure that the security and trustlessness properties of Witnet will
/// be relayed to bridges with other block chains.
#[derive(Debug, Eq, PartialEq, Clone, Serialize, Deserialize, Default)]
pub struct SuperBlock {
    /// Number of ars members,
    pub ars_length: u64,
    /// Merkle root of the Active Reputation Set members included into the previous SuperBlock
    pub ars_root: Hash,
    /// Merkle root of the data requests in the blocks created since the last SuperBlock
    pub data_request_root: Hash,
    /// Superblock index,
    pub index: u32,
    /// Hash of the block that this SuperBlock is attesting as the latest block in the block chain,
    pub last_block: Hash,
    /// Hash of the block that the previous SuperBlock used for its own `last_block` field,
    pub last_block_in_previous_superblock: Hash,
    /// Merkle root of the tallies in the blocks created since the last SuperBlock
    pub tally_root: Hash,
}

impl Hashable for SuperBlock {
    /// Hash the superblock bytes
    fn hash(&self) -> Hash {
        calculate_sha256(&self.serialize_as_bytes()).into()
    }
}

impl SuperBlock {
    /// Serialize the SuperBlock structure
    /// We do not use protocolBuffers as we would need a protocol buffer decoder in Ethereum
    /// Note that both the node and Ethereum smart contracts need to hash identical superblocks
    fn serialize_as_bytes(&self) -> Vec<u8> {
        [
            &self.ars_length.to_be_bytes()[..],
            self.ars_root.as_ref(),
            self.data_request_root.as_ref(),
            &self.index.to_be_bytes()[..],
            self.last_block.as_ref(),
            self.last_block_in_previous_superblock.as_ref(),
            self.tally_root.as_ref(),
        ]
        .concat()
    }
}

#[derive(Debug, Eq, PartialEq, Clone, Hash, ProtobufConvert, Serialize, Deserialize)]
#[protobuf_convert(pb = "witnet::SuperBlockVote")]
pub struct SuperBlockVote {
    pub bn256_signature: Bn256Signature,
    pub secp256k1_signature: KeyedSignature,
    pub superblock_hash: Hash,
    pub superblock_index: u32,
}

impl SuperBlockVote {
    /// Create a new vote for the `superblock_hash`, but do not sign it.
    /// The vote needs to be signed with a BN256 key, and that signature must
    /// later be signed with a secp256k1 key
    pub fn new_unsigned(superblock_hash: Hash, superblock_index: Epoch) -> Self {
        Self {
            superblock_hash,
            bn256_signature: Bn256Signature { signature: vec![] },
            secp256k1_signature: Default::default(),
            superblock_index,
        }
    }
    pub fn set_bn256_signature(&mut self, bn256_signature: Bn256Signature) {
        self.bn256_signature = bn256_signature;
    }
    pub fn set_secp256k1_signature(&mut self, secp256k1_signature: KeyedSignature) {
        self.secp256k1_signature = secp256k1_signature;
    }
    /// The message to be signed with the secp256k1 key is the concatenation
    /// of the superblock index in big endian and the hash of the superblock as bytes
    pub fn bn256_signature_message(&self) -> Vec<u8> {
        [
            &self.superblock_index.to_be_bytes()[..],
            self.superblock_hash.as_ref(),
        ]
        .concat()
    }
    /// The message to be signed with the secp256k1 key is the concatenation
    /// of the superblock index in big endian, the hash of the superblock and the BN256 signature as bytes:
    pub fn secp256k1_signature_message(&self) -> Vec<u8> {
        [
            &self.superblock_index.to_be_bytes()[..],
            self.superblock_hash.as_ref(),
            &self.bn256_signature.signature,
        ]
        .concat()
    }
}

/// Digital signatures structure (based on supported cryptosystems)
#[derive(Debug, Eq, PartialEq, Clone, Hash, Serialize, Deserialize, ProtobufConvert)]
#[protobuf_convert(pb = "witnet::Signature")]
pub enum Signature {
    /// ECDSA over secp256k1
    Secp256k1(Secp256k1Signature),
}

impl Hashable for Signature {
    fn hash(&self) -> Hash {
        match self {
            Signature::Secp256k1(y) => calculate_sha256(&y.der).into(),
        }
    }
}

impl Default for Signature {
    fn default() -> Self {
        Signature::Secp256k1(Secp256k1Signature::default())
    }
}

impl Signature {
    /// Serialize the Signature for interoperability with OpenSSL.
    pub fn to_bytes(&self) -> Result<[u8; 64], failure::Error> {
        match self {
            Secp256k1(x) => {
                let signature = Secp256k1_Signature::from_der(x.der.as_slice())
                    .map_err(|_| Secp256k1ConversionError::FailSignatureConversion)?;

                Ok(signature.serialize_compact())
            }
        }
    }
}

/// ECDSA (over secp256k1) signature
#[derive(Debug, Default, Eq, PartialEq, Clone, Hash, Serialize, Deserialize, ProtobufConvert)]
#[protobuf_convert(pb = "witnet::Secp256k1Signature")]
pub struct Secp256k1Signature {
    /// The signature serialized in DER
    pub der: Vec<u8>,
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
        let der = secp256k1_signature.serialize_der().to_vec();

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

        PublicKey::from_bytes(serialize)
    }
}

impl TryInto<Secp256k1_PublicKey> for PublicKey {
    type Error = failure::Error;

    fn try_into(self) -> Result<Secp256k1_PublicKey, Self::Error> {
        Secp256k1_PublicKey::from_slice(&self.to_bytes())
            .map_err(|_| Secp256k1ConversionError::FailPublicKeyConversion.into())
    }
}

impl From<Secp256k1_SecretKey> for SecretKey {
    fn from(secp256k1_sk: Secp256k1_SecretKey) -> Self {
        let bytes = Protected::from(&secp256k1_sk[..]);

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
            secret_key: SecretKey {
                bytes: extended_sk.secret(),
            },
            chain_code: extended_sk.chain_code(),
        }
    }
}

impl Into<ExtendedSK> for ExtendedSecretKey {
    fn into(self) -> ExtendedSK {
        let secret_key = self.secret_key.into();

        ExtendedSK::new(secret_key, self.chain_code)
    }
}

/// Hash
#[derive(Eq, PartialEq, Ord, PartialOrd, Copy, Clone, Hash, ProtobufConvert)]
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

impl From<Vec<u8>> for Hash {
    fn from(x: Vec<u8>) -> Self {
        let mut hash = [0u8; 32];
        hash[..32].clone_from_slice(&x[..32]);

        Hash::SHA256(hash)
    }
}

impl AsRef<[u8]> for Hash {
    fn as_ref(&self) -> &[u8] {
        match self {
            Hash::SHA256(bytes) => bytes.as_ref(),
        }
    }
}

impl Into<Sha256> for Hash {
    fn into(self) -> Sha256 {
        match self {
            Hash::SHA256(x) => Sha256(x),
        }
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

impl Hash {
    /// Create a Hash which is all zeros except the first 4 bytes,
    /// which correspond to the bytes of `x` in big endian
    pub fn with_first_u32(x: u32) -> Hash {
        let mut h: [u8; 32] = [0xFF; 32];
        let [h0, h1, h2, h3] = x.to_be_bytes();
        h[0] = h0;
        h[1] = h1;
        h[2] = h2;
        h[3] = h3;

        Hash::SHA256(h)
    }
}

/// Error when parsing hash from string
#[derive(Debug, Fail)]
pub enum HashParseError {
    #[fail(display = "Failed to parse hex: {}", _0)]
    Hex(#[cause] hex::FromHexError),

    #[fail(display = "Invalid hash length: expected 32 bytes but got {}", _0)]
    InvalidLength(usize),
}

impl FromStr for Hash {
    type Err = HashParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let mut h = [0; 32];

        match hex::decode_to_slice(s, &mut h) {
            Err(e) => Err(HashParseError::Hex(e)),
            Ok(_) => Ok(Hash::SHA256(h)),
        }
    }
}

impl fmt::Debug for Hash {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self)
    }
}

/// SHA-256 Hash
pub type SHA256 = [u8; 32];

/// Public Key Hash: slice of the digest of a public key (20 bytes).
///
/// It is the first 20 bytes of the SHA256 hash of the PublicKey.
#[derive(Copy, Clone, Debug, Default, Eq, PartialEq, Hash, ProtobufConvert, Ord, PartialOrd)]
#[protobuf_convert(pb = "witnet::PublicKeyHash")]
pub struct PublicKeyHash {
    pub(crate) hash: [u8; 20],
}

impl AsRef<[u8]> for PublicKeyHash {
    fn as_ref(&self) -> &[u8] {
        self.hash.as_ref()
    }
}

impl fmt::Display for PublicKeyHash {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let address = self.bech32(get_environment());

        f.write_str(&address)
    }
}

impl Hashable for PublicKeyHash {
    fn hash(&self) -> Hash {
        calculate_sha256(&self.to_pb_bytes().unwrap()).into()
    }
}

/// Error when parsing hash from string
#[derive(Debug, Fail)]
pub enum PublicKeyHashParseError {
    #[fail(display = "Failed to parse hex: {}", _0)]
    Hex(#[cause] hex::FromHexError),

    #[fail(display = "Invalid PKH length: expected 20 bytes but got {}", _0)]
    InvalidLength(usize),
    #[fail(
        display = "Address is for different environment: prefix \"{}\" is not valid for {}",
        prefix, expected_environment
    )]
    WrongEnvironment {
        prefix: String,
        expected_environment: Environment,
    },
    #[fail(display = "Failed to deserialize Bech32: {}", _0)]
    Bech32(#[cause] bech32::Error),
}

impl FromStr for PublicKeyHash {
    type Err = PublicKeyHashParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::from_bech32(get_environment(), s)
    }
}

impl PublicKeyHash {
    /// Calculate the hash of the provided public key
    pub fn from_public_key(pk: &PublicKey) -> Self {
        let mut pkh = [0; 20];
        let Hash::SHA256(h) = pk.hash();
        pkh.copy_from_slice(&h[..20]);

        Self { hash: pkh }
    }

    /// Create from existing bytes representing the PKH.
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, PublicKeyHashParseError> {
        let len = bytes.len();

        match len {
            20 => {
                let mut pkh = [0; 20];
                pkh.copy_from_slice(bytes);

                Ok(Self { hash: pkh })
            }
            _ => Err(PublicKeyHashParseError::InvalidLength(len)),
        }
    }

    /// Serialize the PKH as hex bytes
    pub fn to_hex(&self) -> String {
        self.hash
            .iter()
            .fold(String::new(), |acc, x| format!("{}{:02x}", acc, x))
    }

    /// Deserialize PKH from hex bytes
    pub fn from_hex(s: &str) -> Result<Self, PublicKeyHashParseError> {
        let mut hash = [0; 20];

        match hex::decode_to_slice(s, &mut hash) {
            Err(e) => Err(PublicKeyHashParseError::Hex(e)),
            Ok(_) => Ok(PublicKeyHash { hash }),
        }
    }

    /// Serialize PKH according to Bech32
    pub fn bech32(&self, environment: Environment) -> String {
        // This unwrap is safe because every PKH will serialize correctly,
        // and every possible prefix is valid according the Bech32 rules
        bech32::encode(environment.bech32_prefix(), self.hash.to_base32()).unwrap()
    }

    /// Deserialize PKH according to Bech32, checking prefix to avoid mixing mainnet and testned addresses
    pub fn from_bech32(
        environment: Environment,
        address: &str,
    ) -> Result<Self, PublicKeyHashParseError> {
        let (prefix, pkh_u5) = bech32::decode(address).map_err(PublicKeyHashParseError::Bech32)?;
        let pkh_vec = Vec::from_base32(&pkh_u5).map_err(PublicKeyHashParseError::Bech32)?;

        let expected_prefix = environment.bech32_prefix();

        if prefix != expected_prefix {
            Err(PublicKeyHashParseError::WrongEnvironment {
                prefix,
                expected_environment: environment,
            })
        } else {
            Self::from_bytes(&pkh_vec)
        }
    }
}

/// Input data structure
#[derive(Debug, Default, Eq, PartialEq, Clone, Serialize, Deserialize, ProtobufConvert, Hash)]
#[protobuf_convert(pb = "witnet::Input")]
pub struct Input {
    output_pointer: OutputPointer,
}

impl Input {
    /// Create a new Input from an OutputPointer
    pub fn new(output_pointer: OutputPointer) -> Self {
        Self { output_pointer }
    }
    /// Return the [`OutputPointer`](OutputPointer) of an input.
    pub fn output_pointer(&self) -> &OutputPointer {
        &self.output_pointer
    }
    /// Return the [`OutputPointer`](OutputPointer) of an input.
    pub fn into_output_pointer(self) -> OutputPointer {
        self.output_pointer
    }
}

/// Value transfer output transaction data structure
#[derive(Debug, Eq, PartialEq, Clone, Serialize, Deserialize, ProtobufConvert, Hash, Default)]
#[protobuf_convert(pb = "witnet::ValueTransferOutput")]
pub struct ValueTransferOutput {
    pub pkh: PublicKeyHash,
    pub value: u64,
    /// The value attached to a time-locked output cannot be spent before the specified
    /// timestamp. That is, they cannot be used as an input in any transaction of a
    /// subsequent block proposed for an epoch whose opening timestamp predates the time lock.
    pub time_lock: u64,
}

/// Data request output transaction data structure
#[derive(Debug, Eq, PartialEq, Clone, Serialize, Deserialize, ProtobufConvert, Hash, Default)]
#[protobuf_convert(pb = "witnet::DataRequestOutput")]
pub struct DataRequestOutput {
    pub data_request: RADRequest,
    pub witness_reward: u64,
    pub witnesses: u16,
    pub commit_fee: u64,
    pub reveal_fee: u64,
    pub tally_fee: u64,
    // This field must be >50 and <100.
    // >50 because simple majority
    // <100 because a 100% consensus encourages to commit a RadError for free
    pub min_consensus_percentage: u32,
    // This field must be >= collateral_minimum, or zero
    // If zero, it will be treated as collateral_minimum
    pub collateral: u64,
}

impl DataRequestOutput {
    /// Calculate the total value of a data request, return error on overflow
    ///
    /// ```ignore
    /// total_value = (witness_reward + commit_fee + reveal_fee) * witnesses + tally_fee
    /// ```
    pub fn checked_total_value(&self) -> Result<u64, TransactionError> {
        self.witness_reward
            .checked_add(self.commit_fee)
            .and_then(|res| res.checked_add(self.reveal_fee))
            .and_then(|res| res.checked_mul(u64::from(self.witnesses)))
            .and_then(|res| res.checked_add(self.tally_fee))
            .ok_or_else(|| TransactionError::FeeOverflow)
    }

    /// Returns the DataRequestOutput weight
    pub fn weight(&self) -> u32 {
        // Witness reward: 8 bytes
        // Witnesses: 2 bytes
        // commit_fee: 8 bytes
        // reveal_fee: 8 bytes
        // tally_fee: 8 bytes
        // min_consensus_percentage: 4 bytes
        // collateral: 8 bytes

        self.data_request
            .weight()
            .saturating_add(8 + 2 + 8 + 8 + 8 + 4 + 8)
    }
}

/// Keyed signature data structure
#[derive(Debug, Default, Eq, PartialEq, Clone, Hash, Serialize, Deserialize, ProtobufConvert)]
#[protobuf_convert(pb = "witnet::KeyedSignature")]
pub struct KeyedSignature {
    pub signature: Signature,
    pub public_key: PublicKey,
}

/// Public Key data structure
#[derive(Debug, Default, Eq, PartialEq, Clone, Hash, Serialize, Deserialize)]
pub struct PublicKey {
    pub compressed: u8,
    pub bytes: [u8; 32],
}

impl PublicKey {
    /// Serialize the Public Key for interoperability with OpenSSL.
    pub fn to_bytes(&self) -> [u8; 33] {
        let mut v = [0; 33];
        v[0] = self.compressed;
        v[1..].copy_from_slice(&self.bytes);
        v
    }

    /// Deserialize the Public Key for interoperability with OpenSSL.
    pub fn from_bytes(serialized: [u8; 33]) -> Self {
        Self::try_from_slice(&serialized[..]).unwrap()
    }

    /// Deserialize the Public Key for interoperability with OpenSSL.
    /// Returns an error if the slice is not 33 bytes long.
    pub fn try_from_slice(serialized: &[u8]) -> Result<Self, Secp256k1ConversionError> {
        if serialized.len() != 33 {
            Err(Secp256k1ConversionError::FailPublicKeyFromSlice {
                size: serialized.len(),
            })
        } else {
            let mut x = [0; 32];
            x.copy_from_slice(&serialized[1..]);

            Ok(PublicKey {
                compressed: serialized[0],
                bytes: x,
            })
        }
    }

    /// Returns the PublicKeyHash related to the PublicKey
    pub fn pkh(&self) -> PublicKeyHash {
        PublicKeyHash::from_public_key(&self)
    }
}

/// Secret Key data structure
#[derive(Debug, Eq, PartialEq, Clone, Serialize, Deserialize)]
pub struct SecretKey {
    pub bytes: Protected,
}

/// Extended Secret Key data structure
#[derive(Debug, Eq, PartialEq, Clone, Serialize, Deserialize)]
pub struct ExtendedSecretKey {
    /// Secret key
    pub secret_key: SecretKey,
    /// Chain code
    pub chain_code: Protected,
}

/// BLS data structures

#[derive(Debug, Default, Eq, PartialEq, Clone, Hash, Serialize, Deserialize, ProtobufConvert)]
#[protobuf_convert(pb = "witnet::Bn256PublicKey")]
pub struct Bn256PublicKey {
    /// Compressed form of a BN256 public key
    pub public_key: Vec<u8>,
}
#[derive(Debug, Eq, PartialEq, Clone, Serialize, Deserialize)]
pub struct Bn256SecretKey {
    pub bytes: Protected,
}
#[derive(Debug, Eq, PartialEq, Clone, Hash, Serialize, Deserialize, ProtobufConvert)]
#[protobuf_convert(pb = "witnet::Bn256Signature")]
pub struct Bn256Signature {
    pub signature: Vec<u8>,
}
#[derive(Debug, Eq, PartialEq, Clone, Hash, Serialize, Deserialize, ProtobufConvert)]
#[protobuf_convert(pb = "witnet::Bn256KeyedSignature")]
pub struct Bn256KeyedSignature {
    pub signature: Bn256Signature,
    pub public_key: Bn256PublicKey,
}

impl Bn256PublicKey {
    pub fn from_secret_key(secret_key: &Bn256SecretKey) -> Result<Self, failure::Error> {
        let public_key = Bn256.derive_public_key(&secret_key.bytes)?;

        Ok(Self { public_key })
    }

    pub fn from_bytes(bytes: &[u8]) -> Result<Self, failure::Error> {
        // Verify that this slice is a valid public key
        let _ = bn256::PublicKey::from_compressed(bytes)?;

        Ok(Self {
            public_key: bytes.to_vec(),
        })
    }

    pub fn to_uncompressed(&self) -> Result<Vec<u8>, failure::Error> {
        // Verify that this slice is a valid public key
        let uncompressed =
            bn256::PublicKey::from_compressed(&self.public_key)?.to_uncompressed()?;
        Ok(uncompressed)
    }

    pub fn is_valid(&self) -> bool {
        // Verify that the provided key is a valid public key
        bn256::PublicKey::from_compressed(&self.public_key).is_ok()
    }

    pub fn to_bytes(&self) -> Vec<u8> {
        self.public_key.clone()
    }
}

impl Hashable for Bn256PublicKey {
    fn hash(&self) -> Hash {
        calculate_sha256(&self.to_uncompressed().unwrap()).into()
    }
}

impl Bn256SecretKey {
    pub fn from_slice(bytes: &[u8]) -> Result<Self, failure::Error> {
        // Verify that this slice is a valid secret key
        let _ = bn256::PrivateKey::new(bytes)?;

        Ok(Self {
            bytes: bytes.to_vec().into(),
        })
    }

    pub fn sign(&self, message: &[u8]) -> Result<Bn256Signature, failure::Error> {
        let signature = Bn256.sign(&self.bytes, &message)?;

        Ok(Bn256Signature { signature })
    }
}

impl Hashable for Bn256Signature {
    fn hash(&self) -> Hash {
        calculate_sha256(&self.signature).into()
    }
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
#[protobuf_convert(pb = "witnet::DataRequestOutput_RADRequest", crate = "crate")]
pub struct RADRequest {
    /// Commitments for this request will not be accepted in any block proposed for an epoch
    /// whose opening timestamp predates the specified time lock. This effectively prevents
    /// a request from being processed before a specific future point in time.
    pub time_lock: u64,
    pub retrieve: Vec<RADRetrieve>,
    pub aggregate: RADAggregate,
    pub tally: RADTally,
}

impl RADRequest {
    pub fn weight(&self) -> u32 {
        // Time lock: 8 bytes

        let mut retrievals_weight: u32 = 0;
        for i in self.retrieve.iter() {
            retrievals_weight = retrievals_weight.saturating_add(i.weight());
        }

        retrievals_weight
            .saturating_add(self.aggregate.weight())
            .saturating_add(self.tally.weight())
            .saturating_add(8)
    }
}

#[derive(Debug, Eq, PartialEq, Clone, Serialize, Deserialize, ProtobufConvert, Hash, Default)]
#[protobuf_convert(
    pb = "witnet::DataRequestOutput_RADRequest_RADRetrieve",
    crate = "crate"
)]
pub struct RADRetrieve {
    pub kind: RADType,
    pub url: String,
    pub script: Vec<u8>,
}

impl RADRetrieve {
    pub fn weight(&self) -> u32 {
        // RADType: 1 byte
        let script_weight = u32::try_from(self.script.len()).unwrap_or(u32::MAX);
        let url_weight = u32::try_from(self.url.len()).unwrap_or(u32::MAX);

        script_weight.saturating_add(url_weight).saturating_add(1)
    }
}

#[derive(Debug, Eq, PartialEq, Clone, Serialize, Deserialize, ProtobufConvert, Hash, Default)]
#[protobuf_convert(pb = "witnet::DataRequestOutput_RADRequest_RADFilter", crate = "crate")]
pub struct RADFilter {
    pub op: u32,
    pub args: Vec<u8>,
}

impl RADFilter {
    pub fn weight(&self) -> u32 {
        // op: 4 bytes
        let args_weight = u32::try_from(self.args.len()).unwrap_or(u32::MAX);

        args_weight.saturating_add(4)
    }
}
#[derive(Debug, Eq, PartialEq, Clone, Serialize, Deserialize, ProtobufConvert, Hash, Default)]
#[protobuf_convert(
    pb = "witnet::DataRequestOutput_RADRequest_RADAggregate",
    crate = "crate"
)]
pub struct RADAggregate {
    pub filters: Vec<RADFilter>,
    pub reducer: u32,
}

impl RADAggregate {
    pub fn weight(&self) -> u32 {
        // reducer: 4 bytes

        let mut filters_weight: u32 = 0;
        for i in self.filters.iter() {
            filters_weight = filters_weight.saturating_add(i.weight());
        }

        filters_weight.saturating_add(4)
    }
}

#[derive(Debug, Eq, PartialEq, Clone, Serialize, Deserialize, ProtobufConvert, Hash, Default)]
#[protobuf_convert(pb = "witnet::DataRequestOutput_RADRequest_RADTally", crate = "crate")]
pub struct RADTally {
    pub filters: Vec<RADFilter>,
    pub reducer: u32,
}

impl RADTally {
    pub fn weight(&self) -> u32 {
        // reducer: 4 bytes

        let mut filters_weight: u32 = 0;
        for i in self.filters.iter() {
            filters_weight = filters_weight.saturating_add(i.weight());
        }

        filters_weight.saturating_add(4)
    }
}

type PrioritizedHash = (OrderedFloat<f64>, Hash);
type PrioritizedVTTransaction = (OrderedFloat<f64>, VTTransaction);
type PrioritizedDRTransaction = (OrderedFloat<f64>, DRTransaction);

/// A pool of validated transactions that supports constant access by
/// [`Hash`](Hash) and iteration over the
/// transactions sorted from by transactions with bigger fees to
/// transactions with smaller fees.
#[derive(Debug, Default, Clone)]
pub struct TransactionsPool {
    vt_transactions: HashMap<Hash, PrioritizedVTTransaction>,
    sorted_vt_index: BTreeSet<PrioritizedHash>,
    dr_transactions: HashMap<Hash, PrioritizedDRTransaction>,
    sorted_dr_index: BTreeSet<PrioritizedHash>,
    // Index commits by transaction hash
    co_hash_index: HashMap<Hash, CommitTransaction>,
    // A map of `data_request_hash` to a map of `commit_pkh` to `commit_transaction_hash`
    co_transactions: HashMap<Hash, HashMap<PublicKeyHash, Hash>>,
    // Index reveals by transaction hash
    re_hash_index: HashMap<Hash, RevealTransaction>,
    // A map of `data_request_hash` to a map of `reveal_pkh` to `reveal_transaction_hash`
    re_transactions: HashMap<Hash, HashMap<PublicKeyHash, Hash>>,
    // A hashset of recently received transactions hashes
    // Used to avoid validating the same transaction more than once
    pending_transactions: HashSet<Hash>,
    // Map to avoid double spending issues
    output_pointer_map: HashMap<OutputPointer, Vec<Hash>>,
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
        TransactionsPool::default()
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
            vt_transactions: HashMap::with_capacity(capacity),
            dr_transactions: HashMap::with_capacity(capacity),
            co_hash_index: HashMap::with_capacity(capacity),
            co_transactions: HashMap::with_capacity(capacity),
            re_hash_index: HashMap::with_capacity(capacity),
            re_transactions: HashMap::with_capacity(capacity),
            sorted_vt_index: BTreeSet::new(),
            sorted_dr_index: BTreeSet::new(),
            pending_transactions: HashSet::new(),
            output_pointer_map: HashMap::with_capacity(capacity),
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
        self.vt_transactions.capacity()
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
        self.vt_transactions.is_empty()
            && self.dr_transactions.is_empty()
            && self.co_transactions.is_empty()
            && self.re_transactions.is_empty()
    }

    /// Returns the number of value transfer transactions in the pool.
    ///
    /// # Examples:
    ///
    /// ```
    /// # use witnet_data_structures::chain::{TransactionsPool, Hash};
    /// # use witnet_data_structures::transaction::{Transaction, VTTransaction};
    /// let mut pool = TransactionsPool::new();
    ///
    /// let transaction = Transaction::ValueTransfer(VTTransaction::default());
    ///
    /// assert_eq!(pool.vt_len(), 0);
    ///
    /// pool.insert(transaction, 0);
    ///
    /// assert_eq!(pool.vt_len(), 1);
    /// ```
    pub fn vt_len(&self) -> usize {
        self.vt_transactions.len()
    }

    /// Returns the number of data request transactions in the pool.
    ///
    /// # Examples:
    ///
    /// ```
    /// # use witnet_data_structures::chain::{TransactionsPool, Hash};
    /// # use witnet_data_structures::transaction::{Transaction, DRTransaction};
    /// let mut pool = TransactionsPool::new();
    ///
    /// let transaction = Transaction::DataRequest(DRTransaction::default());
    ///
    /// assert_eq!(pool.dr_len(), 0);
    ///
    /// pool.insert(transaction, 0);
    ///
    /// assert_eq!(pool.dr_len(), 1);
    /// ```
    pub fn dr_len(&self) -> usize {
        self.dr_transactions.len()
    }

    /// Clear commit transactions in TransactionsPool
    pub fn clear_commits(&mut self) {
        self.co_transactions.clear();
        self.co_hash_index.clear();
    }

    /// Clear reveal transactions in TransactionsPool
    pub fn clear_reveals(&mut self) {
        self.re_transactions.clear();
        self.re_hash_index.clear();
    }

    /// Returns `Ok(true)` if the pool already contains this transaction.
    /// Returns an error if:
    /// * The transaction is of an invalid type (mint or tally)
    /// * The commit transaction has the same data request pointer and pkh as
    /// an existing transaction, but different hash.
    /// * The reveal transaction has the same data request pointer and pkh as
    /// an existing transaction, but different hash.
    pub fn contains(&self, transaction: &Transaction) -> Result<bool, TransactionError> {
        let tx_hash = transaction.hash();

        if self.pending_transactions.contains(&tx_hash) {
            Ok(true)
        } else {
            match transaction {
                Transaction::ValueTransfer(_vt) => Ok(self.vt_contains(&tx_hash)),
                Transaction::DataRequest(_drt) => Ok(self.dr_contains(&tx_hash)),
                Transaction::Commit(ct) => {
                    let dr_pointer = ct.body.dr_pointer;
                    let pkh = ct.body.proof.proof.pkh();

                    self.commit_contains(&dr_pointer, &pkh, &tx_hash)
                }
                Transaction::Reveal(rt) => {
                    let dr_pointer = rt.body.dr_pointer;
                    let pkh = rt.body.pkh;

                    self.reveal_contains(&dr_pointer, &pkh, &tx_hash)
                }
                // Tally and mint transaction only exist inside blocks, it should
                // be impossible for nodes to broadcast these kinds of transactions.
                Transaction::Tally(_tt) => Err(TransactionError::NotValidTransaction),
                Transaction::Mint(_mt) => Err(TransactionError::NotValidTransaction),
            }
        }
    }

    /// Returns `true` if the pool contains a value transfer
    /// transaction for the specified hash.
    ///
    /// The `key` may be any borrowed form of the hash, but `Hash` and
    /// `Eq` on the borrowed form must match those for the key type.
    ///
    /// # Examples:
    /// ```
    /// # use witnet_data_structures::chain::{TransactionsPool, Hash, Hashable};
    /// # use witnet_data_structures::transaction::{Transaction, VTTransaction};
    /// let mut pool = TransactionsPool::new();
    ///
    /// let transaction = Transaction::ValueTransfer(VTTransaction::default());
    /// let hash = transaction.hash();
    /// assert!(!pool.vt_contains(&hash));
    ///
    /// pool.insert(transaction, 0);
    ///
    /// assert!(pool.vt_contains(&hash));
    /// ```
    pub fn vt_contains(&self, key: &Hash) -> bool {
        self.vt_transactions.contains_key(key)
    }

    /// Returns `true` if the pool contains a data request
    /// transaction for the specified hash.
    ///
    /// The `key` may be any borrowed form of the hash, but `Hash` and
    /// `Eq` on the borrowed form must match those for the key type.
    pub fn dr_contains(&self, key: &Hash) -> bool {
        self.dr_transactions.contains_key(key)
    }

    /// Returns `Ok(true)` if the pool contains a commit transaction for the specified hash,
    /// data request pointer, and pkh. Return an error if the pool contains a commit transaction
    /// with the same data request pointer and pkh, but different hash.
    ///
    /// The `key` may be any borrowed form of the hash, but `Hash` and
    /// `Eq` on the borrowed form must match those for the key type.
    pub fn commit_contains(
        &self,
        dr_pointer: &Hash,
        pkh: &PublicKeyHash,
        tx_hash: &Hash,
    ) -> Result<bool, TransactionError> {
        self.co_transactions
            .get(dr_pointer)
            .and_then(|hm| hm.get(pkh))
            .map(|h| {
                if h == tx_hash {
                    Ok(true)
                } else {
                    Err(TransactionError::DuplicatedCommit {
                        pkh: *pkh,
                        dr_pointer: *dr_pointer,
                    })
                }
            })
            .unwrap_or(Ok(false))
    }

    /// Returns `Ok(true)` if the pool contains a reveal transaction for the specified hash,
    /// data request pointer, and pkh. Return an error if the pool contains a reveal transaction
    /// with the same data request pointer and pkh, but different hash.
    ///
    /// The `key` may be any borrowed form of the hash, but `Hash` and
    /// `Eq` on the borrowed form must match those for the key type.
    pub fn reveal_contains(
        &self,
        dr_pointer: &Hash,
        pkh: &PublicKeyHash,
        tx_hash: &Hash,
    ) -> Result<bool, TransactionError> {
        self.re_transactions
            .get(dr_pointer)
            .and_then(|hm| hm.get(pkh))
            .map(|h| {
                if h == tx_hash {
                    Ok(true)
                } else {
                    Err(TransactionError::DuplicatedReveal {
                        pkh: *pkh,
                        dr_pointer: *dr_pointer,
                    })
                }
            })
            .unwrap_or(Ok(false))
    }

    /// Remove a value transfer transaction from the pool and make sure that other transactions
    /// that may try to spend the same UTXOs are also removed.
    ///
    /// Returns an `Option` with the value transfer transaction for the specified hash or `None` if not exist.
    ///
    /// The `key` may be any borrowed form of the hash, but `Hash` and
    /// `Eq` on the borrowed form must match those for the key type.
    ///
    /// # Examples:
    /// ```
    /// # use witnet_data_structures::chain::{TransactionsPool, Hash, Hashable};
    /// # use witnet_data_structures::transaction::{Transaction, VTTransaction};
    /// let mut pool = TransactionsPool::new();
    /// let vt_transaction = VTTransaction::default();
    /// let transaction = Transaction::ValueTransfer(vt_transaction.clone());
    /// pool.insert(transaction.clone(),0);
    ///
    /// assert!(pool.vt_contains(&transaction.hash()));
    ///
    /// let op_transaction_removed = pool.vt_remove(&transaction.hash());
    ///
    /// assert_eq!(Some(vt_transaction), op_transaction_removed);
    /// assert!(!pool.vt_contains(&transaction.hash()));
    /// ```
    pub fn vt_remove(&mut self, key: &Hash) -> Option<VTTransaction> {
        let transaction = self.vt_remove_inner(key);

        if let Some(transaction) = &transaction {
            self.remove_inputs(&transaction.body.inputs);
        }

        transaction
    }

    /// Remove a value transfer transaction from the pool without any further guarantee
    /// about removing other transactions that may try to spend the same UTXOs.
    fn vt_remove_inner(&mut self, key: &Hash) -> Option<VTTransaction> {
        self.vt_transactions
            .remove(key)
            .map(|(weight, transaction)| {
                self.sorted_vt_index.remove(&(weight, *key));
                transaction
            })
    }

    /// Remove a data request transaction from the pool and make sure that other transactions
    /// that may try to spend the same UTXOs are also removed.
    ///
    /// Returns an `Option` with the data request transaction for the specified hash or `None` if not exist.
    ///
    /// The `key` may be any borrowed form of the hash, but `Hash` and
    /// `Eq` on the borrowed form must match those for the key type.
    ///
    /// # Examples:
    /// ```
    /// # use witnet_data_structures::chain::{TransactionsPool, Hash, Hashable};
    /// # use witnet_data_structures::transaction::{Transaction, DRTransaction};
    /// let mut pool = TransactionsPool::new();
    /// let dr_transaction = DRTransaction::default();
    /// let transaction = Transaction::DataRequest(dr_transaction.clone());
    /// pool.insert(transaction.clone(),0);
    ///
    /// assert!(pool.dr_contains(&transaction.hash()));
    ///
    /// let op_transaction_removed = pool.dr_remove(&transaction.hash());
    ///
    /// assert_eq!(Some(dr_transaction), op_transaction_removed);
    /// assert!(!pool.dr_contains(&transaction.hash()));
    /// ```
    pub fn dr_remove(&mut self, key: &Hash) -> Option<DRTransaction> {
        let transaction = self.dr_remove_inner(key);

        if let Some(transaction) = &transaction {
            self.remove_inputs(&transaction.body.inputs);
        }

        transaction
    }

    /// Remove a data request transaction from the pool without any further guarantee
    /// about removing other transactions that may try to spend the same UTXOs.
    fn dr_remove_inner(&mut self, key: &Hash) -> Option<DRTransaction> {
        self.dr_transactions
            .remove(key)
            .map(|(weight, transaction)| {
                self.sorted_dr_index.remove(&(weight, *key));
                transaction
            })
    }

    /// Remove all the transactions with the specified inputs
    pub fn remove_inputs(&mut self, inputs: &[Input]) {
        for input in inputs.iter() {
            if let Some(hashes) = self.output_pointer_map.remove(&input.output_pointer) {
                for hash in hashes.iter() {
                    self.vt_remove_inner(hash);
                    self.dr_remove_inner(hash);
                }
            }
        }
    }

    /// Returns a tuple with a vector of commit transactions that achieve the minimum specify
    /// by the data request, and the value of all the fees obtained with those commits
    pub fn remove_commits(&mut self, dr_pool: &DataRequestPool) -> (Vec<CommitTransaction>, u64) {
        let mut total_fee = 0;
        let mut spent_inputs = HashSet::new();
        let co_hash_index = &mut self.co_hash_index;
        let commits_vector = self
            .co_transactions
            .iter_mut()
            // TODO: Decide with optimal capacity
            .fold(
                Vec::with_capacity(20),
                |mut commits_vec, (dr_pointer, commits)| {
                    if let Some(dr_output) = dr_pool.get_dr_output(&dr_pointer) {
                        let n_commits = dr_output.witnesses as usize;

                        if commits.len() >= n_commits {
                            let filtered_commits: Vec<CommitTransaction> = commits
                                .drain()
                                .filter_map(|(_h, c)| {
                                    co_hash_index.remove(&c).and_then(|commit_tx| {
                                        let mut valid = true;
                                        let mut current_spent_inputs = HashSet::new();
                                        for input in commit_tx.body.collateral.iter() {
                                            // Check that no Input from this or any other commitment is being spent twice
                                            if spent_inputs.contains(input)
                                                || current_spent_inputs.contains(input)
                                            {
                                                valid = false;
                                                break;
                                            } else {
                                                current_spent_inputs.insert(input.clone());
                                            }
                                        }
                                        // Only mark the inputs of this commitment as used if all of them
                                        // are valid and they will be returned by this function
                                        if valid {
                                            spent_inputs.extend(current_spent_inputs);
                                            Some(commit_tx)
                                        } else {
                                            None
                                        }
                                    })
                                })
                                .take(n_commits)
                                .collect();

                            // Check once again that after filtering invalid commitments, we still have as many as requested
                            if filtered_commits.len() == n_commits {
                                commits_vec.extend(filtered_commits);
                                total_fee += dr_output.commit_fee * n_commits as u64;
                            }
                        }
                    }
                    commits_vec
                },
            );

        // Clear commit hash index: commits are invalidated at the end of the epoch
        self.clear_commits();

        (commits_vector, total_fee)
    }

    /// Returns a tuple with a vector of reveal transactions and the value
    /// of all the fees obtained with those reveals
    pub fn remove_reveals(&mut self, dr_pool: &DataRequestPool) -> (Vec<RevealTransaction>, u64) {
        let mut total_fee = 0;
        let re_hash_index = &mut self.re_hash_index;
        let reveals_vector = self
            .re_transactions
            .iter_mut()
            // TODO: Decide with optimal capacity
            .fold(
                Vec::with_capacity(20),
                |mut reveals_vec, (dr_pointer, reveals)| {
                    if let Some(dr_output) = dr_pool.get_dr_output(&dr_pointer) {
                        let n_reveals = reveals.len();
                        reveals_vec.extend(
                            reveals
                                .drain()
                                .map(|(_h, r)| re_hash_index.remove(&r).unwrap()),
                        );

                        total_fee += dr_output.reveal_fee * n_reveals as u64;
                    }

                    reveals_vec
                },
            );
        // Clear reveal hash index: reveals can still be added to later blocks, but a miner will
        // always use as many reveals as possible, and this method is used by the mining code
        self.clear_reveals();

        (reveals_vector, total_fee)
    }

    /// Insert a transaction identified by `key` into the pool.
    ///
    /// # Examples:
    ///
    /// ```
    /// # use witnet_data_structures::chain::{TransactionsPool, Hash};
    /// # use witnet_data_structures::transaction::{Transaction, VTTransaction};
    /// let mut pool = TransactionsPool::new();
    /// let transaction = Transaction::ValueTransfer(VTTransaction::default());
    /// pool.insert(transaction, 0);
    ///
    /// assert!(!pool.is_empty());
    /// ```
    #[allow(clippy::cast_precision_loss)]
    pub fn insert(&mut self, transaction: Transaction, fee: u64) {
        let key = transaction.hash();

        match transaction {
            Transaction::ValueTransfer(vt_tx) => {
                let weight = f64::from(vt_tx.weight());
                let priority = OrderedFloat(fee as f64 / weight);

                for input in &vt_tx.body.inputs {
                    self.output_pointer_map
                        .entry(input.output_pointer.clone())
                        .or_insert_with(Vec::new)
                        .push(vt_tx.hash());
                }

                self.vt_transactions.insert(key, (priority, vt_tx));
                self.sorted_vt_index.insert((priority, key));
            }
            Transaction::DataRequest(dr_tx) => {
                let weight = f64::from(dr_tx.weight());
                let priority = OrderedFloat(fee as f64 / weight);

                for input in &dr_tx.body.inputs {
                    self.output_pointer_map
                        .entry(input.output_pointer.clone())
                        .or_insert_with(Vec::new)
                        .push(dr_tx.hash());
                }

                self.dr_transactions.insert(key, (priority, dr_tx));
                self.sorted_dr_index.insert((priority, key));
            }
            Transaction::Commit(co_tx) => {
                let dr_pointer = co_tx.body.dr_pointer;
                let pkh = PublicKeyHash::from_public_key(&co_tx.signatures[0].public_key);
                let tx_hash = co_tx.hash();

                self.co_hash_index.insert(tx_hash, co_tx);
                self.co_transactions
                    .entry(dr_pointer)
                    .or_default()
                    .insert(pkh, tx_hash);
            }
            Transaction::Reveal(re_tx) => {
                let dr_pointer = re_tx.body.dr_pointer;
                let pkh = re_tx.body.pkh;
                let tx_hash = re_tx.hash();

                self.re_hash_index.insert(tx_hash, re_tx);
                self.re_transactions
                    .entry(dr_pointer)
                    .or_default()
                    .insert(pkh, tx_hash);
            }
            _ => {}
        }
    }

    /// Insert a pending transaction hash
    pub fn insert_pending_transaction(&mut self, transaction: &Transaction) {
        self.pending_transactions.insert(transaction.hash());
    }

    /// Remove a pending transaction hash
    pub fn clear_pending_transactions(&mut self) {
        self.pending_transactions.clear();
    }

    /// An iterator visiting all the value transfer transactions
    /// in the pool in descending-fee order, that is, transactions
    /// with bigger fees come first.
    ///
    /// Examples:
    ///
    /// ```
    /// # use witnet_data_structures::chain::{TransactionsPool, Hash};
    /// # use witnet_data_structures::transaction::{Transaction, VTTransaction};
    /// let mut pool = TransactionsPool::new();
    ///
    /// let transaction = Transaction::ValueTransfer(VTTransaction::default());
    ///
    /// pool.insert(transaction.clone(),0);
    /// pool.insert(transaction, 0);
    ///
    /// let mut iter = pool.vt_iter();
    /// let tx1 = iter.next();
    /// let tx2 = iter.next();
    ///
    /// ```
    pub fn vt_iter(&self) -> impl Iterator<Item = &VTTransaction> {
        self.sorted_vt_index
            .iter()
            .rev()
            .filter_map(move |(_, h)| self.vt_transactions.get(h).map(|(_, t)| t))
    }

    /// An iterator visiting all the data request transactions
    /// in the pool
    pub fn dr_iter(&self) -> impl Iterator<Item = &DRTransaction> {
        self.sorted_dr_index
            .iter()
            .rev()
            .filter_map(move |(_, h)| self.dr_transactions.get(h).map(|(_, t)| t))
    }

    /// Returns a reference to the value corresponding to the key.
    ///
    /// Examples:
    ///
    /// ```
    /// # use witnet_data_structures::chain::{TransactionsPool, Hash, Hashable};
    /// # use witnet_data_structures::transaction::{Transaction, VTTransaction};
    /// let mut pool = TransactionsPool::new();
    ///
    /// let transaction = Transaction::ValueTransfer(VTTransaction::default());
    /// let hash = transaction.hash();
    ///
    /// assert!(pool.vt_get(&hash).is_none());
    ///
    /// pool.insert(transaction, 0);
    ///
    /// assert!(pool.vt_get(&hash).is_some());
    /// ```
    pub fn vt_get(&self, key: &Hash) -> Option<&VTTransaction> {
        self.vt_transactions
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
    /// # use witnet_data_structures::chain::{TransactionsPool, Hash, ValueTransferOutput};
    /// # use witnet_data_structures::transaction::{Transaction, VTTransaction, VTTransactionBody};
    /// use witnet_data_structures::transaction::Transaction::ValueTransfer;
    ///
    /// let mut pool = TransactionsPool::new();
    ///
    /// let transaction1 = Transaction::ValueTransfer(VTTransaction::default());
    /// let transaction2 = Transaction::ValueTransfer(VTTransaction::new(VTTransactionBody::new(vec![], vec![ValueTransferOutput {
    /// value:3,
    /// ..ValueTransferOutput::default()
    /// }]),
    /// vec![]));
    ///
    /// pool.insert(transaction1,0);
    /// pool.insert(transaction2,0);
    /// assert_eq!(pool.vt_len(), 2);
    /// pool.vt_retain(|tx| tx.body.outputs.len()>0);
    /// assert_eq!(pool.vt_len(), 1);
    /// ```
    pub fn vt_retain<F>(&mut self, mut f: F)
    where
        F: FnMut(&VTTransaction) -> bool,
    {
        let TransactionsPool {
            ref mut vt_transactions,
            sorted_vt_index: ref mut sorted_index,
            ..
        } = *self;

        vt_transactions.retain(|hash, (weight, vt_transaction)| {
            let retain = f(vt_transaction);
            if !retain {
                sorted_index.remove(&(*weight, *hash));
            }

            retain
        });
    }

    /// Get transaction by hash
    pub fn get(&self, hash: &Hash) -> Option<Transaction> {
        self.vt_transactions
            .get(hash)
            .map(|(_, vtt)| Transaction::ValueTransfer(vtt.clone()))
            .or_else(|| {
                self.dr_transactions
                    .get(hash)
                    .map(|(_, drt)| Transaction::DataRequest(drt.clone()))
            })
            .or_else(|| {
                self.co_hash_index
                    .get(hash)
                    .map(|ct| Transaction::Commit(ct.clone()))
            })
            .or_else(|| {
                self.re_hash_index
                    .get(hash)
                    .map(|rt| Transaction::Reveal(rt.clone()))
            })
    }
}

/// Unspent output data structure (equivalent of Bitcoin's UTXO)
/// It is used to locate the output by its transaction identifier and its position
#[derive(Default, Hash, Clone, Eq, PartialEq, ProtobufConvert)]
#[protobuf_convert(pb = "witnet::OutputPointer")]
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

        let transaction_id = tokens
            .next()
            .ok_or(OutputPointerParseError::Hash(
                HashParseError::InvalidLength(0),
            ))?
            .parse()
            .map_err(OutputPointerParseError::Hash)?;
        let output_index = tokens
            .next()
            .ok_or(OutputPointerParseError::MissingColon)?
            .parse::<u32>()
            .map_err(OutputPointerParseError::ParseIntError)?;

        Ok(OutputPointer {
            output_index,
            transaction_id,
        })
    }
}

impl Ord for OutputPointer {
    fn cmp(&self, other: &Self) -> Ordering {
        (self.transaction_id, self.output_index).cmp(&(other.transaction_id, other.output_index))
    }
}

impl PartialOrd for OutputPointer {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(&other))
    }
}

/// Inventory entry data structure
#[derive(Debug, Eq, PartialEq, Clone, Serialize, Deserialize, ProtobufConvert)]
#[protobuf_convert(pb = "witnet::InventoryEntry")]
pub enum InventoryEntry {
    Tx(Hash),
    Block(Hash),
}

#[derive(Debug, Eq, PartialEq, Copy, Clone, Serialize, Deserialize)]
pub enum TransactionPointer {
    ValueTransfer(u32),
    DataRequest(u32),
    Commit(u32),
    Reveal(u32),
    Tally(u32),
    Mint,
}

/// This is how transactions are stored in the database: hash of the containing block, plus index
#[derive(Debug, Eq, PartialEq, Clone, Serialize, Deserialize)]
pub struct PointerToBlock {
    pub block_hash: Hash,
    pub transaction_index: TransactionPointer,
}

/// Inventory element: block, txns
#[derive(Debug, Eq, PartialEq, Clone, Serialize, Deserialize)]
#[allow(clippy::large_enum_variant)]
pub enum InventoryItem {
    #[serde(rename = "transaction")]
    Transaction(Transaction),
    #[serde(rename = "block")]
    Block(Block),
}

/// Data request report to be persisted into Storage and
/// using as index the Data Request OutputPointer
// FIXME (#792): Review if this struct is needed
// It is not needed, we just need to store the transaction hash for all the commits, reveals, and
// tally. All the information can then be retrieved from the database. The data request transaction
// hash is used as the key.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct DataRequestReport {
    /// List of commitment output pointers to resolve the data request
    pub commits: Vec<CommitTransaction>,
    /// List of reveal output pointers to the commitments (contains the data request result of the witnet)
    pub reveals: Vec<RevealTransaction>,
    /// Tally output pointer (contains final result)
    pub tally: TallyTransaction,
    /// Hash of the block with the DataRequestTransaction
    pub block_hash_dr_tx: Hash,
    /// Hash of the block with the TallyTransaction
    pub block_hash_tally_tx: Hash,
    /// Current commit round starting from 1
    pub current_commit_round: u16,
    /// Current reveal round starting from 1
    pub current_reveal_round: u16,
}

impl TryFrom<DataRequestInfo> for DataRequestReport {
    type Error = failure::Error;

    fn try_from(x: DataRequestInfo) -> Result<Self, failure::Error> {
        if let (Some(tally), Some(block_hash_dr_tx), Some(block_hash_tally_tx)) =
            (x.tally, x.block_hash_dr_tx, x.block_hash_tally_tx)
        {
            Ok(DataRequestReport {
                commits: x.commits.values().cloned().collect(),
                reveals: x.reveals.values().cloned().collect(),
                tally,
                block_hash_dr_tx,
                block_hash_tally_tx,
                current_commit_round: x.current_commit_round,
                current_reveal_round: x.current_reveal_round,
            })
        } else {
            Err(DataRequestError::UnfinishedDataRequest.into())
        }
    }
}

/// List of outputs related to a data request
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct DataRequestInfo {
    /// List of commitments to resolve the data request
    pub commits: HashMap<PublicKeyHash, CommitTransaction>,
    /// List of reveals to the commitments (contains the data request witnet result)
    pub reveals: HashMap<PublicKeyHash, RevealTransaction>,
    /// Tally of data request (contains final result)
    pub tally: Option<TallyTransaction>,
    /// Hash of the block with the DataRequestTransaction
    pub block_hash_dr_tx: Option<Hash>,
    /// Hash of the block with the TallyTransaction
    pub block_hash_tally_tx: Option<Hash>,
    /// Current commit round
    pub current_commit_round: u16,
    /// Current reveal round
    pub current_reveal_round: u16,
    /// Current stage, or None if finished
    pub current_stage: Option<DataRequestStage>,
}

impl Default for DataRequestInfo {
    fn default() -> Self {
        Self {
            commits: Default::default(),
            reveals: Default::default(),
            tally: None,
            block_hash_dr_tx: None,
            block_hash_tally_tx: None,
            current_commit_round: 0,
            current_reveal_round: 0,
            current_stage: Some(DataRequestStage::COMMIT),
        }
    }
}

impl From<DataRequestReport> for DataRequestInfo {
    fn from(x: DataRequestReport) -> Self {
        Self {
            commits: x
                .commits
                .into_iter()
                .map(|c| (c.body.proof.proof.pkh(), c))
                .collect(),
            reveals: x.reveals.into_iter().map(|r| (r.body.pkh, r)).collect(),
            tally: Some(x.tally),
            block_hash_dr_tx: Some(x.block_hash_dr_tx),
            block_hash_tally_tx: Some(x.block_hash_tally_tx),
            current_commit_round: x.current_commit_round,
            current_reveal_round: x.current_reveal_round,
            current_stage: None,
        }
    }
}

/// State of data requests in progress (stored in memory)
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct DataRequestState {
    /// Data request output (contains all required information to process it)
    pub data_request: DataRequestOutput,
    /// PublicKeyHash of the data request creator
    pub pkh: PublicKeyHash,
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
    pub fn new(
        data_request: DataRequestOutput,
        pkh: PublicKeyHash,
        epoch: Epoch,
        block_hash_dr_tx: &Hash,
    ) -> Self {
        let stage = DataRequestStage::COMMIT;
        let mut info = DataRequestInfo {
            ..DataRequestInfo::default()
        };
        info.block_hash_dr_tx = Some(*block_hash_dr_tx);
        info.current_stage = Some(stage);

        Self {
            data_request,
            info,
            stage,
            epoch,
            pkh,
        }
    }

    /// Add commit
    pub fn add_commit(
        &mut self,
        pkh: PublicKeyHash,
        commit: CommitTransaction,
    ) -> Result<(), failure::Error> {
        if let DataRequestStage::COMMIT = self.stage {
            self.info.commits.insert(pkh, commit);

            Ok(())
        } else {
            Err(DataRequestError::NotCommitStage.into())
        }
    }

    /// Add reveal
    pub fn add_reveal(
        &mut self,
        pkh: PublicKeyHash,
        reveal: RevealTransaction,
    ) -> Result<(), failure::Error> {
        if let DataRequestStage::REVEAL = self.stage {
            self.info.reveals.insert(pkh, reveal);

            Ok(())
        } else {
            Err(DataRequestError::NotRevealStage.into())
        }
    }

    /// Add tally and return the data request report
    pub fn add_tally(
        mut self,
        tally: TallyTransaction,
        block_hash_tally_tx: &Hash,
    ) -> Result<DataRequestReport, failure::Error> {
        if let DataRequestStage::TALLY = self.stage {
            self.info.tally = Some(tally);
            self.info.block_hash_tally_tx = Some(*block_hash_tally_tx);

            // This try_from can only fail if the tally is None, and we have just set it to Some
            let data_request_report = DataRequestReport::try_from(self.info)?;

            Ok(data_request_report)
        } else {
            Err(DataRequestError::NotTallyStage.into())
        }
    }

    /// Advance to the next stage.
    /// Since the data requests are updated by looking at the transactions from a valid block,
    /// the only issue would be that there were no commits in that block.
    pub fn update_stage(&mut self, extra_rounds: u16) {
        self.stage = match self.stage {
            DataRequestStage::COMMIT => {
                if self.info.commits.is_empty() {
                    if self.info.current_commit_round <= extra_rounds {
                        self.info.current_commit_round += 1;
                        DataRequestStage::COMMIT
                    } else {
                        DataRequestStage::TALLY
                    }
                } else {
                    self.info.current_reveal_round = 1;
                    DataRequestStage::REVEAL
                }
            }
            DataRequestStage::REVEAL => {
                if self.info.reveals.len() < self.data_request.witnesses as usize
                    && self.info.current_reveal_round <= extra_rounds
                {
                    self.info.current_reveal_round += 1;
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
        self.info.current_stage = Some(self.stage);
    }

    /// Function to calculate the backup witnesses required
    pub fn backup_witnesses(&self) -> u16 {
        calculate_backup_witnesses(self.data_request.witnesses, self.info.current_commit_round)
    }
}

fn calculate_backup_witnesses(witnesses: u16, commit_round: u16) -> u16 {
    let exponent = u32::from(commit_round).saturating_sub(1);
    let coefficient = 2_u16.saturating_pow(exponent);

    witnesses.saturating_mul(coefficient) / 2
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

/// Unspent Outputs Pool
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct UnspentOutputsPool {
    /// Map of output pointer to a tuple of:
    /// * Value transfer output
    /// * The number of the block that included the transaction
    ///   (how many blocks were consolidated before this one).
    map: HashMap<OutputPointer, (ValueTransferOutput, u32)>,
}

impl UnspentOutputsPool {
    pub fn get<Q: ?Sized>(&self, k: &Q) -> Option<&ValueTransferOutput>
    where
        OutputPointer: std::borrow::Borrow<Q>,
        Q: std::hash::Hash + Eq,
    {
        self.map.get(k).map(|(vt, _n)| vt)
    }

    pub fn contains_key<Q: ?Sized>(&self, k: &Q) -> bool
    where
        OutputPointer: std::borrow::Borrow<Q>,
        Q: std::hash::Hash + Eq,
    {
        self.map.contains_key(k)
    }

    pub fn insert(
        &mut self,
        k: OutputPointer,
        v: ValueTransferOutput,
        block_number: u32,
    ) -> Option<(ValueTransferOutput, u32)> {
        self.map.insert(k, (v, block_number))
    }

    pub fn remove(&mut self, k: &OutputPointer) -> Option<(ValueTransferOutput, u32)> {
        self.map.remove(k)
    }

    pub fn drain(
        &mut self,
    ) -> std::collections::hash_map::Drain<OutputPointer, (ValueTransferOutput, u32)> {
        self.map.drain()
    }

    pub fn iter(
        &self,
    ) -> std::collections::hash_map::Iter<OutputPointer, (ValueTransferOutput, u32)> {
        self.map.iter()
    }

    /// Returns the number of the block that included the transaction referenced
    /// by this OutputPointer. The difference between that number and the
    /// current number of consolidated blocks is the "collateral age".
    pub fn included_in_block_number(&self, k: &OutputPointer) -> Option<Epoch> {
        self.map.get(k).map(|(_vt, n)| *n)
    }
}

/// List of unspent outputs that can be spent by this node
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct OwnUnspentOutputsPool {
    /// Map of output pointer to timestamp
    /// Those UTXOs have a timestamp value to avoid double spending
    map: HashMap<OutputPointer, u64>,
    /// Counter of UTXOs bigger than the minimum collateral
    bigger_than_min_counter: usize,
    /// Collateral minimum
    collateral_min: u64,
}

impl OwnUnspentOutputsPool {
    pub fn new(collateral_min: u64) -> Self {
        Self {
            map: HashMap::default(),
            bigger_than_min_counter: 0,
            collateral_min,
        }
    }
    pub fn get<Q: ?Sized>(&self, k: &Q) -> Option<&u64>
    where
        OutputPointer: std::borrow::Borrow<Q>,
        Q: std::hash::Hash + Eq,
    {
        self.map.get(k)
    }

    pub fn get_mut<Q: ?Sized>(&mut self, k: &Q) -> Option<&mut u64>
    where
        OutputPointer: std::borrow::Borrow<Q>,
        Q: std::hash::Hash + Eq,
    {
        self.map.get_mut(k)
    }

    pub fn contains_key<Q: ?Sized>(&self, k: &Q) -> bool
    where
        OutputPointer: std::borrow::Borrow<Q>,
        Q: std::hash::Hash + Eq,
    {
        self.map.contains_key(k)
    }

    pub fn insert(&mut self, k: OutputPointer, v: u64, value: u64) -> Option<u64> {
        let old_ts = self.map.insert(k, v);
        if old_ts.is_none() && value >= self.collateral_min {
            self.bigger_than_min_counter += 1;
        }

        old_ts
    }

    pub fn remove(&mut self, k: &OutputPointer, value: u64) -> Option<u64> {
        let ts = self.map.remove(k);

        if ts.is_some() && value >= self.collateral_min {
            self.bigger_than_min_counter -= 1;
        }

        ts
    }

    pub fn drain(&mut self) -> std::collections::hash_map::Drain<OutputPointer, u64> {
        self.bigger_than_min_counter = 0;
        self.map.drain()
    }

    pub fn iter(&self) -> std::collections::hash_map::Iter<OutputPointer, u64> {
        self.map.iter()
    }

    pub fn keys(&self) -> std::collections::hash_map::Keys<OutputPointer, u64> {
        self.map.keys()
    }

    pub fn bigger_than_min_counter(&self) -> usize {
        self.bigger_than_min_counter
    }

    pub fn len(&self) -> usize {
        self.map.len()
    }

    pub fn is_empty(&self) -> bool {
        self.map.is_empty()
    }

    /// Method to sort own_utxos by value
    pub fn sort(&self, all_utxos: &UnspentOutputsPool, bigger_first: bool) -> Vec<OutputPointer> {
        self.keys()
            .sorted_by_key(|o| {
                let value = all_utxos.get(o).map(|vt| i128::from(vt.value)).unwrap_or(0);

                if bigger_first {
                    -value
                } else {
                    value
                }
            })
            .cloned()
            .collect()
    }
}

/// Strategy to sort our own unspent outputs pool
#[derive(Clone, Debug, Serialize, Deserialize, Eq, PartialEq, Hash)]
pub enum UtxoSelectionStrategy {
    Random,
    BigFirst,
    SmallFirst,
}

impl Default for UtxoSelectionStrategy {
    fn default() -> Self {
        UtxoSelectionStrategy::Random
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct UtxoMetadata {
    pub output_pointer: OutputPointer,
    pub value: u64,
    pub timelock: u64,
    pub ready_for_collateral: bool,
}

/// Information about our own UTXOs
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct UtxoInfo {
    /// Vector of OutputPointer with their values, time_locks and if it is ready for collateral
    pub utxos: Vec<UtxoMetadata>,
    /// Counter of UTXOs bigger than the minimum collateral
    pub bigger_than_min_counter: usize,
    /// Counter of UTXOs bigger than the minimum collateral and older than collateral coinage
    pub old_utxos_counter: usize,
}

#[allow(clippy::cast_sign_loss)]
fn create_utxo_metadata(
    vto: &ValueTransferOutput,
    o: &OutputPointer,
    all_utxos: &UnspentOutputsPool,
    collateral_min: u64,
    block_number_limit: u32,
    bigger_than_min_counter: &mut usize,
    old_utxos_counter: &mut usize,
) -> UtxoMetadata {
    let now = get_timestamp() as u64;
    let timelock = if vto.time_lock >= now {
        vto.time_lock
    } else {
        0
    };
    if vto.value >= collateral_min {
        *bigger_than_min_counter += 1;
    }
    let ready_for_collateral = (vto.value >= collateral_min)
        && (all_utxos.included_in_block_number(o).unwrap() <= block_number_limit)
        && timelock == 0;
    if ready_for_collateral {
        *old_utxos_counter += 1;
    }

    UtxoMetadata {
        output_pointer: o.clone(),
        value: vto.value,
        timelock,
        ready_for_collateral,
    }
}

/// Get Utxo Information
#[allow(clippy::cast_sign_loss)]
pub fn get_utxo_info(
    pkh: Option<PublicKeyHash>,
    own_utxos: &OwnUnspentOutputsPool,
    all_utxos: &UnspentOutputsPool,
    collateral_min: u64,
    block_number_limit: u32,
) -> UtxoInfo {
    let mut bigger_than_min_counter = 0;
    let mut old_utxos_counter = 0;

    let utxos = if let Some(pkh) = pkh {
        all_utxos
            .iter()
            .filter_map(|(o, (vto, _))| {
                if vto.pkh == pkh {
                    Some(create_utxo_metadata(
                        vto,
                        o,
                        all_utxos,
                        collateral_min,
                        block_number_limit,
                        &mut bigger_than_min_counter,
                        &mut old_utxos_counter,
                    ))
                } else {
                    None
                }
            })
            .collect()
    } else {
        // Read your own UtxoInfo is cheaper than from other pkhs
        own_utxos
            .iter()
            .filter_map(|(o, _)| {
                all_utxos.get(o).map(|vto| {
                    create_utxo_metadata(
                        vto,
                        o,
                        all_utxos,
                        collateral_min,
                        block_number_limit,
                        &mut bigger_than_min_counter,
                        &mut old_utxos_counter,
                    )
                })
            })
            .collect()
    };

    UtxoInfo {
        utxos,
        bigger_than_min_counter,
        old_utxos_counter,
    }
}

pub type Blockchain = BTreeMap<Epoch, Hash>;

/// Node stats
#[derive(Debug, Default, Clone, Serialize, Deserialize, PartialEq)]
pub struct NodeStats {
    /// Number of proposed blocks
    pub block_proposed_count: u32,
    /// Number of blocks included in the block chain
    pub block_mined_count: u32,
    /// Number of times we were eligible to participate in a Data Request
    pub dr_eligibility_count: u32,
    /// Number of proposed commits
    pub commits_proposed_count: u32,
    /// Number of commits included in a data request
    pub commits_count: u32,
    /// Last block proposed
    pub last_block_proposed: Hash,
    /// Number of slashed commits
    pub slashed_count: u32,
}

/// Blockchain state (valid at a certain epoch)
#[derive(Debug, Default, Clone, Serialize, Deserialize, PartialEq)]
pub struct ChainState {
    /// Blockchain information data structure
    pub chain_info: Option<ChainInfo>,
    /// Unspent Outputs Pool
    pub unspent_outputs_pool: UnspentOutputsPool,
    /// Collection of state structures for active data requests
    pub data_request_pool: DataRequestPool,
    /// List of consolidated blocks by epoch
    pub block_chain: Blockchain,
    /// List of unspent outputs that can be spent by this node
    /// Those UTXOs have a timestamp value to avoid double spending
    pub own_utxos: OwnUnspentOutputsPool,
    /// Reputation engine
    pub reputation_engine: Option<ReputationEngine>,
    /// Node mining stats
    pub node_stats: NodeStats,
    /// Alternative public key mapping
    pub alt_keys: AltKeys,
    /// Last ARS pkhs
    pub last_ars: Vec<PublicKeyHash>,
    /// Last ARS keys vector ordered by reputation
    pub last_ars_ordered_keys: Vec<Bn256PublicKey>,
    /// Current superblock state
    pub superblock_state: SuperBlockState,
}

/// Alternative public key mapping: maps each secp256k1 public key hash to
/// different public keys in other curves
#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct AltKeys {
    /// BN256 curve
    bn256: HashMap<PublicKeyHash, Bn256PublicKey>,
}

impl AltKeys {
    /// Get the associated BN256 public key for a given identity
    pub fn get_bn256(&self, k: &PublicKeyHash) -> Option<&Bn256PublicKey> {
        self.bn256.get(k)
    }
    /// Insert a new BN256 public key, if valid,  for a given identity. If the identity
    /// already had an associated BN256 public key, it will be overwritten.
    /// Returns the old BN256 public key for this identity, if any
    pub fn insert_bn256(&mut self, k: PublicKeyHash, v: Bn256PublicKey) -> Option<Bn256PublicKey> {
        if v.is_valid() {
            self.bn256.insert(k, v)
        } else {
            log::warn!(
                "Ignoring invalid bn256 public key {:02x?} specified by PKH {:02x?}",
                v,
                k
            );
            None
        }
    }
    /// Retain only the identities that return true when applied the input function.
    /// Used to remove multiple identities at once
    pub fn retain<F>(&mut self, mut f: F)
    where
        F: FnMut(&PublicKeyHash) -> bool,
    {
        self.bn256.retain(|k, _v| f(k));
    }
    /// Insert all bn256 public keys that appeared in the block (from miner and/or committers)
    pub fn insert_keys_from_block(&mut self, block: &Block) {
        // Add miner bn256 public keys
        if let Some(value) = block.block_header.bn256_public_key.as_ref() {
            self.insert_bn256(block.block_header.proof.proof.pkh(), value.clone());
        }
        // Add bn256 public keys from commitment transactions
        for commit in &block.txns.commit_txns {
            if let Some(value) = commit.body.bn256_public_key.as_ref() {
                self.insert_bn256(commit.body.proof.proof.pkh(), value.clone());
            }
        }
    }

    /// Get bn256 keys ordered by reputation. If tie, order by pkh.
    /// These keys will be used to form the superblock
    pub fn get_rep_ordered_bn256_list(
        &self,
        reputation_set: &TotalReputationSet<PublicKeyHash, Reputation, Alpha>,
    ) -> Vec<Bn256PublicKey> {
        self.bn256
            .iter()
            .map(|(pkh, _)| *pkh)
            .sorted_by_key(|&pkh| (reputation_set.get(&pkh), pkh))
            .filter_map(|pkh| self.get_bn256(&pkh).cloned())
            .collect()
    }

    /// Get keys ordered by reputation. If tie, order by pkh.
    /// These keys will be used to order the ARS when forming a superblock
    pub fn get_rep_ordered_pkh_list(
        &self,
        reputation_set: &TotalReputationSet<PublicKeyHash, Reputation, Alpha>,
    ) -> Vec<PublicKeyHash> {
        self.bn256
            .iter()
            .map(|(pkh, _)| *pkh)
            .sorted_by_key(|&pkh| (reputation_set.get(&pkh), pkh))
            .collect()
    }
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
    pub fn get_output_from_input(&self, input: &Input) -> Option<&ValueTransferOutput> {
        let output_pointer = input.output_pointer();

        self.unspent_outputs_pool.get(&output_pointer)
    }
    /// Map a vector of inputs to a the vector of ValueTransferOutputs pointed by the inputs' output pointers
    pub fn get_outputs_from_inputs(
        &self,
        inputs: &[Input],
    ) -> Result<Vec<ValueTransferOutput>, Input> {
        let v = inputs
            .iter()
            .map(|i| self.get_output_from_input(i))
            .fuse()
            .flatten()
            .cloned()
            .collect();

        Ok(v)
    }

    /// Return the number of consolidated blocks
    pub fn block_number(&self) -> u32 {
        u32::try_from(self.block_chain.len()).unwrap()
    }
}

/// State related to the Reputation Engine
#[derive(Debug, PartialEq, Clone, Serialize, Deserialize)]
pub struct ReputationEngine {
    /// Total number of witnessing acts
    pub current_alpha: Alpha,
    /// Reputation to be split between honest identities in the next epoch
    pub extra_reputation: Reputation,
    /// Total Reputation Set
    trs: TotalReputationSet<PublicKeyHash, Reputation, Alpha>,
    /// Active Reputation Set
    ars: ActiveReputationSet<PublicKeyHash>,
    /// Cached results of self.threshold_factor
    #[serde(skip)]
    threshold_cache: RefCell<ReputationThresholdCache>,
}

impl ReputationEngine {
    /// Initial state of the Reputation Engine at epoch 0
    pub fn new(activity_period: usize) -> Self {
        Self {
            current_alpha: Alpha(0),
            extra_reputation: Reputation(0),
            trs: TotalReputationSet::default(),
            ars: ActiveReputationSet::new(activity_period),
            threshold_cache: RefCell::default(),
        }
    }

    pub fn trs(&self) -> &TotalReputationSet<PublicKeyHash, Reputation, Alpha> {
        &self.trs
    }
    pub fn ars(&self) -> &ActiveReputationSet<PublicKeyHash> {
        &self.ars
    }
    pub fn trs_mut(&mut self) -> &mut TotalReputationSet<PublicKeyHash, Reputation, Alpha> {
        self.invalidate_reputation_threshold_cache();

        &mut self.trs
    }
    pub fn ars_mut(&mut self) -> &mut ActiveReputationSet<PublicKeyHash> {
        self.invalidate_reputation_threshold_cache();

        &mut self.ars
    }

    /// Calculate total active reputation and sorted active reputation
    fn calculate_active_rep(&self) -> (u64, Vec<u32>) {
        let mut total_active_rep = 0;
        let mut sorted_identities: Vec<u32> = self
            .ars
            .active_identities()
            .map(|pkh| self.trs.get(pkh).0 + 1)
            .inspect(|rep| total_active_rep += u64::from(*rep))
            .collect();
        sorted_identities.sort_unstable_by_key(|&r| std::cmp::Reverse(r));

        (total_active_rep, sorted_identities)
    }

    /// Return a factor to increase the threshold dynamically
    pub fn threshold_factor(&self, witnesses_number: u16) -> u32 {
        self.threshold_cache
            .borrow_mut()
            .threshold_factor(witnesses_number, || self.calculate_active_rep())
    }

    /// Return the total active reputation, adding 1 point for every active identity
    pub fn total_active_reputation(&self) -> u64 {
        self.threshold_cache
            .borrow_mut()
            .total_active_reputation(|| self.calculate_active_rep())
    }

    /// Invalidate cached values of self.threshold_factor
    /// Must be called after mutating self.ars or self.trs
    pub fn invalidate_reputation_threshold_cache(&self) {
        self.threshold_cache.borrow_mut().invalidate()
    }

    /// Check if the given `pkh` is in the Active Reputation Set
    pub fn is_ars_member(&self, pkh: &PublicKeyHash) -> bool {
        self.ars.contains(pkh)
    }

    pub fn clear_threshold_cache(&self) {
        self.threshold_cache.borrow_mut().clear_threshold_cache()
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct ReputationThresholdCache {
    valid: bool,
    total_active_rep: u64,
    sorted_active_rep: Vec<u32>,
    threshold: HashMap<u16, u32>,
}

impl ReputationThresholdCache {
    fn clear_threshold_cache(&mut self) {
        self.threshold.clear();
    }
    fn initialize(&mut self, total_active_rep: u64, sorted_active_rep: Vec<u32>) {
        self.threshold.clear();
        self.total_active_rep = total_active_rep;
        self.sorted_active_rep = sorted_active_rep;
        self.valid = true;
    }

    fn invalidate(&mut self) {
        self.valid = false;
    }

    fn threshold_factor<F>(&mut self, n: u16, gen: F) -> u32
    where
        F: Fn() -> (u64, Vec<u32>),
    {
        if !self.valid {
            let (total_active_rep, sorted_active_rep) = gen();
            self.initialize(total_active_rep, sorted_active_rep);
        }

        let Self {
            total_active_rep,
            sorted_active_rep,
            threshold,
            ..
        } = self;

        *threshold.entry(n).or_insert_with(|| {
            internal_threshold_factor(
                u64::from(n),
                *total_active_rep,
                sorted_active_rep.iter().copied(),
            )
        })
    }

    fn total_active_reputation<F>(&mut self, gen: F) -> u64
    where
        F: Fn() -> (u64, Vec<u32>),
    {
        if !self.valid {
            let (total_active_rep, sorted_active_rep) = gen();
            self.initialize(total_active_rep, sorted_active_rep);
        }

        self.total_active_rep
    }
}

/// Internal function for the threshold_factor function
fn internal_threshold_factor<I>(mut n: u64, total_rep: u64, rep_sorted: I) -> u32
where
    I: Iterator<Item = u32>,
{
    if n == 0 {
        return 0;
    }

    let mut remaining_rep = total_rep;

    for top_rep in rep_sorted {
        if (u64::from(top_rep) * n) > remaining_rep {
            n -= 1;
            remaining_rep -= u64::from(top_rep);
        } else {
            let factor = if (total_rep % remaining_rep) > 0 {
                (total_rep * n / remaining_rep) + 1
            } else {
                total_rep * n / remaining_rep
            };

            return u32::try_from(factor).unwrap_or(u32::max_value());
        }
    }

    u32::max_value()
}

/// Witnessing Acts Counter
#[derive(Debug, Default, Serialize, Deserialize, Copy, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct Alpha(pub u32);

impl AddAssign for Alpha {
    fn add_assign(&mut self, rhs: Self) {
        self.0 += rhs.0
    }
}

/// Reputation value
#[derive(Debug, Default, Serialize, Deserialize, Copy, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct Reputation(pub u32);

impl AddAssign for Reputation {
    fn add_assign(&mut self, rhs: Self) {
        self.0 += rhs.0
    }
}

impl SubAssign for Reputation {
    fn sub_assign(&mut self, rhs: Self) {
        self.0 -= rhs.0
    }
}

/// Calculate the new issued reputation
pub fn reputation_issuance(
    reputation_issuance: Reputation,
    reputation_issuance_stop: Alpha,
    old_alpha: Alpha,
    new_alpha: Alpha,
) -> Reputation {
    if old_alpha >= reputation_issuance_stop {
        Reputation(0)
    } else {
        let new = std::cmp::min(reputation_issuance_stop.0, new_alpha.0);
        let alpha_diff = new - old_alpha.0;
        Reputation(alpha_diff * reputation_issuance.0)
    }
}

/// Penalization function.
/// Multiply total reputation by `penalization_factor` for each lie.
// FIXME: Allow for now, wait for https://github.com/rust-lang/rust/issues/67058 to reach stable
#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
pub fn penalize_factor(
    penalization_factor: f64,
    num_lies: u32,
) -> impl Fn(Reputation) -> Reputation {
    move |Reputation(r)| {
        Reputation((f64::from(r) * penalization_factor.powf(f64::from(num_lies))) as u32)
    }
}

fn update_utxo_inputs(utxo: &mut UnspentOutputsPool, inputs: &[Input]) {
    for input in inputs {
        // Obtain the OutputPointer of each input and remove it from the utxo_set
        let output_pointer = input.output_pointer();

        // This does not check for missing inputs
        utxo.remove(&output_pointer);
    }
}

fn update_utxo_outputs(
    utxo: &mut UnspentOutputsPool,
    outputs: &[ValueTransferOutput],
    txn_hash: Hash,
    block_number: u32,
) {
    for (index, output) in outputs.iter().enumerate() {
        // Add the new outputs to the utxo_set
        let output_pointer = OutputPointer {
            transaction_id: txn_hash,
            output_index: u32::try_from(index).unwrap(),
        };

        utxo.insert(output_pointer, output.clone(), block_number);
    }
}

/// Method to update the unspent outputs pool
pub fn generate_unspent_outputs_pool(
    unspent_outputs_pool: &UnspentOutputsPool,
    transactions: &[Transaction],
    block_number: u32,
) -> UnspentOutputsPool {
    // Create a copy of the state "unspent_outputs_pool"
    let mut unspent_outputs = unspent_outputs_pool.clone();

    for transaction in transactions {
        let txn_hash = transaction.hash();
        match transaction {
            Transaction::ValueTransfer(vt_transaction) => {
                update_utxo_inputs(&mut unspent_outputs, &vt_transaction.body.inputs);
                update_utxo_outputs(
                    &mut unspent_outputs,
                    &vt_transaction.body.outputs,
                    txn_hash,
                    block_number,
                );
            }
            Transaction::DataRequest(dr_transaction) => {
                update_utxo_inputs(&mut unspent_outputs, &dr_transaction.body.inputs);
                update_utxo_outputs(
                    &mut unspent_outputs,
                    &dr_transaction.body.outputs,
                    txn_hash,
                    block_number,
                );
            }
            Transaction::Tally(tally_transaction) => {
                update_utxo_outputs(
                    &mut unspent_outputs,
                    &tally_transaction.outputs,
                    txn_hash,
                    block_number,
                );
            }
            Transaction::Mint(mint_transaction) => {
                update_utxo_outputs(
                    &mut unspent_outputs,
                    &mint_transaction.outputs,
                    txn_hash,
                    block_number,
                );
            }
            _ => {}
        }
    }

    unspent_outputs
}

/// Constants used to convert between epoch and timestamp
#[derive(Copy, Clone, Debug)]
pub struct EpochConstants {
    /// Timestamp of checkpoint #0 (the second in which epoch #0 started)
    pub checkpoint_zero_timestamp: i64,

    /// Period between checkpoints, in seconds
    pub checkpoints_period: u16,
}

// This default is only used for tests
impl Default for EpochConstants {
    fn default() -> Self {
        Self {
            checkpoint_zero_timestamp: 0,
            // This cannot be 0 because we would divide by zero
            checkpoints_period: 1,
        }
    }
}

impl EpochConstants {
    /// Calculate the last checkpoint (current epoch) at the supplied timestamp
    pub fn epoch_at(&self, timestamp: i64) -> Result<Epoch, EpochCalculationError> {
        let zero = self.checkpoint_zero_timestamp;
        let period = self.checkpoints_period;
        let elapsed = timestamp - zero;

        Epoch::try_from(elapsed)
            .map(|epoch| epoch / Epoch::from(period))
            .map_err(|_| EpochCalculationError::CheckpointZeroInTheFuture(zero))
    }

    /// Calculate the timestamp for a checkpoint (the start of an epoch)
    pub fn epoch_timestamp(&self, epoch: Epoch) -> Result<i64, EpochCalculationError> {
        let zero = self.checkpoint_zero_timestamp;
        let period = self.checkpoints_period;

        Epoch::from(period)
            .checked_mul(epoch)
            .filter(|&x| x <= Epoch::max_value() as Epoch)
            .map(i64::from)
            .and_then(|x| x.checked_add(zero))
            .ok_or(EpochCalculationError::Overflow)
    }

    /// Calculate the timestamp for when block mining should happen.
    pub fn block_mining_timestamp(&self, epoch: Epoch) -> Result<i64, EpochCalculationError> {
        let start = self.epoch_timestamp(epoch)?;
        // TODO: analyze when should nodes start mining a block
        // Start mining at the midpoint of the epoch
        let seconds_before_next_epoch = self.checkpoints_period / 2;

        start
            .checked_add(i64::from(
                self.checkpoints_period - seconds_before_next_epoch,
            ))
            .ok_or(EpochCalculationError::Overflow)
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum SignaturesToVerify {
    VrfDr {
        proof: DataRequestEligibilityClaim,
        vrf_input: CheckpointVRF,
        dr_hash: Hash,
        target_hash: Hash,
    },
    VrfBlock {
        proof: BlockEligibilityClaim,
        vrf_input: CheckpointVRF,
        target_hash: Hash,
    },
    SecpTx {
        public_key: Secp256k1_PublicKey,
        data: Vec<u8>,
        signature: Secp256k1_Signature,
    },
    SecpBlock {
        public_key: Secp256k1_PublicKey,
        data: Vec<u8>,
        signature: Secp256k1_Signature,
    },
    SuperBlockVote {
        superblock_vote: SuperBlockVote,
    },
}

// Auxiliar functions for test
pub fn transaction_example() -> Transaction {
    let keyed_signature = vec![KeyedSignature::default()];
    let data_request_input = Input::default();
    let value_transfer_output = ValueTransferOutput::default();

    let rad_retrieve = RADRetrieve {
        url: "https://openweathermap.org/data/2.5/weather?id=2950159&appid=b6907d289e10d714a6e88b30761fae22".to_string(),
        ..RADRetrieve::default()
    };

    let rad_request = RADRequest {
        retrieve: vec![rad_retrieve.clone(), rad_retrieve],
        ..RADRequest::default()
    };
    let data_request_output = DataRequestOutput {
        data_request: rad_request,
        ..DataRequestOutput::default()
    };

    let inputs = vec![data_request_input];
    let outputs = vec![value_transfer_output];

    Transaction::DataRequest(DRTransaction::new(
        DRTransactionBody::new(inputs, outputs, data_request_output),
        keyed_signature,
    ))
}

pub fn block_example() -> Block {
    let block_header = BlockHeader::default();
    let block_sig = KeyedSignature::default();

    let mut dr_txns = vec![];
    if let Transaction::DataRequest(dr_tx) = transaction_example() {
        dr_txns.push(dr_tx);
    }

    let txns = BlockTransactions {
        data_request_txns: dr_txns,
        ..BlockTransactions::default()
    };

    Block {
        block_header,
        block_sig,
        txns,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::transaction::{CommitTransactionBody, RevealTransactionBody, VTTransactionBody};

    #[test]
    fn test_block_hashable_trait() {
        let block = block_example();
        let expected = "70e15ac70bb00f49c7a593b2423f722dca187bbae53dc2f22647063b17608c01";
        assert_eq!(block.hash().to_string(), expected);
    }

    #[test]
    fn test_transaction_hashable_trait() {
        let transaction = transaction_example();
        let expected = "7b4001b4a43b3e3dccec642791031d8094ea52164d89c2a4732d0be79ed1af83";

        // Signatures don't affect the hash of a transaction (SegWit style), thus both must be equal
        assert_eq!(transaction.hash().to_string(), expected);
        if let Transaction::DataRequest(dr_tx) = transaction {
            assert_eq!(dr_tx.body.hash().to_string(), expected);
        }
    }

    #[test]
    fn test_superblock_hashable_trait() {
        let superblock = SuperBlock {
            ars_length: 3,
            ars_root: Hash::SHA256([1; 32]),
            data_request_root: Hash::SHA256([2; 32]),
            index: 1,
            last_block: Hash::SHA256([3; 32]),
            last_block_in_previous_superblock: Hash::SHA256([4; 32]),
            tally_root: Hash::SHA256([5; 32]),
        };
        let expected = "abbe7475bbf814e266d295473928ab30f9c2c97da092a5d3162704fc19403925";
        assert_eq!(superblock.hash().to_string(), expected);
    }

    #[test]
    fn test_superblock_hashable_trait_2() {
        let superblock = SuperBlock {
            ars_length: 0x0fff_ffff_ffff,
            ars_root: Hash::SHA256([1; 32]),
            data_request_root: Hash::SHA256([2; 32]),
            index: 0x0020_2020,
            last_block: Hash::SHA256([3; 32]),
            last_block_in_previous_superblock: Hash::SHA256([4; 32]),
            tally_root: Hash::SHA256([5; 32]),
        };
        let expected = "15ac1620ee1a0743e99d9a415c7c77de6935f778189597c699c2126b1d52229b";
        assert_eq!(superblock.hash().to_string(), expected);
    }

    #[test]
    fn test_superblock_hashable_trait_3() {
        let superblock = SuperBlock {
            ars_length: 0x000a_456b_32c7,
            ars_root: Hash::SHA256([1; 32]),
            data_request_root: Hash::SHA256([2; 32]),
            index: 0x20a6_b256,
            last_block: Hash::SHA256([3; 32]),
            last_block_in_previous_superblock: Hash::SHA256([4; 32]),
            tally_root: Hash::SHA256([5; 32]),
        };
        let expected = "7a2426b925203c9e8b514fadf3b689ce3f01a98698e992b70aec4dd286590b62";
        assert_eq!(superblock.hash().to_string(), expected);
    }

    #[test]
    fn test_superblock_hashable_trait_4() {
        let dr_root =
            hex::decode("000000000000000000000000000000000000000000000000000876564fd345ea")
                .unwrap()
                .as_slice()
                .try_into()
                .unwrap();
        let tally_root =
            hex::decode("0000000000000000000000000000000000000000000000000000543feab6575c")
                .unwrap()
                .as_slice()
                .try_into()
                .unwrap();
        let last_block_hash =
            hex::decode("0000000000000000000000000000000000000000000000000000023986aecd34")
                .unwrap()
                .as_slice()
                .try_into()
                .unwrap();
        let prev_last_block_hash =
            hex::decode("0000000000000000000000000000000000000000000000000000098fe34ad3c5")
                .unwrap()
                .as_slice()
                .try_into()
                .unwrap();
        let ars_root =
            hex::decode("00000000000000000000000000000000000000000000000000000034742d5a7c")
                .unwrap()
                .as_slice()
                .try_into()
                .unwrap();

        let superblock = SuperBlock {
            ars_length: 0x000a_456b_32c7,
            ars_root: Hash::SHA256(ars_root),
            data_request_root: Hash::SHA256(dr_root),
            index: 0x0020_a6b2,
            last_block: Hash::SHA256(last_block_hash),
            last_block_in_previous_superblock: Hash::SHA256(prev_last_block_hash),
            tally_root: Hash::SHA256(tally_root),
        };
        let expected = "0a76ac7744694ebec75b56674056b8b8e08903d02f0b467e0f5ea5770718188b";
        assert_eq!(superblock.hash().to_string(), expected);
    }

    #[test]
    fn test_superblock_hashable_trait_5() {
        let dr_root =
            hex::decode("000000000000000000000000000000000000000000000000000000000000ffff")
                .unwrap()
                .as_slice()
                .try_into()
                .unwrap();
        let tally_root =
            hex::decode("0000000000000000000000000000000000000000000000000000000020a6b256")
                .unwrap()
                .as_slice()
                .try_into()
                .unwrap();
        let last_block_hash =
            hex::decode("0000000000000000000000000000000000000000000000000000023986aecd34")
                .unwrap()
                .as_slice()
                .try_into()
                .unwrap();
        let prev_last_block_hash =
            hex::decode("000000000000000000000000000000000000000000000000000000034ba23cc5")
                .unwrap()
                .as_slice()
                .try_into()
                .unwrap();
        let ars_root =
            hex::decode("00000000000000000000000000000000000000000000000000000034742d5a7c")
                .unwrap()
                .as_slice()
                .try_into()
                .unwrap();

        let superblock = SuperBlock {
            ars_length: 0x0012_34ad_3e5f,
            ars_root: Hash::SHA256(ars_root),
            data_request_root: Hash::SHA256(dr_root),
            index: 0x0765_64fd,
            last_block: Hash::SHA256(last_block_hash),
            last_block_in_previous_superblock: Hash::SHA256(prev_last_block_hash),
            tally_root: Hash::SHA256(tally_root),
        };
        let expected = "c4c70a789723ce620241ccbb2cac5c533195df32f7182e34e4a02fd5e92d8a23";
        assert_eq!(superblock.hash().to_string(), expected);
    }

    #[test]
    fn test_superblock_hashable_trait_6() {
        let dr_root =
            hex::decode("00000000000000000000000000000000000000000000000000000fffffffffff")
                .unwrap()
                .as_slice()
                .try_into()
                .unwrap();
        let tally_root =
            hex::decode("00000000000000000000000000000000000000000000000000000fffffffffff")
                .unwrap()
                .as_slice()
                .try_into()
                .unwrap();
        let last_block_hash =
            hex::decode("00000000000000000000000000000000000000000000000000000fffffffffff")
                .unwrap()
                .as_slice()
                .try_into()
                .unwrap();
        let prev_last_block_hash =
            hex::decode("00000000000000000000000000000000000000000000000000000fffffffffff")
                .unwrap()
                .as_slice()
                .try_into()
                .unwrap();
        let ars_root =
            hex::decode("00000000000000000000000000000000000000000000000000000fffffffffff")
                .unwrap()
                .as_slice()
                .try_into()
                .unwrap();

        let superblock = SuperBlock {
            ars_length: 0x0fff_ffff_ffff,
            ars_root: Hash::SHA256(ars_root),
            data_request_root: Hash::SHA256(dr_root),
            index: 0x00ff_ffff,
            last_block: Hash::SHA256(last_block_hash),
            last_block_in_previous_superblock: Hash::SHA256(prev_last_block_hash),
            tally_root: Hash::SHA256(tally_root),
        };
        let expected = "493837a41d3c53be420213398f9d42c949ad5cc7f8680cf40a691fd85fa552d6";
        assert_eq!(superblock.hash().to_string(), expected);
    }

    #[test]
    fn test_superblock_vote_bn256_signature_message() {
        // If this test fails, the bridge will also fail
        let superblock_hash = "4ee0395751a5b8d94217ba71623414721ab8dc8c1634a5c79769d5196a1b3993"
            .parse()
            .unwrap();
        let superblock_vote = SuperBlockVote::new_unsigned(superblock_hash, 1);
        let expected_bls =
            hex::decode("000000014ee0395751a5b8d94217ba71623414721ab8dc8c1634a5c79769d5196a1b3993")
                .unwrap();
        assert_eq!(superblock_vote.bn256_signature_message(), expected_bls);
    }

    #[test]
    fn test_superblock_vote_secp256k1_signature_message() {
        let superblock_hash = "4ee0395751a5b8d94217ba71623414721ab8dc8c1634a5c79769d5196a1b3993"
            .parse()
            .unwrap();
        let mut superblock_vote = SuperBlockVote::new_unsigned(superblock_hash, 1);
        superblock_vote.bn256_signature.signature =
            hex::decode("03100709d625d82c4eedf5f330d538b0cfa0dd68a5c505b6896902ed935b5cce0e")
                .unwrap();
        let expected_secp = hex::decode("000000014ee0395751a5b8d94217ba71623414721ab8dc8c1634a5c79769d5196a1b399303100709d625d82c4eedf5f330d538b0cfa0dd68a5c505b6896902ed935b5cce0e").unwrap();
        assert_eq!(superblock_vote.secp256k1_signature_message(), expected_secp);
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
        use secp256k1::SecretKey as Secp256k1_SecretKey;

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

    #[test]
    fn hash_ord() {
        // Make sure that the Ord implementation of Hash compares the hashes left to right
        let a = Hash::from_str("1111111111111111111111111111111111111111111111111111111111111111")
            .unwrap();
        let b = Hash::from_str("2111111111111111111111111111111111111111111111111111111111111110")
            .unwrap();
        assert!(a < b);
    }

    #[test]
    fn hash_from_str() {
        // 32 bytes
        let a = Hash::from_str("1111111111111111111111111111111111111111111111111111111111111111");
        assert!(a.is_ok());

        // 32.5 bytes
        assert!(matches!(
            Hash::from_str("11111111111111111111111111111111111111111111111111111111111111112"),
            Err(HashParseError::Hex(hex::FromHexError::OddLength))
        ));

        // 33 bytes
        assert!(matches!(
            Hash::from_str("111111111111111111111111111111111111111111111111111111111111111122"),
            Err(HashParseError::Hex(hex::FromHexError::InvalidStringLength))
        ));
    }

    #[test]
    fn bech32_ser_de() {
        let addr = "wit1gdm8mqlz8lxtj05w05mw63jvecyenvua7ajdk5";
        let hex = "43767d83e23fccb93e8e7d36ed464cce0999b39d";

        let addr_to_pkh = PublicKeyHash::from_bech32(Environment::Mainnet, addr).unwrap();
        let hex_to_pkh = PublicKeyHash::from_hex(hex).unwrap();
        assert_eq!(addr_to_pkh, hex_to_pkh);

        let pkh = addr_to_pkh;
        assert_eq!(pkh.bech32(Environment::Mainnet), addr);

        // If we change the environment, the prefix and the checksum change
        let addr_testnet = "twit1gdm8mqlz8lxtj05w05mw63jvecyenvuasgmfk9";
        assert_eq!(pkh.bech32(Environment::Testnet), addr_testnet);
        // But the PKH is the same as mainnet
        assert_eq!(
            PublicKeyHash::from_bech32(Environment::Testnet, addr_testnet).unwrap(),
            pkh
        );
        // Although if we try to deserialize this as a mainnet address, it will fail
        assert!(PublicKeyHash::from_bech32(Environment::Mainnet, addr_testnet).is_err());
    }

    #[test]
    fn transactions_pool_contains_commit_no_signatures() {
        let transactions_pool = TransactionsPool::default();
        let transaction = Transaction::Commit(CommitTransaction {
            body: Default::default(),
            signatures: vec![],
        });

        assert_eq!(transactions_pool.contains(&transaction), Ok(false));
    }

    #[test]
    fn transactions_pool_contains_reveal_no_signatures() {
        let transactions_pool = TransactionsPool::default();
        let transaction = Transaction::Reveal(RevealTransaction {
            body: Default::default(),
            signatures: vec![],
        });

        assert_eq!(transactions_pool.contains(&transaction), Ok(false));
    }

    #[test]
    fn transactions_pool_remove_all_transactions_with_same_output_pointer() {
        let input = Input::default();
        let vt1 = Transaction::ValueTransfer(VTTransaction::new(
            VTTransactionBody::new(vec![input.clone()], vec![]),
            vec![],
        ));
        let vt2 = Transaction::ValueTransfer(VTTransaction::new(
            VTTransactionBody::new(vec![input.clone()], vec![ValueTransferOutput::default()]),
            vec![],
        ));
        assert_ne!(vt1.hash(), vt2.hash());

        let dr1 = Transaction::DataRequest(DRTransaction::new(
            DRTransactionBody::new(vec![input.clone()], vec![], DataRequestOutput::default()),
            vec![],
        ));
        let dr2 = Transaction::DataRequest(DRTransaction::new(
            DRTransactionBody::new(
                vec![input],
                vec![ValueTransferOutput::default()],
                DataRequestOutput::default(),
            ),
            vec![],
        ));
        assert_ne!(dr1.hash(), dr2.hash());

        let mut transactions_pool = TransactionsPool::default();
        transactions_pool.insert(vt1.clone(), 1);
        transactions_pool.insert(vt2.clone(), 1);
        let t = transactions_pool.vt_remove(&vt1.hash()).unwrap();
        assert_eq!(Transaction::ValueTransfer(t), vt1);
        assert!(!transactions_pool.contains(&vt1).unwrap());
        assert!(!transactions_pool.contains(&vt2).unwrap());

        let mut transactions_pool = TransactionsPool::default();
        transactions_pool.insert(vt1.clone(), 1);
        transactions_pool.insert(dr2.clone(), 1);
        let t = transactions_pool.vt_remove(&vt1.hash()).unwrap();
        assert_eq!(Transaction::ValueTransfer(t), vt1);
        assert!(!transactions_pool.contains(&vt1).unwrap());
        assert!(!transactions_pool.contains(&dr2).unwrap());

        let mut transactions_pool = TransactionsPool::default();
        transactions_pool.insert(dr1.clone(), 1);
        transactions_pool.insert(dr2.clone(), 1);
        let t = transactions_pool.dr_remove(&dr1.hash()).unwrap();
        assert_eq!(Transaction::DataRequest(t), dr1);
        assert!(!transactions_pool.contains(&dr1).unwrap());
        assert!(!transactions_pool.contains(&dr2).unwrap());

        let mut transactions_pool = TransactionsPool::default();
        transactions_pool.insert(dr1.clone(), 1);
        transactions_pool.insert(vt2.clone(), 1);
        let t = transactions_pool.dr_remove(&dr1.hash()).unwrap();
        assert_eq!(Transaction::DataRequest(t), dr1);
        assert!(!transactions_pool.contains(&dr1).unwrap());
        assert!(!transactions_pool.contains(&vt2).unwrap());
    }

    #[test]
    fn transactions_pool_not_remove_transaction_with_different_output_pointer() {
        let input = Input::default();
        let input2 = Input::new(OutputPointer {
            output_index: 1,
            transaction_id: Hash::default(),
        });
        let vt1 = Transaction::ValueTransfer(VTTransaction::new(
            VTTransactionBody::new(vec![input.clone()], vec![]),
            vec![],
        ));
        let vt2 = Transaction::ValueTransfer(VTTransaction::new(
            VTTransactionBody::new(vec![input2.clone()], vec![ValueTransferOutput::default()]),
            vec![],
        ));
        assert_ne!(vt1.hash(), vt2.hash());

        let dr1 = Transaction::DataRequest(DRTransaction::new(
            DRTransactionBody::new(vec![input], vec![], DataRequestOutput::default()),
            vec![],
        ));
        let dr2 = Transaction::DataRequest(DRTransaction::new(
            DRTransactionBody::new(
                vec![input2],
                vec![ValueTransferOutput::default()],
                DataRequestOutput::default(),
            ),
            vec![],
        ));
        assert_ne!(dr1.hash(), dr2.hash());

        let mut transactions_pool = TransactionsPool::default();
        transactions_pool.insert(vt1.clone(), 1);
        transactions_pool.insert(vt2.clone(), 1);
        let t = transactions_pool.vt_remove(&vt1.hash()).unwrap();
        assert_eq!(Transaction::ValueTransfer(t), vt1);
        assert!(!transactions_pool.contains(&vt1).unwrap());
        assert!(transactions_pool.contains(&vt2).unwrap());

        let mut transactions_pool = TransactionsPool::default();
        transactions_pool.insert(vt1.clone(), 1);
        transactions_pool.insert(dr2.clone(), 1);
        let t = transactions_pool.vt_remove(&vt1.hash()).unwrap();
        assert_eq!(Transaction::ValueTransfer(t), vt1);
        assert!(!transactions_pool.contains(&vt1).unwrap());
        assert!(transactions_pool.contains(&dr2).unwrap());

        let mut transactions_pool = TransactionsPool::default();
        transactions_pool.insert(dr1.clone(), 1);
        transactions_pool.insert(dr2.clone(), 1);
        let t = transactions_pool.dr_remove(&dr1.hash()).unwrap();
        assert_eq!(Transaction::DataRequest(t), dr1);
        assert!(!transactions_pool.contains(&dr1).unwrap());
        assert!(transactions_pool.contains(&dr2).unwrap());

        let mut transactions_pool = TransactionsPool::default();
        transactions_pool.insert(dr1.clone(), 1);
        transactions_pool.insert(vt2.clone(), 1);
        let t = transactions_pool.dr_remove(&dr1.hash()).unwrap();
        assert_eq!(Transaction::DataRequest(t), dr1);
        assert!(!transactions_pool.contains(&dr1).unwrap());
        assert!(transactions_pool.contains(&vt2).unwrap());
    }

    #[test]
    fn transactions_pool_contains_commit_same_pkh() {
        let c1 = Hash::SHA256([1; 32]);
        let c2 = Hash::SHA256([2; 32]);
        let t1 = Transaction::Commit(CommitTransaction {
            body: CommitTransactionBody::without_collateral(
                Default::default(),
                c1,
                Default::default(),
            ),
            signatures: vec![KeyedSignature::default()],
        });
        let t2 = Transaction::Commit(CommitTransaction {
            body: CommitTransactionBody::without_collateral(
                Default::default(),
                c2,
                Default::default(),
            ),
            signatures: vec![KeyedSignature::default()],
        });
        let mut transactions_pool = TransactionsPool::default();
        assert_eq!(transactions_pool.contains(&t1), Ok(false));
        transactions_pool.insert(t1.clone(), 0);
        assert_eq!(transactions_pool.contains(&t1), Ok(true));
        assert_eq!(
            transactions_pool.contains(&t2),
            Err(TransactionError::DuplicatedCommit {
                pkh: PublicKey::default().pkh(),
                dr_pointer: Hash::default(),
            })
        );
        transactions_pool.insert(t2.clone(), 0);
        assert_eq!(transactions_pool.contains(&t2), Ok(true));
        // Check that insert overwrites
        let mut expected = TransactionsPool::default();
        expected.insert(t2, 0);
        assert_eq!(transactions_pool.co_transactions, expected.co_transactions);
    }

    #[test]
    fn transactions_pool_contains_reveal_same_pkh() {
        let r1 = vec![1];
        let r2 = vec![2];
        let t1 = Transaction::Reveal(RevealTransaction {
            body: RevealTransactionBody::new(Default::default(), r1, Default::default()),
            signatures: vec![KeyedSignature::default()],
        });
        let t2 = Transaction::Reveal(RevealTransaction {
            body: RevealTransactionBody::new(Default::default(), r2, Default::default()),
            signatures: vec![KeyedSignature::default()],
        });
        let mut transactions_pool = TransactionsPool::default();
        assert_eq!(transactions_pool.contains(&t1), Ok(false));
        transactions_pool.insert(t1.clone(), 0);
        assert_eq!(transactions_pool.contains(&t1), Ok(true));
        assert_eq!(
            transactions_pool.contains(&t2),
            Err(TransactionError::DuplicatedReveal {
                pkh: Default::default(),
                dr_pointer: Hash::default(),
            })
        );
        transactions_pool.insert(t2.clone(), 0);
        assert_eq!(transactions_pool.contains(&t2), Ok(true));
    }

    #[test]
    fn transactions_pool_insert_commit_overwrites() {
        let c1 = Hash::SHA256([1; 32]);
        let c2 = Hash::SHA256([2; 32]);
        let t1 = Transaction::Commit(CommitTransaction {
            body: CommitTransactionBody::without_collateral(
                Default::default(),
                c1,
                Default::default(),
            ),
            signatures: vec![KeyedSignature::default()],
        });
        let t2 = Transaction::Commit(CommitTransaction {
            body: CommitTransactionBody::without_collateral(
                Default::default(),
                c2,
                Default::default(),
            ),
            signatures: vec![KeyedSignature::default()],
        });
        let mut transactions_pool = TransactionsPool::default();
        transactions_pool.insert(t1, 0);
        transactions_pool.insert(t2.clone(), 0);
        let mut expected = TransactionsPool::default();
        expected.insert(t2, 0);
        assert_eq!(transactions_pool.co_transactions, expected.co_transactions);
    }

    #[test]
    fn transactions_pool_insert_reveal_overwrites() {
        let r1 = vec![1];
        let r2 = vec![2];
        let t1 = Transaction::Reveal(RevealTransaction {
            body: RevealTransactionBody::new(Default::default(), r1, Default::default()),
            signatures: vec![KeyedSignature::default()],
        });
        let t2 = Transaction::Reveal(RevealTransaction {
            body: RevealTransactionBody::new(Default::default(), r2, Default::default()),
            signatures: vec![KeyedSignature::default()],
        });

        let mut transactions_pool = TransactionsPool::default();
        transactions_pool.insert(t1, 0);
        transactions_pool.insert(t2.clone(), 0);
        let mut expected = TransactionsPool::default();
        expected.insert(t2, 0);
        assert_eq!(transactions_pool.re_transactions, expected.re_transactions);
    }

    #[test]
    fn transactions_pool_commits_are_cleared_on_remove() {
        let public_key = PublicKey::default();

        let dro = DataRequestOutput {
            witnesses: 1,
            ..Default::default()
        };
        let drb = DRTransactionBody::new(vec![], vec![], dro);
        let drt = DRTransaction::new(
            drb,
            vec![KeyedSignature {
                signature: Default::default(),
                public_key,
            }],
        );
        let dr_pointer = drt.hash();
        let mut dr_pool = DataRequestPool::default();
        dr_pool
            .add_data_request(0, drt, &Default::default())
            .unwrap();

        let c1 = Hash::SHA256([1; 32]);
        let t1 = Transaction::Commit(CommitTransaction {
            body: CommitTransactionBody::without_collateral(dr_pointer, c1, Default::default()),
            signatures: vec![KeyedSignature::default()],
        });
        let mut transactions_pool = TransactionsPool::default();
        transactions_pool.insert(t1, 0);
        assert_eq!(transactions_pool.co_transactions.len(), 1);
        assert_eq!(transactions_pool.co_hash_index.len(), 1);
        let (commits_vec, _commit_fees) = transactions_pool.remove_commits(&dr_pool);
        assert_eq!(commits_vec.len(), 1);
        assert_eq!(transactions_pool.co_transactions, HashMap::new());
        assert_eq!(transactions_pool.co_hash_index, HashMap::new());
    }

    #[test]
    fn transactions_pool_return_only_with_different_inputs() {
        let public_key = PublicKey::default();

        let dro1 = DataRequestOutput {
            witnesses: 1,
            commit_fee: 501,
            ..Default::default()
        };
        let drb1 = DRTransactionBody::new(vec![], vec![], dro1);
        let drt1 = DRTransaction::new(
            drb1,
            vec![KeyedSignature {
                signature: Default::default(),
                public_key: public_key.clone(),
            }],
        );
        let dr_pointer1 = drt1.hash();

        let dro2 = DataRequestOutput {
            witnesses: 1,
            commit_fee: 100,
            ..Default::default()
        };
        let drb2 = DRTransactionBody::new(vec![], vec![], dro2);
        let drt2 = DRTransaction::new(
            drb2,
            vec![KeyedSignature {
                signature: Default::default(),
                public_key: public_key.clone(),
            }],
        );
        let dr_pointer2 = drt2.hash();

        let dro3 = DataRequestOutput {
            witnesses: 1,
            commit_fee: 500,
            ..Default::default()
        };
        let drb3 = DRTransactionBody::new(vec![], vec![], dro3);
        let drt3 = DRTransaction::new(
            drb3,
            vec![KeyedSignature {
                signature: Default::default(),
                public_key,
            }],
        );
        let dr_pointer3 = drt3.hash();

        let mut dr_pool = DataRequestPool::default();
        dr_pool
            .add_data_request(0, drt1, &Default::default())
            .unwrap();
        dr_pool
            .add_data_request(0, drt2, &Default::default())
            .unwrap();
        dr_pool
            .add_data_request(0, drt3, &Default::default())
            .unwrap();

        let mut cb1 = CommitTransactionBody::without_collateral(
            dr_pointer1,
            Hash::SHA256([1; 32]),
            Default::default(),
        );
        cb1.collateral = vec![Input::default()];
        let c1 = CommitTransaction {
            body: cb1,
            signatures: vec![KeyedSignature::default()],
        };
        let t1 = Transaction::Commit(c1.clone());

        let mut cb2 = CommitTransactionBody::without_collateral(
            dr_pointer2,
            Hash::SHA256([2; 32]),
            Default::default(),
        );
        cb2.collateral = vec![Input {
            output_pointer: OutputPointer {
                transaction_id: Default::default(),
                output_index: 2,
            },
        }];
        let c2 = CommitTransaction {
            body: cb2,
            signatures: vec![KeyedSignature::default()],
        };
        let t2 = Transaction::Commit(c2.clone());

        // Commitment with same Input
        let mut cb3 = CommitTransactionBody::without_collateral(
            dr_pointer3,
            Hash::SHA256([3; 32]),
            Default::default(),
        );
        cb3.collateral = vec![Input::default()];
        let c3 = CommitTransaction {
            body: cb3,
            signatures: vec![KeyedSignature::default()],
        };
        let t3 = Transaction::Commit(c3.clone());

        let mut transactions_pool = TransactionsPool::default();
        transactions_pool.insert(t1, 0);
        assert_eq!(transactions_pool.co_transactions.len(), 1);
        assert_eq!(transactions_pool.co_hash_index.len(), 1);
        transactions_pool.insert(t2, 0);
        assert_eq!(transactions_pool.co_transactions.len(), 2);
        assert_eq!(transactions_pool.co_hash_index.len(), 2);
        transactions_pool.insert(t3, 0);
        assert_eq!(transactions_pool.co_transactions.len(), 3);
        assert_eq!(transactions_pool.co_hash_index.len(), 3);

        let (commits_vec, _commit_fees) = transactions_pool.remove_commits(&dr_pool);
        assert_eq!(commits_vec.len(), 2);
        // t2 does not conflict with anything so it must be present
        assert!(commits_vec.contains(&c2));
        // One of t1 or t3 must be present, but not both
        assert!(commits_vec.contains(&c1) ^ commits_vec.contains(&c3));
        assert_eq!(transactions_pool.co_transactions, HashMap::new());
        assert_eq!(transactions_pool.co_hash_index, HashMap::new());
    }

    #[test]
    fn transactions_pool_commits_are_cleared_on_remove_even_if_they_are_not_returned() {
        let public_key = PublicKey::default();

        let dro = DataRequestOutput {
            witnesses: 2,
            ..Default::default()
        };
        let drb = DRTransactionBody::new(vec![], vec![], dro);
        let drt = DRTransaction::new(
            drb,
            vec![KeyedSignature {
                signature: Default::default(),
                public_key,
            }],
        );
        let dr_pointer = drt.hash();
        let mut dr_pool = DataRequestPool::default();
        dr_pool
            .add_data_request(0, drt, &Default::default())
            .unwrap();

        let c1 = Hash::SHA256([1; 32]);
        let t1 = Transaction::Commit(CommitTransaction {
            body: CommitTransactionBody::without_collateral(dr_pointer, c1, Default::default()),
            signatures: vec![KeyedSignature::default()],
        });
        let mut transactions_pool = TransactionsPool::default();
        transactions_pool.insert(t1, 0);
        assert_eq!(transactions_pool.co_transactions.len(), 1);
        assert_eq!(transactions_pool.co_hash_index.len(), 1);
        let (commits_vec, _commit_fees) = transactions_pool.remove_commits(&dr_pool);
        // Since the number of commits is below the minimum of 1 witness specified in the `dro`,
        // remove_commits returns an empty vector
        assert_eq!(commits_vec, vec![]);
        // But the internal maps are cleared anyway, so the commit we inserted no longer exists
        assert_eq!(transactions_pool.co_transactions, HashMap::new());
        assert_eq!(transactions_pool.co_hash_index, HashMap::new());
    }

    #[test]
    fn transactions_pool_reveals_are_cleared_on_remove() {
        let public_key = PublicKey::default();

        let dro = DataRequestOutput {
            witnesses: 1,
            ..Default::default()
        };
        let drb = DRTransactionBody::new(vec![], vec![], dro);
        let drt = DRTransaction::new(
            drb,
            vec![KeyedSignature {
                signature: Default::default(),
                public_key,
            }],
        );
        let dr_pointer = drt.hash();
        let mut dr_pool = DataRequestPool::default();
        dr_pool
            .add_data_request(0, drt, &Default::default())
            .unwrap();

        let r1 = vec![1];
        let t1 = Transaction::Reveal(RevealTransaction {
            body: RevealTransactionBody::new(dr_pointer, r1, Default::default()),
            signatures: vec![KeyedSignature::default()],
        });
        let mut transactions_pool = TransactionsPool::default();
        transactions_pool.insert(t1, 0);
        assert_eq!(transactions_pool.re_transactions.len(), 1);
        assert_eq!(transactions_pool.re_hash_index.len(), 1);
        let (reveals_vec, _reveal_fees) = transactions_pool.remove_reveals(&dr_pool);
        assert_eq!(reveals_vec.len(), 1);
        assert_eq!(transactions_pool.re_transactions, HashMap::new());
        assert_eq!(transactions_pool.re_hash_index, HashMap::new());
    }

    #[test]
    fn transactions_pool_reveals_are_cleared_on_remove_and_always_returned() {
        let public_key = PublicKey::default();

        let dro = DataRequestOutput {
            witnesses: 2,
            ..Default::default()
        };
        let drb = DRTransactionBody::new(vec![], vec![], dro);
        let drt = DRTransaction::new(
            drb,
            vec![KeyedSignature {
                signature: Default::default(),
                public_key,
            }],
        );
        let dr_pointer = drt.hash();
        let mut dr_pool = DataRequestPool::default();
        dr_pool
            .add_data_request(0, drt, &Default::default())
            .unwrap();

        let r1 = vec![1];
        let t1 = Transaction::Reveal(RevealTransaction {
            body: RevealTransactionBody::new(dr_pointer, r1, Default::default()),
            signatures: vec![KeyedSignature::default()],
        });
        let mut transactions_pool = TransactionsPool::default();
        transactions_pool.insert(t1, 0);
        assert_eq!(transactions_pool.re_transactions.len(), 1);
        assert_eq!(transactions_pool.re_hash_index.len(), 1);
        let (reveals_vec, _reveal_fees) = transactions_pool.remove_reveals(&dr_pool);
        // Even though the data request asks for 2 witnesses, it returns the 1 reveal that we have
        assert_eq!(reveals_vec.len(), 1);
        assert_eq!(transactions_pool.re_transactions, HashMap::new());
        assert_eq!(transactions_pool.re_hash_index, HashMap::new());
    }

    #[test]
    fn rep_threshold_zero() {
        let rep_engine = ReputationEngine::new(1000);

        assert_eq!(rep_engine.threshold_factor(1), u32::max_value());
    }

    #[test]
    fn rep_threshold_1() {
        let mut rep_engine = ReputationEngine::new(1000);
        let id1 = PublicKeyHash { hash: [1; 20] };

        rep_engine
            .trs_mut()
            .gain(Alpha(10), vec![(id1, Reputation(99))])
            .unwrap();
        rep_engine.ars_mut().push_activity(vec![id1]);

        assert_eq!(rep_engine.threshold_factor(1), 1);
    }

    #[test]
    fn rep_threshold_2() {
        let mut rep_engine = ReputationEngine::new(1000);
        let id1 = PublicKeyHash { hash: [1; 20] };
        let id2 = PublicKeyHash { hash: [2; 20] };

        rep_engine
            .trs_mut()
            .gain(Alpha(10), vec![(id1, Reputation(99))])
            .unwrap();
        rep_engine.ars_mut().push_activity(vec![id1]);

        rep_engine
            .trs_mut()
            .gain(Alpha(10), vec![(id2, Reputation(49))])
            .unwrap();
        rep_engine.ars_mut().push_activity(vec![id2]);

        assert_eq!(rep_engine.threshold_factor(1), 1);
        assert_eq!(rep_engine.threshold_factor(2), 3);
    }

    #[test]
    fn rep_threshold_2_inverse() {
        let mut rep_engine = ReputationEngine::new(1000);
        let id1 = PublicKeyHash { hash: [1; 20] };
        let id2 = PublicKeyHash { hash: [2; 20] };

        rep_engine
            .trs_mut()
            .gain(Alpha(10), vec![(id1, Reputation(49))])
            .unwrap();
        rep_engine.ars_mut().push_activity(vec![id1]);

        rep_engine
            .trs_mut()
            .gain(Alpha(10), vec![(id2, Reputation(99))])
            .unwrap();
        rep_engine.ars_mut().push_activity(vec![id2]);

        assert_eq!(rep_engine.threshold_factor(1), 1);
        assert_eq!(rep_engine.threshold_factor(2), 3);
    }

    #[test]
    fn rep_threshold_2_sum() {
        let mut rep_engine = ReputationEngine::new(1000);
        let id1 = PublicKeyHash { hash: [1; 20] };
        let id2 = PublicKeyHash { hash: [2; 20] };

        rep_engine
            .trs_mut()
            .gain(Alpha(10), vec![(id1, Reputation(49))])
            .unwrap();
        rep_engine.ars.push_activity(vec![id1]);

        rep_engine
            .trs_mut()
            .gain(Alpha(10), vec![(id2, Reputation(99))])
            .unwrap();
        rep_engine.ars_mut().push_activity(vec![id2]);

        rep_engine
            .trs_mut()
            .gain(Alpha(10), vec![(id1, Reputation(200))])
            .unwrap();

        assert_eq!(rep_engine.threshold_factor(1), 1);
        assert_eq!(rep_engine.threshold_factor(2), 4);
    }

    #[test]
    fn rep_threshold_2_more_than_actives() {
        let mut rep_engine = ReputationEngine::new(1000);
        let id1 = PublicKeyHash { hash: [1; 20] };
        let id2 = PublicKeyHash { hash: [2; 20] };

        rep_engine
            .trs_mut()
            .gain(Alpha(10), vec![(id1, Reputation(49))])
            .unwrap();
        rep_engine.ars_mut().push_activity(vec![id1]);

        rep_engine
            .trs_mut()
            .gain(Alpha(10), vec![(id2, Reputation(99))])
            .unwrap();
        rep_engine.ars_mut().push_activity(vec![id2]);

        assert_eq!(rep_engine.threshold_factor(10), u32::max_value());
    }

    #[test]
    fn rep_threshold_2_zero_requested() {
        let mut rep_engine = ReputationEngine::new(1000);
        let id1 = PublicKeyHash { hash: [1; 20] };
        let id2 = PublicKeyHash { hash: [2; 20] };

        rep_engine
            .trs_mut()
            .gain(Alpha(10), vec![(id1, Reputation(49))])
            .unwrap();
        rep_engine.ars_mut().push_activity(vec![id1]);

        rep_engine
            .trs_mut()
            .gain(Alpha(10), vec![(id2, Reputation(99))])
            .unwrap();
        rep_engine.ars_mut().push_activity(vec![id2]);

        assert_eq!(rep_engine.threshold_factor(0), 0);
    }

    #[test]
    fn rep_threshold_specific_example() {
        let mut rep_engine = ReputationEngine::new(1000);
        let mut ids = vec![];
        for i in 0..8 {
            ids.push(PublicKeyHash::from_bytes(&[i; 20]).unwrap());
        }
        rep_engine.ars_mut().push_activity(ids.clone());

        rep_engine
            .trs_mut()
            .gain(Alpha(10), vec![(ids[0], Reputation(79))])
            .unwrap();
        rep_engine
            .trs_mut()
            .gain(Alpha(10), vec![(ids[1], Reputation(9))])
            .unwrap();
        rep_engine
            .trs_mut()
            .gain(Alpha(10), vec![(ids[2], Reputation(1))])
            .unwrap();
        rep_engine
            .trs
            .gain(Alpha(10), vec![(ids[3], Reputation(1))])
            .unwrap();
        rep_engine
            .trs_mut()
            .gain(Alpha(10), vec![(ids[4], Reputation(1))])
            .unwrap();
        rep_engine
            .trs_mut()
            .gain(Alpha(10), vec![(ids[5], Reputation(1))])
            .unwrap();

        assert_eq!(rep_engine.threshold_factor(0), 0);
        assert_eq!(rep_engine.threshold_factor(1), 1);
        assert_eq!(rep_engine.threshold_factor(2), 5);
        assert_eq!(rep_engine.threshold_factor(3), 10);
        assert_eq!(rep_engine.threshold_factor(4), 20);
        assert_eq!(rep_engine.threshold_factor(5), 30);
        assert_eq!(rep_engine.threshold_factor(6), 40);
        assert_eq!(rep_engine.threshold_factor(7), 50);
        assert_eq!(rep_engine.threshold_factor(8), 100);
        assert_eq!(rep_engine.threshold_factor(9), u32::max_value());
    }

    #[test]
    fn utxo_set_coin_age() {
        let mut p = UnspentOutputsPool::default();
        let v = || ValueTransferOutput::default();

        let k0: OutputPointer =
            "0222222222222222222222222222222222222222222222222222222222222222:0"
                .parse()
                .unwrap();
        p.insert(k0.clone(), v(), 0);
        assert_eq!(p.included_in_block_number(&k0), Some(0));

        let k1: OutputPointer =
            "1222222222222222222222222222222222222222222222222222222222222222:0"
                .parse()
                .unwrap();
        p.insert(k1.clone(), v(), 1);
        assert_eq!(p.included_in_block_number(&k1), Some(1));

        // k2 points to the same transaction as k1, so they must have the same coin age
        let k2: OutputPointer =
            "1222222222222222222222222222222222222222222222222222222222222222:1"
                .parse()
                .unwrap();
        p.insert(k2.clone(), v(), 1);
        assert_eq!(p.included_in_block_number(&k2), Some(1));

        // Removing k2 should not affect k1
        p.remove(&k2);
        assert_eq!(p.included_in_block_number(&k2), None);
        assert_eq!(p.included_in_block_number(&k1), Some(1));
        assert_eq!(p.included_in_block_number(&k0), Some(0));

        p.remove(&k1);
        assert_eq!(p.included_in_block_number(&k2), None);
        assert_eq!(p.included_in_block_number(&k1), None);
        assert_eq!(p.included_in_block_number(&k0), Some(0));

        p.remove(&k0);
        assert_eq!(p.included_in_block_number(&k0), None);

        assert_eq!(p, UnspentOutputsPool::default());
    }

    #[test]
    fn utxo_set_insert_twice() {
        // Inserting the same input twice into the UTXO set overwrites the transaction
        let mut p = UnspentOutputsPool::default();
        let v = || ValueTransferOutput::default();

        let k0: OutputPointer =
            "0222222222222222222222222222222222222222222222222222222222222222:0"
                .parse()
                .unwrap();
        p.insert(k0.clone(), v(), 0);
        p.insert(k0.clone(), v(), 0);
        assert_eq!(p.included_in_block_number(&k0), Some(0));
        // Removing once is enough
        p.remove(&k0);
        assert_eq!(p.included_in_block_number(&k0), None);
    }

    #[test]
    fn utxo_set_insert_same_transaction_different_epoch() {
        // Inserting the same transaction twice with different indexes means a different UTXO
        // so, each UTXO keeps their own block number
        let mut p = UnspentOutputsPool::default();
        let v = || ValueTransferOutput::default();

        let k0: OutputPointer =
            "0222222222222222222222222222222222222222222222222222222222222222:0"
                .parse()
                .unwrap();
        p.insert(k0.clone(), v(), 0);
        assert_eq!(p.included_in_block_number(&k0), Some(0));
        let k1: OutputPointer =
            "0222222222222222222222222222222222222222222222222222222222222222:1"
                .parse()
                .unwrap();

        p.insert(k1.clone(), v(), 1);
        assert_eq!(p.included_in_block_number(&k1), Some(1));
    }

    #[test]
    fn test_sort_own_utxos() {
        let mut vto1 = ValueTransferOutput::default();
        vto1.value = 100;
        let mut vto2 = ValueTransferOutput::default();
        vto2.value = 500;
        let mut vto3 = ValueTransferOutput::default();
        vto3.value = 200;
        let mut vto4 = ValueTransferOutput::default();
        vto4.value = 300;

        let vt = Transaction::ValueTransfer(VTTransaction::new(
            VTTransactionBody::new(vec![], vec![vto1, vto2, vto3, vto4]),
            vec![],
        ));

        let utxo_pool = generate_unspent_outputs_pool(&UnspentOutputsPool::default(), &[vt], 0);
        assert_eq!(utxo_pool.iter().len(), 4);

        let mut own_utxos = OwnUnspentOutputsPool::default();
        for (o, _) in utxo_pool.iter() {
            own_utxos.insert(o.clone(), 0, 0);
        }
        assert_eq!(own_utxos.len(), 4);

        let sorted_bigger = own_utxos.sort(&utxo_pool, true);
        let mut aux = 1000;
        for o in sorted_bigger.iter() {
            let value = utxo_pool.get(o).unwrap().value;
            assert!(value < aux);
            aux = value;
        }

        let sorted_lower = own_utxos.sort(&utxo_pool, false);
        let mut aux = 0;
        for o in sorted_lower.iter() {
            let value = utxo_pool.get(o).unwrap().value;
            assert!(value > aux);
            aux = value;
        }
    }

    #[test]
    fn test_calculate_backup_witnesses() {
        assert_eq!(calculate_backup_witnesses(10, 1), 5);
        assert_eq!(calculate_backup_witnesses(10, 2), 10);
        assert_eq!(calculate_backup_witnesses(10, 3), 20);
        assert_eq!(calculate_backup_witnesses(10, 4), 40);

        assert_eq!(calculate_backup_witnesses(11, 1), 5);
        assert_eq!(calculate_backup_witnesses(11, 2), 11);
        assert_eq!(calculate_backup_witnesses(11, 3), 22);
        assert_eq!(calculate_backup_witnesses(11, 4), 44);
    }

    #[test]
    fn test_ordered_alts_no_tie() {
        let mut trs = TotalReputationSet::new();
        let mut alt_keys = AltKeys::default();

        let p1 = PublicKeyHash::from_bytes(&[0x01 as u8; 20]).unwrap();
        let p2 = PublicKeyHash::from_bytes(&[0x03 as u8; 20]).unwrap();
        let p3 = PublicKeyHash::from_bytes(&[0x02 as u8; 20]).unwrap();

        let p1_bls =
            Bn256PublicKey::from_secret_key(&Bn256SecretKey::from_slice(&[1; 32]).unwrap())
                .unwrap();

        let p2_bls =
            Bn256PublicKey::from_secret_key(&Bn256SecretKey::from_slice(&[2; 32]).unwrap())
                .unwrap();

        let p3_bls =
            Bn256PublicKey::from_secret_key(&Bn256SecretKey::from_slice(&[3; 32]).unwrap())
                .unwrap();

        alt_keys.insert_bn256(p1, p1_bls.clone());
        alt_keys.insert_bn256(p2, p2_bls.clone());
        alt_keys.insert_bn256(p3, p3_bls.clone());

        let v4 = vec![
            (p1, Reputation(3)),
            (p2, Reputation(2)),
            (p3, Reputation(1)),
        ];

        trs.gain(Alpha(4), v4).unwrap();
        let expected_order = vec![p3_bls, p2_bls, p1_bls];
        let ordered_keys = alt_keys.get_rep_ordered_bn256_list(&trs);

        assert_eq!(expected_order, ordered_keys);
    }

    #[test]
    fn test_ordered_alts_with_tie() {
        let mut trs = TotalReputationSet::new();
        let mut alt_keys = AltKeys::default();

        let p1 = PublicKeyHash::from_bytes(&[0x01 as u8; 20]).unwrap();
        let p2 = PublicKeyHash::from_bytes(&[0x03 as u8; 20]).unwrap();
        let p3 = PublicKeyHash::from_bytes(&[0x02 as u8; 20]).unwrap();

        let p1_bls =
            Bn256PublicKey::from_secret_key(&Bn256SecretKey::from_slice(&[1; 32]).unwrap())
                .unwrap();
        let p2_bls =
            Bn256PublicKey::from_secret_key(&Bn256SecretKey::from_slice(&[2; 32]).unwrap())
                .unwrap();
        let p3_bls =
            Bn256PublicKey::from_secret_key(&Bn256SecretKey::from_slice(&[3; 32]).unwrap())
                .unwrap();

        alt_keys.insert_bn256(p1, p1_bls.clone());
        alt_keys.insert_bn256(p2, p2_bls.clone());
        alt_keys.insert_bn256(p3, p3_bls.clone());

        let v4 = vec![
            (p1, Reputation(3)),
            (p2, Reputation(1)),
            (p3, Reputation(1)),
        ];

        trs.gain(Alpha(4), v4).unwrap();
        let expected_order = vec![p3_bls, p2_bls, p1_bls];
        let ordered_keys = alt_keys.get_rep_ordered_bn256_list(&trs);

        assert_eq!(expected_order, ordered_keys);
    }

    #[test]
    fn test_ordered_alts_with_tie_2() {
        let mut trs = TotalReputationSet::new();
        let mut alt_keys = AltKeys::default();

        let p1 = PublicKeyHash::from_bytes(&[0x01 as u8; 20]).unwrap();
        let p2 = PublicKeyHash::from_bytes(&[0x03 as u8; 20]).unwrap();
        let p3 = PublicKeyHash::from_bytes(&[0x02 as u8; 20]).unwrap();
        let p4 = PublicKeyHash::from_bytes(&[0x05 as u8; 20]).unwrap();
        let p5 = PublicKeyHash::from_bytes(&[0x04 as u8; 20]).unwrap();

        let p1_bls =
            Bn256PublicKey::from_secret_key(&Bn256SecretKey::from_slice(&[1; 32]).unwrap())
                .unwrap();

        let p2_bls =
            Bn256PublicKey::from_secret_key(&Bn256SecretKey::from_slice(&[2; 32]).unwrap())
                .unwrap();

        let p3_bls =
            Bn256PublicKey::from_secret_key(&Bn256SecretKey::from_slice(&[3; 32]).unwrap())
                .unwrap();

        let p4_bls =
            Bn256PublicKey::from_secret_key(&Bn256SecretKey::from_slice(&[4; 32]).unwrap())
                .unwrap();

        let p5_bls =
            Bn256PublicKey::from_secret_key(&Bn256SecretKey::from_slice(&[5; 32]).unwrap())
                .unwrap();

        alt_keys.insert_bn256(p1, p1_bls.clone());
        alt_keys.insert_bn256(p2, p2_bls.clone());
        alt_keys.insert_bn256(p3, p3_bls.clone());
        alt_keys.insert_bn256(p4, p4_bls.clone());
        alt_keys.insert_bn256(p5, p5_bls.clone());

        let v4 = vec![
            (p1, Reputation(3)),
            (p2, Reputation(1)),
            (p3, Reputation(1)),
            (p4, Reputation(1)),
            (p5, Reputation(1)),
        ];

        trs.gain(Alpha(4), v4).unwrap();
        let expected_order = vec![p3_bls, p2_bls, p5_bls, p4_bls, p1_bls];
        let ordered_keys = alt_keys.get_rep_ordered_bn256_list(&trs);

        assert_eq!(expected_order, ordered_keys);
    }

    #[test]
    fn test_is_valid_true() {
        let bls_pk =
            Bn256PublicKey::from_secret_key(&Bn256SecretKey::from_slice(&[3; 32]).unwrap())
                .unwrap();
        assert!(bls_pk.is_valid());
    }

    #[test]
    fn test_is_valid_false_invalid_length() {
        let bls_pk =
            Bn256PublicKey::from_secret_key(&Bn256SecretKey::from_slice(&[3; 32]).unwrap())
                .unwrap();
        let bls_pk_2 = Bn256PublicKey {
            public_key: bls_pk.public_key[1..63].to_vec(),
        };
        assert_eq!(bls_pk_2.is_valid(), false);
    }

    #[test]
    fn test_is_valid_false() {
        let bls_pk = Bn256PublicKey {
            public_key: vec![1; 65],
        };
        assert_eq!(bls_pk.is_valid(), false);
    }
}
