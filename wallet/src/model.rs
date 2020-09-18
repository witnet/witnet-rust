//! Types that are serializable and can be returned as a response.
use std::{collections::HashMap, fmt};

use failure::_core::fmt::Formatter;
use serde::{Deserialize, Serialize};

use crate::{account, types};

#[derive(Debug, Clone, Serialize)]
pub struct Wallet {
    pub id: String,
    pub name: Option<String>,
    pub caption: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct UnlockedWallet {
    pub name: Option<String>,
    pub caption: Option<String>,
    pub current_account: u32,
    pub session_id: String,
    pub available_accounts: Vec<u32>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct Address {
    pub address: String,
    pub index: u32,
    pub keychain: u32,
    pub account: u32,
    pub path: String,
    pub info: AddressInfo,
    #[serde(skip)]
    pub pkh: types::PublicKeyHash,
}

#[derive(Debug, Serialize)]
pub struct Addresses {
    pub addresses: Vec<Address>,
    pub total: u32,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct AddressInfo {
    /// Database key for storing `AddressInfo` objects
    #[serde(skip)]
    pub db_key: String,
    pub label: Option<String>,
    pub received_payments: Vec<String>,
    pub received_amount: u64,
    pub first_payment_date: Option<i64>,
    pub last_payment_date: Option<i64>,
}

#[derive(Debug, Serialize)]
pub struct Balance {
    pub total: String,
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

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct BalanceMovement {
    /// Database key for storing `BalanceMovement` objects
    #[serde(skip)]
    pub db_key: u32,
    /// Balance movement from the wallet perspective: `value = own_outputs - own_inputs`
    /// - A positive value means that the wallet received WITs from others.
    /// - A negative value means that the wallet sent WITs to others.
    #[serde(rename = "type")]
    pub kind: MovementType,
    pub amount: u64,
    pub transaction: Transaction,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
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
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Transaction {
    /// Block in which the transaction is included
    pub block: Option<Beacon>,
    /// Transaction is confirmed if block is consolidated by superblock
    pub confirmed: bool,
    /// Transaction data depending on its type
    pub data: TransactionData,
    /// Transaction hash (used as identifier)
    pub hash: String,
    /// Reward to miner for including transaction in the block
    pub miner_fee: u64,
    /// Date when transaction was included a block (same as block date)
    pub timestamp: i64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
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

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct VtData {
    pub inputs: Vec<Input>,
    pub outputs: Vec<Output>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DrData {
    pub inputs: Vec<Input>,
    pub outputs: Vec<Output>,
    pub tally: Option<TallyReport>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TallyData {
    pub request_transaction_hash: String,
    pub outputs: Vec<Output>,
    pub tally: TallyReport,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct MintData {
    pub outputs: Vec<Output>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Input {
    pub address: String,
    pub value: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Output {
    pub address: String,
    pub time_lock: u64,
    pub value: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TallyReport {
    pub result: String,
    pub reveals: Vec<Reveal>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Reveal {
    pub value: String,
    pub in_consensus: bool,
}

#[derive(Debug, Serialize)]
pub struct Transactions {
    pub transactions: Vec<BalanceMovement>,
    pub total: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct OutPtr {
    pub txn_hash: Vec<u8>,
    pub output_index: u32,
}

impl OutPtr {
    /// Create a `TransactionId` from a the transaction hash.
    pub fn transaction_id(&self) -> types::Hash {
        let mut array_bytes = [0; 32];
        array_bytes.copy_from_slice(&self.txn_hash);

        types::Hash::SHA256(array_bytes)
    }
}

impl From<&types::OutputPointer> for OutPtr {
    fn from(p: &types::OutputPointer) -> Self {
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

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct KeyBalance {
    /// PKH receiving this balance
    pub pkh: types::PublicKeyHash,
    /// Amount of the UTXO
    pub amount: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Beacon {
    pub epoch: u32,
    pub block_hash: types::Hash,
}

impl fmt::Display for Beacon {
    #[inline]
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "blk {} ({})", hex::encode(&self.block_hash), self.epoch)
    }
}

pub type UtxoSet = HashMap<OutPtr, KeyBalance>;

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

pub struct ExtendedTransaction {
    pub transaction: types::Transaction,
    pub metadata: Option<TransactionMetadata>,
}

pub enum TransactionMetadata {
    InputValues(Vec<types::VttOutput>),
    Tally(Box<types::DataRequestInfo>),
}

#[cfg(tests)]
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

        assert_eq!(&txn_hash, id.as_ref());
    }
}
