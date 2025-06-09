use std::convert::TryFrom;
use std::sync::{Arc, RwLock};

use protobuf::Message;
use serde::{Deserialize, Serialize};

use witnet_crypto::{
    hash::calculate_sha256, merkle::FullMerkleTree, secp256k1::Message as Secp256k1Message,
    signature::PublicKey,
};

use crate::{
    chain::{
        Block, Bn256PublicKey, DataRequestOutput, Epoch, Hash, Hashable, Input, KeyedSignature,
        PublicKeyHash, StakeOutput, ValueTransferOutput,
    },
    error::TransactionError,
    proto::{
        schema::witnet,
        versioning::{ProtocolVersion, Versioned, VersionedHashable},
        ProtobufConvert,
    },
    serialization_helpers::number_from_string,
    vrf::DataRequestEligibilityClaim,
};

// These constants were calculated in:
// https://github.com/witnet/WIPs/blob/master/wip-0007.md
pub const INPUT_SIZE: u32 = 133;
pub const OUTPUT_SIZE: u32 = 36;
pub const STAKE_OUTPUT_WEIGHT: u32 = 105;
pub const UNSTAKE_TRANSACTION_WEIGHT: u32 = 153;
pub const COMMIT_WEIGHT: u32 = 400;
pub const REVEAL_WEIGHT: u32 = 200;
pub const TALLY_WEIGHT: u32 = 100;
/// Data request complexity factor
pub const ALPHA: u32 = 1;
/// Data request return type factor
pub const BETA: u32 = 1;
/// Output multiplicative factor.
/// Value transfers that join several UTXOs are favored in detriment of value transfer that split
/// to many UTXOs
pub const GAMMA: u32 = 10;

pub trait MemoizedHashable {
    fn hashable_bytes(&self) -> Vec<u8>;
    fn memoized_hash(&self) -> &MemoHash;
}

/// A shareable wrapper for hash that may or may not be already computed.
/// This low level structure does not include the implementation for compute-on-read, as that is up
/// to the implementors of `MemoizedHashable`.
///
/// # Examples
/// ```rust
/// use witnet_data_structures::{chain::Hash, transaction::MemoHash};
///
/// let memo_hash = MemoHash::new();
/// assert_eq!(memo_hash.get(), None);
///
/// let hash = Some(Hash::SHA256([0u8; 32]));
/// memo_hash.set(hash);
/// assert_eq!(memo_hash.get(), hash);
///
/// memo_hash.set(None);
/// assert_eq!(memo_hash.get(), None);
/// ```
pub type MemoHash = Memoized<Hash>;

/// Same as MemoHash, but can be used for arbitrary data
#[derive(Clone, Debug, Default)]
pub struct Memoized<T> {
    hash: Arc<RwLock<Option<T>>>,
}

// PartialEq always returns true because we dont want to compare
// this field in a Transaction comparison.
impl<T> PartialEq for Memoized<T> {
    fn eq(&self, _other: &Self) -> bool {
        true
    }
}

// Force `Eq` implementation
impl<T> Eq for Memoized<T> {}

impl<T> std::hash::Hash for Memoized<T> {
    fn hash<H: std::hash::Hasher>(&self, _state: &mut H) {
        // Memoized does not affect the hash
    }
}

impl<T> Memoized<T> {
    /// Initialize a new `Memoized` set to `None` (not computed yet)
    pub fn new() -> Self {
        Self {
            hash: Arc::new(RwLock::new(None)),
        }
    }

    /// Get the hash, if already computed.
    pub fn get(&self) -> Option<T>
    where
        T: Copy,
    {
        *self
            .hash
            .read()
            .expect("read locks should only fail if poisoned")
    }

    /// Get the hash, if already computed.
    pub fn get_cloned(&self) -> Option<T>
    where
        T: Clone,
    {
        self.hash
            .read()
            .expect("read locks should only fail if poisoned")
            .clone()
    }

    /// Set or replace the hash.
    pub fn set(&self, h: Option<T>) {
        let mut lock = self
            .hash
            .write()
            .expect("Write locks should only fail if poisoned");
        *lock = h;
    }
}

/// Transaction data structure
#[derive(Debug, Eq, PartialEq, Clone, Serialize, Deserialize, ProtobufConvert, Hash)]
#[protobuf_convert(pb = "witnet::Transaction")]
// FIXME(#649): Remove clippy skip error
#[allow(clippy::large_enum_variant)]
pub enum Transaction {
    ValueTransfer(VTTransaction),
    DataRequest(DRTransaction),
    Commit(CommitTransaction),
    Reveal(RevealTransaction),
    Tally(TallyTransaction),
    Mint(MintTransaction),
    Stake(StakeTransaction),
    Unstake(UnstakeTransaction),
}

impl From<VTTransaction> for Transaction {
    fn from(transaction: VTTransaction) -> Self {
        Self::ValueTransfer(transaction)
    }
}

impl From<DRTransaction> for Transaction {
    fn from(transaction: DRTransaction) -> Self {
        Self::DataRequest(transaction)
    }
}

impl From<CommitTransaction> for Transaction {
    fn from(transaction: CommitTransaction) -> Self {
        Self::Commit(transaction)
    }
}

impl From<RevealTransaction> for Transaction {
    fn from(transaction: RevealTransaction) -> Self {
        Self::Reveal(transaction)
    }
}

