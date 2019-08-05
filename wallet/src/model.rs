//! Type definitions common to all actors and are intended to be
//! returned to clients.
use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct Wallet {
    pub id: String,
    pub name: Option<String>,
    pub caption: Option<String>,
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
#[serde(rename_all = "camelCase")]
pub struct AccountBalance {
    pub wallet_id: String,
    pub account: u32,
    pub balance: u64,
}
