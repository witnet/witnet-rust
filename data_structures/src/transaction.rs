use serde::{Deserialize, Serialize};

use crate::{
    chain::{
        DataRequestOutput, Epoch, Hash, Hashable, Input, KeyedSignature, PublicKeyHash,
        ValueTransferOutput,
    },
    proto::{schema::witnet, ProtobufConvert},
    vrf::DataRequestEligibilityClaim,
};
use protobuf::Message;
use std::cell::Cell;
use witnet_crypto::hash::calculate_sha256;

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

impl AsRef<Transaction> for Transaction {
    fn as_ref(&self) -> &Self {
        self
    }
}

impl Transaction {
    /// Returns the byte size that a transaction will have on the wire
    pub fn size(&self) -> u32 {
        self.to_pb().write_to_bytes().unwrap().len() as u32
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
        self.to_pb().write_to_bytes().unwrap().len() as u32
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
    // Inputs
    // TODO: Discussion about collateral
    //pub collateral: Vec<Input>, // ValueTransferOutput
    pub dr_pointer: Hash, // DTTransaction hash
    // Outputs
    pub commitment: Hash,
    // Proof of elegibility for this pkh, epoch, and data request
    pub proof: DataRequestEligibilityClaim,

    #[protobuf_convert(skip)]
    #[serde(skip)]
    hash: MemoHash,
}

impl CommitTransactionBody {
    /// Creates a new commit transaction body.
    pub fn new(dr_pointer: Hash, commitment: Hash, proof: DataRequestEligibilityClaim) -> Self {
        CommitTransactionBody {
            dr_pointer,
            commitment,
            proof,
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

    #[protobuf_convert(skip)]
    #[serde(skip)]
    hash: MemoHash,
}

impl TallyTransaction {
    /// Creates a new tally transaction.
    pub fn new(dr_pointer: Hash, tally: Vec<u8>, outputs: Vec<ValueTransferOutput>) -> Self {
        TallyTransaction {
            dr_pointer,
            tally,
            outputs,
            hash: MemoHash::new(),
        }
    }
}

#[derive(Debug, Default, Eq, PartialEq, Clone, Serialize, Deserialize, ProtobufConvert)]
#[protobuf_convert(pb = "witnet::MintTransaction")]
pub struct MintTransaction {
    pub epoch: Epoch,
    // FIXME(#650): Modify outputs -> output
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

    pub fn len(&self) -> usize {
        if self.is_empty() {
            0
        } else {
            1
        }
    }

    pub fn is_empty(&self) -> bool {
        self.outputs.is_empty()
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
        self.to_pb_bytes().unwrap()
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
        self.to_pb_bytes().unwrap()
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