impl From<TallyTransaction> for Transaction {
    fn from(transaction: TallyTransaction) -> Self {
        Self::Tally(transaction)
    }
}

impl From<MintTransaction> for Transaction {
    fn from(transaction: MintTransaction) -> Self {
        Self::Mint(transaction)
    }
}

impl From<StakeTransaction> for Transaction {
    fn from(transaction: StakeTransaction) -> Self {
        Self::Stake(transaction)
    }
}

impl From<UnstakeTransaction> for Transaction {
    fn from(transaction: UnstakeTransaction) -> Self {
        Self::Unstake(transaction)
    }
}

impl AsRef<Transaction> for Transaction {
    fn as_ref(&self) -> &Self {
        self
    }
}

impl Transaction {
    /// Returns the byte size that a transaction will have on the wire
    pub fn size(&self) -> u32 {
        u32::try_from(self.to_pb().write_to_bytes().unwrap().len()).unwrap()
    }

    /// Get the weight of the transaction
    pub fn weight(&self) -> u32 {
        match self {
            Transaction::ValueTransfer(vt_txn) => vt_txn.weight(),
            Transaction::DataRequest(dr_txn) => dr_txn.weight(),
            _ => 0,
        }
    }
}

pub fn mint(tx: &Transaction) -> Option<&MintTransaction> {
    match tx {
        Transaction::Mint(tx) => Some(tx),
        _ => None,
    }
}

#[derive(Debug, Default, Eq, PartialEq, Clone, Serialize, Deserialize, ProtobufConvert, Hash)]
#[protobuf_convert(pb = "witnet::VTTransaction")]
pub struct VTTransaction {
    pub body: VTTransactionBody,
    pub signatures: Vec<KeyedSignature>,
}

impl VTTransaction {
    /// Creates a new value transfer transaction.
    pub fn new(body: VTTransactionBody, signatures: Vec<KeyedSignature>) -> Self {
        VTTransaction { body, signatures }
    }

    /// Returns the weight of a value transfer transaction.
    /// This is the weight that will be used to calculate
    /// how many transactions can fit inside one block
    pub fn weight(&self) -> u32 {
        self.body.weight()
    }

    /// Create a special value transfer transaction that is only valid inside the genesis block,
    /// because it is used to create value.
    ///
    /// Note that in order to be valid:
    /// * The transaction must have at least one output
    /// * All the outputs must have some value (value cannot be 0)
    pub fn genesis(outputs: Vec<ValueTransferOutput>) -> Self {
        Self::new(VTTransactionBody::new(vec![], outputs), vec![])
    }
}

#[derive(Debug, Default, Eq, PartialEq, Clone, Serialize, Deserialize, ProtobufConvert, Hash)]
#[protobuf_convert(pb = "witnet::VTTransactionBody")]
pub struct VTTransactionBody {
    pub inputs: Vec<Input>,
    pub outputs: Vec<ValueTransferOutput>,

    #[protobuf_convert(skip)]
    #[serde(skip)]
    hash: MemoHash,
}

impl VTTransactionBody {
    /// Creates a new value transfer transaction body.
    pub fn new(inputs: Vec<Input>, outputs: Vec<ValueTransferOutput>) -> Self {
        VTTransactionBody {
            inputs,
            outputs,
            hash: MemoHash::new(),
        }
    }

    pub fn value(&self) -> u64 {
        self.outputs
            .iter()
            .map(ValueTransferOutput::value)
            .reduce(|acc, value| acc + value)
            .unwrap_or_default()
    }

    /// Value Transfer transaction weight. It is calculated as:
    ///
    /// ```text
    /// VT_weight = N*INPUT_SIZE + M*OUTPUT_SIZE*gamma
    /// ```
    pub fn weight(&self) -> u32 {
        let inputs_len = u32::try_from(self.inputs.len()).unwrap_or(u32::MAX);
        let outputs_len = u32::try_from(self.outputs.len()).unwrap_or(u32::MAX);

        let inputs_weight = inputs_len.saturating_mul(INPUT_SIZE);
        let outputs_weight = outputs_len
            .saturating_mul(OUTPUT_SIZE)
            .saturating_mul(GAMMA);

        inputs_weight.saturating_add(outputs_weight)
    }
}

/// Proof of transaction inclusion in a block.
#[derive(Debug, Default, Eq, PartialEq, Clone, Serialize, Deserialize)]
pub struct TxInclusionProof {
    /// Index of the element in the merkle-tree.
    /// This is not the index of the transaction in the list of transactions.
    pub index: usize,
    /// List of hashes needed to proof inclusion, ordered from bottom to top.
    pub lemma: Vec<Hash>,
}

