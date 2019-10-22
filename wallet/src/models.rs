use serde::Serialize;

use crate::schema::accounts;
use crate::schema::wallets;

#[derive(Debug, Clone, Queryable, Serialize)]
pub struct WalletInfo {
    pub id: i32,
    pub name: String,
    pub caption: Option<String>,
}

#[derive(Insertable)]
#[table_name = "accounts"]
pub struct NewAccount<'a> {
    pub idx: i32,
    pub internal_key: &'a [u8],
    pub internal_chain_code: &'a [u8],
    pub external_key: &'a [u8],
    pub external_chain_code: &'a [u8],
}

#[derive(Insertable)]
#[table_name = "wallets"]
pub struct NewWallet<'a> {
    pub name: &'a str,
    pub caption: Option<&'a String>,
}
