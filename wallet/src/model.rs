use serde::{Deserialize, Serialize};

use witnet_crypto::key;

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
pub struct Account {
    pub index: u32,
    pub external: AccountKey,
    pub internal: AccountKey,
    pub rad: AccountKey,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct AccountKey {
    pub path: String,
    pub key: key::ExtendedSK,
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
    pub label: Option<String>,
}