impl TxInclusionProof {
    /// New inclusion proof given index and list of all the transactions in the
    /// block, in the same order.
    pub fn new<'a, I: IntoIterator<Item = &'a H>, H: 'a + VersionedHashable>(
        index: usize,
        leaves: I,
        protocol_version: ProtocolVersion,
    ) -> TxInclusionProof {
        Self::new_with_hashes(
            index,
            leaves
                .into_iter()
                .map(|t| t.versioned_hash(protocol_version)),
        )
    }

    /// Create a TX inclusion proof assuming the inputs are already Hashes
    pub fn new_with_hashes<I: IntoIterator<Item = Hash>>(
        index: usize,
        leaves: I,
    ) -> TxInclusionProof {
        let mt = FullMerkleTree::sha256(leaves.into_iter().map(|t| t.into()).collect());

        // The index is valid, so this operation cannot fail
        let proof = mt.inclusion_proof(index).unwrap();

        TxInclusionProof {
            index: proof.proof_index(),
            lemma: proof.lemma().iter().map(|sha| (*sha).into()).collect(),
        }
    }

    /// Add a new level in the TxInclusionProof
    pub fn add_leave(&mut self, leave: Hash) {
        self.index <<= 1;
        self.lemma.insert(0, leave);
    }

    /// Concatenate two PoIs by extending the siblings of the first with the second
    /// The index gets updated as: first_index += second_index * 2**len(first_lemma)
    pub fn concat(&mut self, second_poi: TxInclusionProof) {
        self.index |= second_poi.index << self.lemma.len();
        self.lemma.extend_from_slice(&second_poi.lemma);
    }
}

#[derive(Debug, Default, Eq, PartialEq, Clone, Serialize, Deserialize, ProtobufConvert, Hash)]
#[protobuf_convert(pb = "witnet::DRTransaction")]
pub struct DRTransaction {
    pub body: DRTransactionBody,
    pub signatures: Vec<KeyedSignature>,
}

impl DRTransaction {
    /// Creates a new data request transaction.
    pub fn new(body: DRTransactionBody, signatures: Vec<KeyedSignature>) -> Self {
        DRTransaction { body, signatures }
    }

    /// Creates a proof of inclusion.
    ///
    /// Returns None if the transaction is not included in this block.
    pub fn proof_of_inclusion(
        &self,
        block: &Block,
        protocol_version: ProtocolVersion,
    ) -> Option<TxInclusionProof> {
        // Find the transaction in this block
        let txs = &block.txns.data_request_txns;

        txs.iter()
            .position(|x| x == self)
            .map(|tx_idx| TxInclusionProof::new(tx_idx, txs, protocol_version))
    }

    /// Returns the weight of a data request transaction.
    /// This is the weight that will be used to calculate
    /// how many transactions can fit inside one block
    pub fn weight(&self) -> u32 {
        self.body.weight()
    }

    /// Modify the proof of inclusion adding a new level that divide a specified data
    /// from the rest of transaction
    pub fn data_proof_of_inclusion(&self, block: &Block) -> Option<TxInclusionProof> {
        self.proof_of_inclusion(block, ProtocolVersion::default())
            .map(|mut poi| {
                poi.add_leave(self.body.rest_poi_hash(ProtocolVersion::default()));

                poi
            })
    }
}

#[derive(Debug, Default, Eq, PartialEq, Clone, Serialize, Deserialize, ProtobufConvert, Hash)]
#[protobuf_convert(pb = "witnet::DRTransactionBody")]
pub struct DRTransactionBody {
    pub inputs: Vec<Input>,
    pub outputs: Vec<ValueTransferOutput>,
    pub dr_output: DataRequestOutput,

    #[protobuf_convert(skip)]
    #[serde(skip)]
    hash: MemoHash,
}
impl DRTransactionBody {
    /// Creates a new data request transaction body.
    pub fn new(
        inputs: Vec<Input>,
        dr_output: DataRequestOutput,
        outputs: Vec<ValueTransferOutput>,
    ) -> Self {
        DRTransactionBody {
            inputs,
            outputs,
            dr_output,
            hash: MemoHash::new(),
        }
    }

    pub fn value(&self) -> Result<u64, TransactionError> {
        let dr_value = self.dr_output.checked_total_value()?;
        let change_value = self
            .outputs
            .iter()
            .map(ValueTransferOutput::value)
            .reduce(|acc, value| acc + value)
            .unwrap_or_default();

        Ok(dr_value + change_value)
    }

    /// Data Request Transaction weight. It is calculated as:
    ///
    /// ```text
    /// DR_weight = DR_output_size*alpha + W*COMMIT + W*REVEAL*beta + TALLY*beta + N*INPUT_SIZE + (W + M)*OUTPUT_SIZE
    /// ```
    pub fn weight(&self) -> u32 {
        let inputs_len = u32::try_from(self.inputs.len()).unwrap_or(u32::MAX);
        let outputs_len = u32::try_from(self.outputs.len()).unwrap_or(u32::MAX);
        let dr_output_size = self.dr_output.weight().saturating_mul(ALPHA);
        let dr_extra_weight = self.dr_output.extra_weight();
        let inputs_weight = inputs_len.saturating_mul(INPUT_SIZE);
        let outputs_weight = outputs_len.saturating_mul(OUTPUT_SIZE);

        dr_output_size
            .saturating_add(dr_extra_weight)
            .saturating_add(inputs_weight)
            .saturating_add(outputs_weight)
    }

    /// Specified data to be divided in a new level in the proof of inclusion
    /// In this case data = Hash( dr_output )
    pub fn data_poi_hash(&self, protocol_version: ProtocolVersion) -> Hash {
        self.dr_output.versioned_hash(protocol_version)
    }

    /// Rest of the transaction to be divided in a new level in the proof of inclusion
    /// In this case we choose the complete transaction
    pub fn rest_poi_hash(&self, protocol_version: ProtocolVersion) -> Hash {
        calculate_sha256(&self.to_versioned_pb_bytes(protocol_version).unwrap()).into()
    }
}

