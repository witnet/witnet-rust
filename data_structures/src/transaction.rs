use serde::{Deserialize, Serialize};

use crate::{
    chain::{
        Block, DataRequestOutput, Epoch, Hash, Hashable, Input, KeyedSignature, PublicKeyHash,
        ValueTransferOutput,
    },
    proto::{schema::witnet, ProtobufConvert},
    vrf::DataRequestEligibilityClaim,
};
use protobuf::Message;
use std::{cell::Cell, convert::TryFrom};
use witnet_crypto::{hash::calculate_sha256, merkle::FullMerkleTree};

pub trait MemoizedHashable {
    fn hashable_bytes(&self) -> Vec<u8>;
    fn memoized_hash(&self) -> &MemoHash;
}
#[derive(Debug, Default, Eq, Clone)]
pub struct MemoHash {
    hash: Cell<Option<Hash>>,
}

// PartialEq always returns true because we dont want to compare
// this field in a Transaction comparison.
impl PartialEq for MemoHash {
    fn eq(&self, _other: &Self) -> bool {
        true
    }
}

impl MemoHash {
    fn new() -> Self {
        Self {
            hash: Cell::new(None),
        }
    }

    fn get(&self) -> Option<Hash> {
        self.hash.get()
    }

    fn set(&self, h: Option<Hash>) {
        self.hash.set(h);
    }
}

/// Transaction data structure
#[derive(Debug, Eq, PartialEq, Clone, Serialize, Deserialize, ProtobufConvert)]
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
}

pub fn mint(tx: &Transaction) -> Option<&MintTransaction> {
    match tx {
        Transaction::Mint(tx) => Some(tx),
        _ => None,
    }
}

#[derive(Debug, Default, Eq, PartialEq, Clone, Serialize, Deserialize, ProtobufConvert)]
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

    /// Returns the byte size that a transaction will have on the wire
    pub fn size(&self) -> u32 {
        u32::try_from(self.to_pb().write_to_bytes().unwrap().len()).unwrap()
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

#[derive(Debug, Default, Eq, PartialEq, Clone, Serialize, Deserialize, ProtobufConvert)]
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
    pub fn new<'a, I: IntoIterator<Item = &'a H>, H: 'a + Hashable>(
        index: usize,
        leaves: I,
    ) -> TxInclusionProof {
        let mt = FullMerkleTree::sha256(leaves.into_iter().map(|t| t.hash().into()).collect());

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
}

#[derive(Debug, Default, Eq, PartialEq, Clone, Serialize, Deserialize, ProtobufConvert)]
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
    pub fn proof_of_inclusion(&self, block: &Block) -> Option<TxInclusionProof> {
        // Find the transaction in this block
        let txs = &block.txns.data_request_txns;

        txs.iter()
            .position(|x| x == self)
            .map(|tx_idx| TxInclusionProof::new(tx_idx, txs))
    }

    /// Modify the proof of inclusion adding a new level that divide a specified data
    /// from the rest of transaction
    pub fn data_proof_of_inclusion(&self, block: &Block) -> Option<TxInclusionProof> {
        self.proof_of_inclusion(block).map(|mut poi| {
            poi.add_leave(self.body.rest_poi_hash());

            poi
        })
    }
}

#[derive(Debug, Default, Eq, PartialEq, Clone, Serialize, Deserialize, ProtobufConvert)]
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
        outputs: Vec<ValueTransferOutput>,
        dr_output: DataRequestOutput,
    ) -> Self {
        DRTransactionBody {
            inputs,
            outputs,
            dr_output,
            hash: MemoHash::new(),
        }
    }

    /// Specified data to be divided in a new level in the proof of inclusion
    /// In this case data = Hash( dr_output )
    pub fn data_poi_hash(&self) -> Hash {
        self.dr_output.hash()
    }

    /// Rest of the transaction to be divided in a new level in the proof of inclusion
    /// In this case we choose the complete transaction
    pub fn rest_poi_hash(&self) -> Hash {
        calculate_sha256(&self.to_pb_bytes().unwrap()).into()
    }
}

#[derive(Debug, Default, Eq, PartialEq, Clone, Serialize, Deserialize, ProtobufConvert)]
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

#[derive(Debug, Default, Eq, PartialEq, Clone, Serialize, Deserialize, ProtobufConvert)]
#[protobuf_convert(pb = "witnet::CommitTransactionBody")]
pub struct CommitTransactionBody {
    // DRTransaction hash
    pub dr_pointer: Hash,
    // RevealTransaction Signature Hash
    pub commitment: Hash,
    // Proof of elegibility for this pkh, epoch, and data request
    pub proof: DataRequestEligibilityClaim,
    // Inputs used as collateral
    pub collateral: Vec<Input>,
    // Change from collateral
    pub outputs: Vec<ValueTransferOutput>,

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
    ) -> Self {
        CommitTransactionBody {
            dr_pointer,
            commitment,
            proof,
            collateral,
            outputs,
            hash: MemoHash::new(),
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
        }
    }
}

