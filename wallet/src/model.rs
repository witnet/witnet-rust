//! Types that are serializable and can be returned as a response.

use serde::Serialize;

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