#[derive(Debug, Default, Eq, PartialEq, Clone, Serialize, Deserialize, ProtobufConvert, Hash)]
#[protobuf_convert(pb = "witnet::CommitTransaction")]
pub struct CommitTransaction {
    pub body: CommitTransactionBody,
    pub signatures: Vec<KeyedSignature>,
}

impl CommitTransaction {
    /// Creates a new commit transaction.
    pub fn new(body: CommitTransactionBody, signatures: Vec<KeyedSignature>) -> Self {
        CommitTransaction { body, signatures }
    }
}

#[derive(Debug, Default, Eq, PartialEq, Clone, Serialize, Deserialize, ProtobufConvert, Hash)]
#[protobuf_convert(pb = "witnet::CommitTransactionBody")]
pub struct CommitTransactionBody {
    // DRTransaction hash
    pub dr_pointer: Hash,
    // RevealTransaction Signature Hash
    pub commitment: Hash,
    // Proof of eligibility for this pkh, epoch, and data request
    pub proof: DataRequestEligibilityClaim,
    // Inputs used as collateral
    pub collateral: Vec<Input>,
    // Change from collateral. The output pkh must be the same as the inputs,
    // and there can only be one output
    pub outputs: Vec<ValueTransferOutput>,
    // BLS public key (curve bn256)
    pub bn256_public_key: Option<Bn256PublicKey>,

    #[protobuf_convert(skip)]
    #[serde(skip)]
    hash: MemoHash,
}

impl CommitTransactionBody {
    /// Creates a new commit transaction body.
    pub fn new(
        dr_pointer: Hash,
        commitment: Hash,
        proof: DataRequestEligibilityClaim,
        collateral: Vec<Input>,
        outputs: Vec<ValueTransferOutput>,
        bn256_public_key: Option<Bn256PublicKey>,
    ) -> Self {
        CommitTransactionBody {
            dr_pointer,
            commitment,
            proof,
            collateral,
            outputs,
            hash: MemoHash::new(),
            bn256_public_key,
        }
    }
    /// Old `Self::new` still used in tests
    pub fn without_collateral(
        dr_pointer: Hash,
        commitment: Hash,
        proof: DataRequestEligibilityClaim,
    ) -> Self {
        CommitTransactionBody {
            dr_pointer,
            commitment,
            proof,
            collateral: vec![],
            outputs: vec![],
            hash: MemoHash::new(),
            bn256_public_key: None,
        }
    }
}

#[derive(Debug, Default, Eq, PartialEq, Clone, Serialize, Deserialize, ProtobufConvert, Hash)]
#[protobuf_convert(pb = "witnet::RevealTransaction")]
pub struct RevealTransaction {
    pub body: RevealTransactionBody,
    pub signatures: Vec<KeyedSignature>,
}

impl RevealTransaction {
    /// Creates a new reveal transaction.
    pub fn new(body: RevealTransactionBody, signatures: Vec<KeyedSignature>) -> Self {
        RevealTransaction { body, signatures }
    }
}

#[derive(Debug, Default, Eq, PartialEq, Clone, Serialize, Deserialize, ProtobufConvert, Hash)]
#[protobuf_convert(pb = "witnet::RevealTransactionBody")]
pub struct RevealTransactionBody {
    // Inputs
    pub dr_pointer: Hash, // DTTransaction hash
    // Outputs
    pub reveal: Vec<u8>,
    pub pkh: PublicKeyHash, // where to receive reward

    #[protobuf_convert(skip)]
    #[serde(skip)]
    hash: MemoHash,
}

impl RevealTransactionBody {
    /// Creates a new reveal transaction body.
    pub fn new(dr_pointer: Hash, reveal: Vec<u8>, pkh: PublicKeyHash) -> Self {
        RevealTransactionBody {
            dr_pointer,
            reveal,
            pkh,
            hash: MemoHash::new(),
        }
    }
}

#[derive(Debug, Default, Eq, PartialEq, Clone, Serialize, Deserialize, ProtobufConvert, Hash)]
#[protobuf_convert(pb = "witnet::TallyTransaction")]
pub struct TallyTransaction {
    /// DRTransaction hash
    pub dr_pointer: Hash,
    /// Tally result
    pub tally: Vec<u8>,
    /// Witness rewards
    pub outputs: Vec<ValueTransferOutput>,
    /// Addresses that are out of consensus (non revealers included)
    pub out_of_consensus: Vec<PublicKeyHash>,
    /// Addresses that commit a RadonError (or considered as an Error due to a RadonError consensus)
    pub error_committers: Vec<PublicKeyHash>,

    #[protobuf_convert(skip)]
    #[serde(skip)]
    hash: MemoHash,
}

impl TallyTransaction {
    /// Creates a new tally transaction.
    pub fn new(
        dr_pointer: Hash,
        tally: Vec<u8>,
        outputs: Vec<ValueTransferOutput>,
        out_of_consensus: Vec<PublicKeyHash>,
        error_committers: Vec<PublicKeyHash>,
    ) -> Self {
        TallyTransaction {
            dr_pointer,
            tally,
            outputs,
            out_of_consensus,
            error_committers,
            hash: MemoHash::new(),
        }
    }