#[derive(Debug, Default, Eq, PartialEq, Clone, Serialize, Deserialize, ProtobufConvert)]
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

#[derive(Debug, Default, Eq, PartialEq, Clone, Serialize, Deserialize, ProtobufConvert)]
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

#[derive(Debug, Default, Eq, PartialEq, Clone, Serialize, Deserialize, ProtobufConvert)]
#[protobuf_convert(pb = "witnet::TallyTransaction")]
pub struct TallyTransaction {
    // Inputs
    pub dr_pointer: Hash, // DTTransaction hash
    // Outputs
    pub tally: Vec<u8>,
    pub outputs: Vec<ValueTransferOutput>, // Witness rewards
    pub slashed_witnesses: Vec<PublicKeyHash>,

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
        slashed_witnesses: Vec<PublicKeyHash>,
    ) -> Self {
        TallyTransaction {
            dr_pointer,
            tally,
            outputs,
            slashed_witnesses,
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
    pub fn rest_poi_hash(&self) -> Hash {
        calculate_sha256(&self.to_pb_bytes().unwrap()).into()
    }

    /// Creates a proof of inclusion.
    ///
    /// Returns None if the transaction is not included in this block.
    pub fn proof_of_inclusion(&self, block: &Block) -> Option<TxInclusionProof> {
        // Find the transaction in this block
        let txs = &block.txns.tally_txns;

        txs.iter()
            .position(|x| x == self)
            .map(|tx_idx| TxInclusionProof::new(tx_idx, txs))
    }

    /// Modify the proof of inclusion adding a new level that divide a specified data
    /// from the rest of transaction
    pub fn data_proof_of_inclusion(&self, block: &Block) -> Option<TxInclusionProof> {
        self.proof_of_inclusion(block).map(|mut poi| {
            poi.add_leave(self.rest_poi_hash());

            poi
        })
    }
}

#[derive(Debug, Default, Eq, PartialEq, Clone, Serialize, Deserialize, ProtobufConvert)]
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

    /// Try to create a MintTransaction with number of outputs equal to `utxos_required`,
    /// where every one of those outputs must be equal to or greater than `collateral_minimum`.
    /// If the reward is too small, the number of outputs may be smaller than `utxos_required`.
    /// The output value will only be smaller than `collateral_minimum` if the `reward` is smaller
    /// than `collateral_minimum`, in which case this function will create a MintTransaction with
    /// exactly one output
    pub fn with_split_utxos(
        epoch: Epoch,
        reward: u64,
        own_pkh: PublicKeyHash,
        collateral_minimum: u64,
        utxos_required: usize,
    ) -> Self {
        let mut vt_outputs = vec![];
        let mut reward = reward;
        let mut utxo_counter = 1;
        while reward >= 2 * collateral_minimum && utxo_counter < utxos_required {
            reward -= collateral_minimum;
            utxo_counter += 1;
            vt_outputs.push(ValueTransferOutput {
                pkh: own_pkh,
                value: collateral_minimum,
                time_lock: 0,
            })
        }

        vt_outputs.push(ValueTransferOutput {
            pkh: own_pkh,
            value: reward,
            time_lock: 0,
        });

        MintTransaction::new(epoch, vt_outputs)
    }

    pub fn len(&self) -> usize {
        if self.is_empty() {
            0
        } else {
            1
        }
    }

    pub fn is_empty(&self) -> bool {
        false
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
        let Hash::SHA256(data_bytes) = self.data_poi_hash();
        let Hash::SHA256(rest_bytes) = self.rest_poi_hash();

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
impl MemoizedHashable for TallyTransaction {
    fn hashable_bytes(&self) -> Vec<u8> {
        let Hash::SHA256(data_bytes) = self.data_poi_hash();
        let Hash::SHA256(rest_bytes) = self.rest_poi_hash();

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

impl Hashable for Transaction {
    fn hash(&self) -> Hash {
        match self {
            Transaction::ValueTransfer(tx) => tx.hash(),
            Transaction::DataRequest(tx) => tx.hash(),
            Transaction::Commit(tx) => tx.hash(),
            Transaction::Reveal(tx) => tx.hash(),
            Transaction::Tally(tx) => tx.hash(),
            Transaction::Mint(tx) => tx.hash(),
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
    use crate::chain::Hashable;
    use crate::transaction::*;

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
}
