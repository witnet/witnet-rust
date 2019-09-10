//! Types that are serializable and can be returned as a response.
use std::collections::HashMap;
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
    pub pkh: Vec<u8>,
}

#[derive(Debug, Serialize)]
pub struct Addresses {
    pub addresses: Vec<Address>,
    pub total: u32,
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
    pub hash: String,
    pub value: u64,
    pub kind: TransactionKind,
    pub label: Option<String>,
    pub fee: Option<u64>,
    pub block: Option<BlockInfo>,
    pub timestamp: i64,
}

#[derive(Debug, Serialize, Deserialize)]
pub enum TransactionKind {
    Debit,
    Credit,
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
    pub pkh: Vec<u8>,
    /// Amount of the UTXO
    pub amount: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
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