    /// Specified data to be divided in a new level in the proof of inclusion
    /// In this case data = Hash( dr_pointer || tally )
    pub fn data_poi_hash(&self) -> Hash {
        let Hash::SHA256(dr_pointer_bytes) = self.dr_pointer;
        let data = [&dr_pointer_bytes, &self.tally[..]].concat();
        calculate_sha256(&data).into()
    }

    /// Rest of the transaction to be divided in a new level in the proof of inclusion
    /// In this case we choose the complete transaction
    pub fn rest_poi_hash(&self, protocol_version: ProtocolVersion) -> Hash {
        calculate_sha256(&self.to_versioned_pb_bytes(protocol_version).unwrap()).into()
    }

    /// Creates a proof of inclusion.
    ///
    /// Returns None if the transaction is not included in this block.
    pub fn proof_of_inclusion(
        &self,
        block: &Block,
        protocol_version: ProtocolVersion,
    ) -> Option<TxInclusionProof> {
        // Find the transaction in this block
        let txs = &block.txns.tally_txns;

        txs.iter()
            .position(|x| x == self)
            .map(|tx_idx| TxInclusionProof::new(tx_idx, txs, protocol_version))
    }

    /// Modify the proof of inclusion adding a new level that divide a specified data
    /// from the rest of transaction
    pub fn data_proof_of_inclusion(
        &self,
        block: &Block,
        protocol_version: ProtocolVersion,
    ) -> Option<TxInclusionProof> {
        self.proof_of_inclusion(block, protocol_version)
            .map(|mut poi| {
                poi.add_leave(self.rest_poi_hash(protocol_version));

                poi
            })
    }
}

#[derive(Debug, Default, Eq, PartialEq, Clone, Serialize, Deserialize, ProtobufConvert, Hash)]
#[protobuf_convert(pb = "witnet::MintTransaction")]
pub struct MintTransaction {
    pub epoch: Epoch,
    pub outputs: Vec<ValueTransferOutput>,

    #[protobuf_convert(skip)]
    #[serde(skip)]
    hash: MemoHash,
}

impl MintTransaction {
    /// Creates a new mint transaction.
    pub fn new(epoch: Epoch, outputs: Vec<ValueTransferOutput>) -> Self {
        MintTransaction {
            epoch,
            outputs,
            hash: MemoHash::new(),
        }
    }

    /// This method creates a MintTransaction with a reward split between the node and an
    /// external address. The external_percentage must be lower than or equal to 100.
    /// If external_address is None all the reward goes to own_pkh.
    pub fn with_external_address(
        epoch: Epoch,
        reward: u64,
        own_pkh: PublicKeyHash,
        external_address: Option<PublicKeyHash>,
        external_percentage: u8,
    ) -> Self {
        let mut reward = reward;
        let mut vt_outputs = vec![];
        let mut external_reward = 0;
        let mut external_pkh = PublicKeyHash::default();
        if let Some(pkh) = external_address {
            // In case of a specified PKH, the reward will be distributed between the node's PKH
            // and the external one, where the external will get `reward * external_percentage`.
            external_reward = reward.saturating_mul(u64::from(external_percentage)) / 100;
            reward -= external_reward;
            external_pkh = pkh;
        }
        // If `external_percentage` is `100`, the external address will receive the entire
        // reward, and the output assigning tokens to the node is not needed.
        if reward > 0 {
            vt_outputs.push(ValueTransferOutput {
                pkh: own_pkh,
                value: reward,
                time_lock: 0,
            });
        }
        // If `external_percentage` is `0` or there is no address specified as 'external_address',
        // the node address will receive the entire reward, and the output assigning tokens to
        // the external address is not needed.
        if external_reward > 0 {
            vt_outputs.push(ValueTransferOutput {
                pkh: external_pkh,
                value: external_reward,
                time_lock: 0,
            })
        }

        MintTransaction::new(epoch, vt_outputs)
    }

    /// Return the number of `MintTransactions` (always 1)
    pub fn len(&self) -> usize {
        1
    }

    /// Returns false because the `MintTransaction` cannot be empty
    pub fn is_empty(&self) -> bool {
        false
    }
}

#[derive(Debug, Default, Eq, PartialEq, Clone, Serialize, Deserialize, ProtobufConvert, Hash)]
#[protobuf_convert(pb = "witnet::StakeTransaction")]
pub struct StakeTransaction {
    pub body: StakeTransactionBody,
    pub signatures: Vec<KeyedSignature>,
}

impl StakeTransaction {
    // Creates a new stake transaction.
    pub fn new(body: StakeTransactionBody, signatures: Vec<KeyedSignature>) -> Self {
        StakeTransaction { body, signatures }
    }

    /// Returns the weight of a stake transaction.
    /// This is the weight that will be used to calculate how many transactions can fit inside one
    /// block
    pub fn weight(&self) -> u32 {
        self.body.weight()
    }
}

#[derive(Debug, Default, Eq, PartialEq, Clone, Serialize, Deserialize, ProtobufConvert, Hash)]
#[protobuf_convert(pb = "witnet::StakeTransactionBody")]
pub struct StakeTransactionBody {
    pub inputs: Vec<Input>,
    pub output: StakeOutput,
    pub change: Option<ValueTransferOutput>,

    #[protobuf_convert(skip)]
    #[serde(skip)]
    hash: MemoHash,
}

