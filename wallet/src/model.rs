use std::sync::{Arc, Mutex};

use serde::Serialize;

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

pub type WalletUnlocked = Arc<InMemoryWallet>;

pub struct InMemoryWallet {
    pub id: String,
    pub name: Option<String>,
    pub caption: Option<String>,
    pub account: Account,
    pub accounts: Vec<u32>,
    pub enc_key: types::Secret,
    pub iv: Vec<u8>,
    pub mutex: Mutex<()>,
}
