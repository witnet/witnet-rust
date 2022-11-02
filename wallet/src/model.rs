//! Types that are serializable and can be returned as a response.
use std::{collections::HashMap, fmt};

use failure::_core::fmt::Formatter;
use serde::{Deserialize, Serialize};

use crate::{
    account,
    types::{number_from_string, u32_to_string, u64_to_string},
};
use witnet_data_structures::{
    chain::{DataRequestInfo, Hash, OutputPointer, PublicKeyHash, ValueTransferOutput},
    transaction::Transaction,
};
use witnet_util::timestamp::get_timestamp;

#[derive(Debug, Clone, Serialize)]
pub struct Wallet {
    pub id: String,
    pub name: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct UnlockedWallet {
    pub name: Option<String>,
    pub description: Option<String>,
    pub current_account: u32,
    pub session_id: String,
    pub available_accounts: Vec<u32>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub struct Address {
    pub address: String,
    pub index: u32,
    pub keychain: u32,
    pub account: u32,
    pub path: String,
    pub info: AddressInfo,
    #[serde(skip)]
    pub pkh: PublicKeyHash,
}

#[derive(Debug, Serialize)]
pub struct Addresses {
    pub addresses: Vec<Address>,
    pub total: u32,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub struct AddressInfo {
    pub label: Option<String>,
    pub received_payments: Vec<String>,
    #[serde(
        serialize_with = "u64_to_string",
        deserialize_with = "number_from_string"
    )]
    pub received_amount: u64,
    pub first_payment_date: Option<u64>,
    pub last_payment_date: Option<u64>,
}

/// A balance with a distinction between UTXOs that are expendable or time-locked
#[derive(Copy, Clone, Debug, Eq, Default, Deserialize, PartialEq, Serialize)]
pub struct BalanceInfo {
    /// Expendable funds
    #[serde(
        serialize_with = "u64_to_string",
        deserialize_with = "number_from_string"
    )]
    pub available: u64,
    /// Time-locked funds
    #[serde(
        serialize_with = "u64_to_string",
        deserialize_with = "number_from_string"
    )]
    pub locked: u64,
}

/// List of wallet balances (confirmed, unconfirmed and pending)
#[derive(Copy, Clone, Debug, Eq, Default, Deserialize, PartialEq, Serialize)]
pub struct WalletBalance {
    /// Total amount of wallet's funds after last confirmed superblock
    pub confirmed: BalanceInfo,
    /// Amount of local pending movements not yet indexed in a block
    #[serde(
        serialize_with = "u64_to_string",
        deserialize_with = "number_from_string"
    )]
    pub local: u64,
    /// Total amount of wallet's funds after last block
    pub unconfirmed: BalanceInfo,
}

#[derive(Debug, Serialize)]
pub struct ExtendedKeyedSignature {
    pub signature: String,
    pub public_key: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub chaincode: String,
}

#[cfg(test)]
impl Addresses {
    /// Number of addresses contained in the internal buffer.
    pub fn len(&self) -> usize {
        self.addresses.len()
    }
}

impl<I> std::ops::Index<I> for Addresses
where
    I: std::slice::SliceIndex<[Address]>,
{
    type Output = <I as std::slice::SliceIndex<[Address]>>::Output;

    fn index(&self, index: I) -> &<Vec<Address> as std::ops::Index<I>>::Output {
        self.addresses.index(index)
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct BalanceMovement {
    /// Database key for storing `BalanceMovement` objects
    #[serde(skip)]
    pub db_key: u32,
    /// Balance movement from the wallet perspective: `value = own_outputs - own_inputs`
    /// - A positive value means that the wallet received WITs from others.
    /// - A negative value means that the wallet sent WITs to others.
    #[serde(rename = "type")]
    pub kind: MovementType,
    #[serde(
        serialize_with = "u64_to_string",
        deserialize_with = "number_from_string"
    )]
    pub amount: u64,
    pub transaction: WalletTransaction,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum MovementType {
    #[serde(rename = "POSITIVE")]
    Positive,
    #[serde(rename = "NEGATIVE")]
    Negative,
}

impl fmt::Display for MovementType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            MovementType::Positive => write!(f, "positive"),
            MovementType::Negative => write!(f, "negative"),
        }
    }
}