impl StakeTransactionBody {
    pub fn authorization_is_valid(&self) -> Result<(), failure::Error> {
        let msg = Secp256k1Message::from_digest(self.output.key.withdrawer.as_secp256k1_msg());
        let public_key = PublicKey::from_slice(&self.output.authorization.public_key.bytes)?;

        self.output
            .authorization
            .signature
            .verify(&msg, &public_key)
    }

    /// Construct a `StakeTransactionBody` from a list of inputs and one `StakeOutput`.
    pub fn new(
        inputs: Vec<Input>,
        output: StakeOutput,
        change: Option<ValueTransferOutput>,
    ) -> Self {
        StakeTransactionBody {
            inputs,
            output,
            change,
            ..Default::default()
        }
    }

    pub fn value(&self) -> u64 {
        let stake_value = self.output.value;
        let change_value = &self
            .change
            .as_ref()
            .map(ValueTransferOutput::value)
            .unwrap_or_default();

        stake_value + change_value
    }

    /// Stake transaction weight. It is calculated as:
    ///
    /// ```text
    /// ST_weight = N*INPUT_SIZE+M*OUTPUT_SIZE+STAKE_OUTPUT
    ///
    /// ```
    pub fn weight(&self) -> u32 {
        let inputs_len = u32::try_from(self.inputs.len()).unwrap_or(u32::MAX);
        let inputs_weight = inputs_len.saturating_mul(INPUT_SIZE);
        let change_weight = if self.change.is_some() {
            OUTPUT_SIZE
        } else {
            0
        };

        inputs_weight
            .saturating_add(change_weight)
            .saturating_add(STAKE_OUTPUT_WEIGHT)
    }
}

#[derive(Debug, Default, Eq, PartialEq, Clone, Serialize, Deserialize, ProtobufConvert, Hash)]
#[protobuf_convert(pb = "witnet::UnstakeTransaction")]
pub struct UnstakeTransaction {
    pub body: UnstakeTransactionBody,
    pub signature: KeyedSignature,
}
impl UnstakeTransaction {
    // Creates a new unstake transaction.
    pub fn new(body: UnstakeTransactionBody, signature: KeyedSignature) -> Self {
        UnstakeTransaction { body, signature }
    }

    /// Returns the weight of a unstake transaction.
    /// This is the weight that will be used to calculate
    /// how many transactions can fit inside one block
    pub fn weight(&self) -> u32 {
        self.body.weight()
    }
}

#[derive(Debug, Default, Eq, PartialEq, Clone, Serialize, Deserialize, ProtobufConvert, Hash)]
#[protobuf_convert(pb = "witnet::UnstakeTransactionBody")]
pub struct UnstakeTransactionBody {
    pub operator: PublicKeyHash,
    pub withdrawal: ValueTransferOutput,
    #[serde(deserialize_with = "number_from_string")]
    pub fee: u64,
    #[serde(deserialize_with = "number_from_string")]
    pub nonce: u64,

    #[protobuf_convert(skip)]
    #[serde(skip)]
    hash: MemoHash,
}

impl UnstakeTransactionBody {
    /// Creates a new stake transaction body.
    pub fn new(
        operator: PublicKeyHash,
        withdrawal: ValueTransferOutput,
        fee: u64,
        nonce: u64,
    ) -> Self {
        UnstakeTransactionBody {
            operator,
            withdrawal,
            fee,
            nonce,
            ..Default::default()
        }
    }

    pub fn value(&self) -> u64 {
        self.withdrawal.value
    }

    /// Stake transaction weight. It is calculated as:
    ///
    /// ```text
    /// ST_weight = 153
    ///
    /// ```
    pub fn weight(&self) -> u32 {
        UNSTAKE_TRANSACTION_WEIGHT
    }
}

impl MemoizedHashable for VTTransactionBody {
    fn hashable_bytes(&self) -> Vec<u8> {
        self.to_pb_bytes().unwrap()
    }

    fn memoized_hash(&self) -> &MemoHash {
        &self.hash
    }
}
impl MemoizedHashable for DRTransactionBody {
    fn hashable_bytes(&self) -> Vec<u8> {
        let Hash::SHA256(data_bytes) = self.data_poi_hash(ProtocolVersion::V1_7);
        let Hash::SHA256(rest_bytes) = self.rest_poi_hash(ProtocolVersion::V1_7);

        [data_bytes, rest_bytes].concat()
    }

    fn memoized_hash(&self) -> &MemoHash {
        &self.hash
    }
}
impl MemoizedHashable for CommitTransactionBody {
    fn hashable_bytes(&self) -> Vec<u8> {
        self.to_pb_bytes().unwrap()
    }

    fn memoized_hash(&self) -> &MemoHash {
        &self.hash
    }
}
impl MemoizedHashable for RevealTransactionBody {
    fn hashable_bytes(&self) -> Vec<u8> {
        self.to_pb_bytes().unwrap()
    }

    fn memoized_hash(&self) -> &MemoHash {
        &self.hash
    }
}
impl MemoizedHashable for StakeTransactionBody {
    fn hashable_bytes(&self) -> Vec<u8> {
        self.to_pb_bytes().unwrap()
    }

    fn memoized_hash(&self) -> &MemoHash {
        &self.hash
    }
}
impl MemoizedHashable for UnstakeTransactionBody {
    fn hashable_bytes(&self) -> Vec<u8> {
        self.to_pb_bytes().unwrap()
    }

