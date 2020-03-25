//! Types that are serializable and can be returned as a response.
use std::collections::HashMap;
use std::convert::TryFrom;
use std::fmt;

use serde::{Deserialize, Serialize};

use crate::types;

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

#[derive(Debug, Serialize, PartialEq)]
pub struct Address {
    pub address: String,
    pub index: u32,
    pub keychain: u32,
    pub account: u32,
    pub path: String,
    pub label: Option<String>,
    pub pkh: types::PublicKeyHash,
}

#[derive(Debug, Serialize)]
pub struct Addresses {
    pub addresses: Vec<Address>,
    pub total: u32,
}

#[derive(Debug, Serialize)]
pub struct Balance {
    pub available: String,
    pub confirmed: String,
    pub unconfirmed: String,
    pub total: String,
}

#[derive(Debug, Serialize)]
pub struct ExtendedKeyedSignature {
    pub signature: String,
    pub public_key: String,
    #[serde(skip_serializing_if = "is_default")]
    pub chaincode: String,
}

#[derive(Debug, Serialize)]
pub struct Block {}

fn is_default<T: Default + PartialEq>(t: &T) -> bool {
    t == &T::default()
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

#[derive(Debug, Serialize)]
pub struct Transaction {
    pub hex_hash: String,
    /// Transaction value from the wallet perspective: `value = own_outputs - own_inputs`
    /// - A positive value means that the wallet received WITs from others.
    /// - A negative value means that the wallet sent WITs to others.
    pub value: i64,
    pub kind: TransactionType,
    pub fee: Option<u64>,
    pub block: Option<BlockInfo>,
    pub timestamp: i64,
}

#[derive(Debug, Serialize, Deserialize)]
pub enum TransactionType {
    ValueTransfer,
    DataRequest,
}

pub struct UnsupportedTransactionType(pub String);

impl TryFrom<&types::Transaction> for TransactionType {
    type Error = UnsupportedTransactionType;

    fn try_from(value: &types::Transaction) -> Result<Self, Self::Error> {
        use types::Transaction::*;

        match value {
            ValueTransfer(_) => Ok(TransactionType::ValueTransfer),
            DataRequest(_) => Ok(TransactionType::DataRequest),
            _ => Err(UnsupportedTransactionType(format!("{:?}", value))),
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub enum TransactionStatus {
    Confirmed,
    Pending,
}

#[derive(Debug, Serialize)]
pub struct Transactions {
    pub transactions: Vec<Transaction>,
    pub total: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct OutPtr {
    pub txn_hash: Vec<u8>,
    pub output_index: u32,
}

impl OutPtr {
    /// Create a `TransactionId` from a the transaction hash.
    pub fn transaction_id(&self) -> types::TransactionId {
        let mut array_bytes = [0; 32];
        array_bytes.copy_from_slice(&self.txn_hash);

        types::TransactionId::SHA256(array_bytes)
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

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct KeyBalance {
    /// PKH receiving this balance
    pub pkh: types::PublicKeyHash,
    /// Amount of the UTXO
    pub amount: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct BlockInfo {
    pub hash: Vec<u8>,
    pub epoch: u32,
}

impl fmt::Display for BlockInfo {
    #[inline]
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "blk {} ({})", hex::encode(&self.hash), self.epoch)
    }
}

pub type UtxoSet = HashMap<OutPtr, KeyBalance>;

#[derive(Debug, Serialize, Deserialize)]
pub struct Path {
    pub account: u32,
    pub keychain: u32,
    pub index: u32,
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