/// Transaction linked to a balance movement in a wallet
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct WalletTransaction {
    /// Block in which the transaction is included
    pub block: Option<Beacon>,
    /// Transaction is confirmed if block is consolidated by superblock
    pub confirmed: bool,
    /// Transaction data depending on its type
    pub data: TransactionData,
    /// Transaction hash (used as identifier)
    pub hash: String,
    /// Reward to miner for including transaction in the block
    #[serde(
        serialize_with = "u64_to_string",
        deserialize_with = "number_from_string"
    )]
    pub miner_fee: u64,
    /// Date when transaction was included a block (same as block date)
    pub timestamp: u64,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum TransactionData {
    #[serde(rename = "value_transfer")]
    ValueTransfer(VtData),
    #[serde(rename = "data_request")]
    DataRequest(DrData),
    #[serde(rename = "tally")]
    Tally(TallyData),
    #[serde(rename = "mint")]
    Mint(MintData),
    #[serde(rename = "commit")]
    Commit(VtData),
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct VtData {
    pub inputs: Vec<Input>,
    pub outputs: Vec<Output>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct DrData {
    pub inputs: Vec<Input>,
    pub outputs: Vec<Output>,
    pub tally: Option<TallyReport>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct TallyData {
    pub request_transaction_hash: String,
    pub outputs: Vec<Output>,
    pub tally: TallyReport,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct MintData {
    pub outputs: Vec<Output>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct Input {
    pub address: String,
    #[serde(
        serialize_with = "u64_to_string",
        deserialize_with = "number_from_string"
    )]
    pub value: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct Output {
    pub address: String,
    pub time_lock: u64,
    #[serde(
        serialize_with = "u64_to_string",
        deserialize_with = "number_from_string"
    )]
    pub value: u64,
    pub output_type: OutputType,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct TallyReport {
    pub result: String,
    pub reveals: Vec<Reveal>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct Reveal {
    pub value: String,
    pub in_consensus: bool,
}

#[derive(Debug, Eq, PartialEq, Serialize)]
pub struct WalletTransactions {
    pub transactions: Vec<BalanceMovement>,
    #[serde(
        serialize_with = "u32_to_string",
        deserialize_with = "number_from_string"
    )]
    pub total: u32,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct OutPtr {
    pub txn_hash: Vec<u8>,
    pub output_index: u32,
}

impl OutPtr {
    /// Create a `TransactionId` from a the transaction hash.
    pub fn transaction_id(&self) -> Hash {
        let mut array_bytes = [0; 32];
        array_bytes.copy_from_slice(&self.txn_hash);

        Hash::SHA256(array_bytes)
    }
}

impl From<&OutPtr> for OutputPointer {
    fn from(out_ptr: &OutPtr) -> OutputPointer {
        OutputPointer {
            transaction_id: out_ptr.transaction_id(),
            output_index: out_ptr.output_index,
        }
    }
}

impl From<&OutputPointer> for OutPtr {
    fn from(p: &OutputPointer) -> Self {
        let txn_hash = p.transaction_id.as_ref().to_vec();
        let output_index = p.output_index;

        Self {
            txn_hash,
            output_index,
        }
    }
}

impl fmt::Display for OutPtr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&format!(
            "{}:{}",
            &self.transaction_id(),
            &self.output_index
        ))
    }
}

/// Distinguish between own internal, own external and other output types
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum OutputType {
    #[serde(rename = "EXTERNAL")]
    External,
    #[serde(rename = "INTERNAL")]
    Internal,
    #[serde(rename = "OTHER")]
    Other,
}

/// UTXO information including amount, address and time lock
#[derive(Clone, Debug, Eq, Deserialize, PartialEq, Serialize)]
pub struct OutputInfo {
    /// Amount of the UTXO
    #[serde(
        serialize_with = "u64_to_string",
        deserialize_with = "number_from_string"
    )]
    pub amount: u64,
    /// PKH receiving this balance
    pub pkh: PublicKeyHash,
    /// Timestamp in which UTXO is unlocked
    #[serde(
        serialize_with = "u64_to_string",
        deserialize_with = "number_from_string"
    )]
    pub time_lock: u64,
}

impl From<OutputInfo> for ValueTransferOutput {
    fn from(
        OutputInfo {
            amount,
            pkh,
            time_lock,
        }: OutputInfo,
    ) -> Self {
        Self {
            pkh,
            value: amount,
            time_lock,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Default, Serialize, Deserialize)]
pub struct Beacon {
    #[serde(
        serialize_with = "u32_to_string",
        deserialize_with = "number_from_string"
    )]
    pub epoch: u32,
    pub block_hash: Hash,
}

impl fmt::Display for Beacon {
    #[inline]
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "blk {} ({})", hex::encode(self.block_hash), self.epoch)
    }
}

pub type UtxoSet = HashMap<OutPtr, OutputInfo>;

/// Map of output pointer to timestamp.
/// Used to mark outputs that have been recently used in a transaction.
/// They will not be used again until this timestamp.
pub type UsedOutputs = HashMap<OutPtr, u64>;

/// Function that returns a cleaned UsedOutputs, it means, without output pointer non existing
/// in the UtxoSet and without output pointer with old timestamps
#[allow(clippy::cast_sign_loss)]
pub fn clean_used_outputs(used_outputs: &UsedOutputs, utxo_set: &UtxoSet) -> UsedOutputs {
    let mut new_hm = UsedOutputs::default();
    let now = get_timestamp() as u64;
    for (o, &ts) in used_outputs.iter() {
        if utxo_set.contains_key(o) && ts < now {
            new_hm.insert(o.clone(), ts);
        }
    }

    new_hm
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Path {
    pub account: u32,
    pub keychain: u32,
    pub index: u32,
}

impl fmt::Display for Path {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{}/{}/{}",
            account::account_keypath(self.account),
            self.keychain,
            self.index
        )
    }
}

#[derive(Debug)]
pub struct ExtendedTransaction {
    pub transaction: Transaction,
    pub metadata: Option<TransactionMetadata>,
}

#[derive(Clone, Debug)]
pub enum TransactionMetadata {
    InputValues(Vec<ValueTransferOutput>),
    Tally(Box<DataRequestInfo>),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_out_ptr_transaction_id() {
        let txn_hash = vec![0; 32];
        let output_index = 0;
        let p = OutPtr {
            txn_hash,
            output_index,
        };
        let id = p.transaction_id();
        let txn = vec![0; 32];

        assert_eq!(&txn.as_slice(), &id.as_ref());
    }
}