    fn memoized_hash(&self) -> &MemoHash {
        self.hash.set(None);
        &self.hash
    }
}
impl MemoizedHashable for TallyTransaction {
    fn hashable_bytes(&self) -> Vec<u8> {
        let Hash::SHA256(data_bytes) = self.data_poi_hash();
        let Hash::SHA256(rest_bytes) = self.rest_poi_hash(ProtocolVersion::default());

        [data_bytes, rest_bytes].concat()
    }

    fn memoized_hash(&self) -> &MemoHash {
        &self.hash
    }
}
impl MemoizedHashable for MintTransaction {
    fn hashable_bytes(&self) -> Vec<u8> {
        self.to_pb_bytes().unwrap()
    }

    fn memoized_hash(&self) -> &MemoHash {
        &self.hash
    }
}

impl Hashable for VTTransaction {
    fn hash(&self) -> Hash {
        self.body.hash()
    }
}
impl Hashable for DRTransaction {
    fn hash(&self) -> Hash {
        self.body.hash()
    }
}
impl Hashable for CommitTransaction {
    fn hash(&self) -> Hash {
        self.body.hash()
    }
}
impl Hashable for RevealTransaction {
    fn hash(&self) -> Hash {
        self.body.hash()
    }
}

impl Hashable for StakeTransaction {
    fn hash(&self) -> Hash {
        self.body.hash()
    }
}
impl Hashable for UnstakeTransaction {
    fn hash(&self) -> Hash {
        self.body.hash()
    }
}

impl Hashable for Transaction {
    fn hash(&self) -> Hash {
        match self {
            Transaction::ValueTransfer(tx) => tx.hash(),
            Transaction::DataRequest(tx) => tx.hash(),
            Transaction::Commit(tx) => tx.hash(),
            Transaction::Reveal(tx) => tx.hash(),
            Transaction::Tally(tx) => tx.hash(),
            Transaction::Mint(tx) => tx.hash(),
            Transaction::Stake(tx) => tx.hash(),
            Transaction::Unstake(tx) => tx.hash(),
        }
    }
}

impl<T> Hashable for T
where
    T: MemoizedHashable,
{
    fn hash(&self) -> Hash {
        let hash = self.memoized_hash();

        hash.get().unwrap_or_else(|| {
            let bytes = calculate_sha256(&self.hashable_bytes()).into();
            hash.set(Some(bytes));
            bytes
        })
    }
}

#[cfg(test)]
mod tests {
    use crate::{
        chain::{
            DataRequestOutput, Hashable, Input, KeyedSignature, PublicKeyHash, ValueTransferOutput,
        },
        transaction::*,
    };

    #[test]
    fn test_memoized_hashable_trait() {
        let vt_tx = VTTransaction::default();
        assert_eq!(vt_tx.body.hash.get(), None);
        let hash = vt_tx.hash();
        assert_eq!(vt_tx.body.hash.get(), Some(hash));

        let dr_tx = DRTransaction::default();
        assert_eq!(dr_tx.body.hash.get(), None);
        let hash = dr_tx.hash();
        assert_eq!(dr_tx.body.hash.get(), Some(hash));

        let co_tx = CommitTransaction::default();
        assert_eq!(co_tx.body.hash.get(), None);
        let hash = co_tx.hash();
        assert_eq!(co_tx.body.hash.get(), Some(hash));

        let re_tx = RevealTransaction::default();
        assert_eq!(re_tx.body.hash.get(), None);
        let hash = re_tx.hash();
        assert_eq!(re_tx.body.hash.get(), Some(hash));

        let ta_tx = TallyTransaction::default();
        assert_eq!(ta_tx.hash.get(), None);
        let hash = ta_tx.hash();
        assert_eq!(ta_tx.hash.get(), Some(hash));

        let mint_tx = MintTransaction::default();
        assert_eq!(mint_tx.hash.get(), None);
        let hash = mint_tx.hash();
        assert_eq!(mint_tx.hash.get(), Some(hash));
    }

    #[test]
    fn memohash_eq() {
        let tx1 = VTTransaction::default();
        let tx2 = VTTransaction::default();
        assert_eq!(tx1, tx2);

        // Check that after memoizing the hash, the transactions are still considered to be equal.
        let _tx_hash = tx1.hash();
        assert_eq!(tx1, tx2);
    }

    #[test]
    fn test_data_request_hash_protocol_version() {
        let dr_tx = DRTransaction::default();
        assert_eq!(dr_tx.hash(), dr_tx.versioned_hash(ProtocolVersion::V1_7),);
    }

    #[test]
    fn test_commit_hash_protocol_version() {
        let c_tx = CommitTransaction::default();
        assert_eq!(c_tx.hash(), c_tx.versioned_hash(ProtocolVersion::V1_7),);
    }

    #[test]
    fn test_reveal_hash_protocol_version() {
        let r_tx = RevealTransaction::default();
        assert_eq!(r_tx.hash(), r_tx.versioned_hash(ProtocolVersion::V1_7),);
    }

    #[test]
    fn test_tally_hash_protocol_version() {
        let t_tx = TallyTransaction::default();
        assert_eq!(t_tx.hash(), t_tx.versioned_hash(ProtocolVersion::V1_7),);
    }

