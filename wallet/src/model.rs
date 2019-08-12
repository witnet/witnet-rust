use serde::{Deserialize, Serialize};

use witnet_crypto::key;

use crate::types;

#[derive(Debug, Clone, Serialize)]
pub struct Wallet {
    pub id: String,
    pub name: Option<String>,
    pub caption: Option<String>,
}

#[derive(Clone)]
pub struct Account {
    pub index: u32,
    pub external: key::ExtendedSK,
    pub internal: key::ExtendedSK,
    pub rad: key::ExtendedSK,
}

#[derive(Debug, Serialize)]
pub struct Address {
    pub address: String,
    pub path: String,
    pub label: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct Addresses {
    pub addresses: Vec<Address>,
    pub total: u32,
}

#[derive(Debug, Serialize)]
pub struct Transaction {
    pub hash: String,
    pub value: u64,
    pub kind: TransactionKind,
}

#[derive(Debug, Serialize)]
pub enum TransactionKind {
    Debit,
    Credit,
}

#[derive(Debug, Serialize)]
pub struct Transactions {
    pub transactions: Vec<Transaction>,
    pub total: u32,
}

#[derive(Clone)]
pub struct WalletUnlocked {
    pub id: String,
    pub name: Option<String>,
    pub caption: Option<String>,
    pub account: Account,
    pub session_id: String,
    pub accounts: Vec<u32>,
    pub enc_key: types::Secret,
}

// ---------------------------- OLD

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WalletInfo {
    pub id: String,
    pub name: Option<String>,
    pub caption: Option<String>,
}

impl PartialEq<WalletInfo> for WalletInfo {
    fn eq(&self, other: &WalletInfo) -> bool {
        self.id == other.id
    }
}

#[derive(Clone, Serialize, Deserialize)]
pub struct Accounts {
    pub accounts: Vec<u32>,
    pub current: u32,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct ReceiveKey {
    pub pkh: Vec<u8>,
    pub index: u32,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct OldAddress {
    pub address: String,
    pub path: String,
    pub label: Option<String>,
}