    #[test]
    fn test_mint_with_external_address() {
        let epoch = 1;
        let own_pkh = PublicKeyHash::from_bytes(&[1; 20]).unwrap();
        let external_pkh = PublicKeyHash::from_bytes(&[2; 20]).unwrap();
        let external_percentage = 30;
        let reward = 500;

        // Without external address
        let expected_mint = MintTransaction::new(
            epoch,
            vec![ValueTransferOutput {
                pkh: own_pkh,
                value: 500,
                time_lock: 0,
            }],
        );
        let mint = MintTransaction::with_external_address(
            epoch,
            reward,
            own_pkh,
            None,
            external_percentage,
        );
        assert_eq!(expected_mint, mint);
        let mint =
            MintTransaction::with_external_address(epoch, reward, own_pkh, Some(external_pkh), 0);
        assert_eq!(expected_mint, mint);

        // Optimistic rollup case
        let expected_mint = MintTransaction::new(
            epoch,
            vec![
                ValueTransferOutput {
                    pkh: own_pkh,
                    value: 350,
                    time_lock: 0,
                },
                ValueTransferOutput {
                    pkh: external_pkh,
                    value: 150,
                    time_lock: 0,
                },
            ],
        );
        let mint = MintTransaction::with_external_address(
            epoch,
            reward,
            own_pkh,
            Some(external_pkh),
            external_percentage,
        );
        assert_eq!(expected_mint, mint);

        // Non exactly division case
        let expected_mint = MintTransaction::new(
            epoch,
            vec![
                ValueTransferOutput {
                    pkh: own_pkh,
                    value: 351,
                    time_lock: 0,
                },
                ValueTransferOutput {
                    pkh: external_pkh,
                    value: 150,
                    time_lock: 0,
                },
            ],
        );
        let mint = MintTransaction::with_external_address(
            epoch,
            reward + 1,
            own_pkh,
            Some(external_pkh),
            external_percentage,
        );
        assert_eq!(expected_mint, mint);

        // 100% external
        let expected_mint = MintTransaction::new(
            epoch,
            vec![ValueTransferOutput {
                pkh: external_pkh,
                value: 500,
                time_lock: 0,
            }],
        );
        let mint =
            MintTransaction::with_external_address(epoch, reward, own_pkh, Some(external_pkh), 100);
        assert_eq!(expected_mint, mint);
    }

    // VT_weight = N*INPUT_SIZE + M*OUTPUT_SIZE*gamma
    #[test]
    fn test_vt_weight() {
        let vt_body =
            VTTransactionBody::new(vec![Input::default()], vec![ValueTransferOutput::default()]);
        let vt_tx = VTTransaction::new(vt_body, vec![KeyedSignature::default()]);
        assert_eq!(INPUT_SIZE + OUTPUT_SIZE * GAMMA, vt_tx.weight());
        assert_eq!(493, vt_tx.weight());

        let vt_body = VTTransactionBody::new(
            vec![Input::default(); 2],
            vec![ValueTransferOutput::default()],
        );
        let vt_tx = VTTransaction::new(vt_body, vec![KeyedSignature::default()]);
        assert_eq!(2 * INPUT_SIZE + OUTPUT_SIZE * GAMMA, vt_tx.weight());
        assert_eq!(626, vt_tx.weight());

        let vt_body = VTTransactionBody::new(
            vec![Input::default()],
            vec![ValueTransferOutput::default(); 2],
        );
        let vt_tx = VTTransaction::new(vt_body, vec![KeyedSignature::default()]);
        assert_eq!(INPUT_SIZE + 2 * OUTPUT_SIZE * GAMMA, vt_tx.weight());
        assert_eq!(853, vt_tx.weight());
    }

    #[test]
    fn test_dr_weight() {
        let dro = DataRequestOutput {
            witnesses: 2,
            ..Default::default()
        };
        let dr_body = DRTransactionBody::new(
            vec![Input::default()],
            dro.clone(),
            vec![ValueTransferOutput::default()],
        );
        let dr_tx = DRTransaction::new(dr_body, vec![KeyedSignature::default()]);
        let dr_weight = INPUT_SIZE + OUTPUT_SIZE + dro.weight();
        assert_eq!(
            dr_weight * ALPHA
                + 2 * COMMIT_WEIGHT
                + 2 * REVEAL_WEIGHT * BETA
                + TALLY_WEIGHT * BETA
                + 2 * OUTPUT_SIZE,
            dr_tx.weight()
        );
        assert_eq!(1587, dr_tx.weight());

        let dro = DataRequestOutput {
            witnesses: 5,
            ..Default::default()
        };
        let dr_body = DRTransactionBody::new(
            vec![Input::default()],
            dro.clone(),
            vec![ValueTransferOutput::default()],
        );
        let dr_tx = DRTransaction::new(dr_body, vec![KeyedSignature::default()]);
        let dr_weight = INPUT_SIZE + OUTPUT_SIZE + dro.weight();
        assert_eq!(
            dr_weight * ALPHA
                + 5 * COMMIT_WEIGHT
                + 5 * REVEAL_WEIGHT * BETA
                + TALLY_WEIGHT * BETA
                + 5 * OUTPUT_SIZE,
            dr_tx.weight()
        );
        assert_eq!(3495, dr_tx.weight());
    }
}
