use serde::Serialize;

use crate::*;

#[derive(Debug, Serialize)]
pub struct WalletInfos {
    pub infos: Vec<models::WalletInfo>,
}

#[derive(Debug, Serialize)]
pub struct Mnemonics {
    pub mnemonics: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum RadRequestResult {
    Value(types::RadonTypes),
    Error(String),
}

#[derive(Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct WalletId {
    pub wallet_id: i32,
}

pub type Empty = ();

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UnlockedWallet {
    pub session_id: types::SessionId,
    pub accounts: Vec<models::AccountInfo>,
    pub default_account: u32,
    pub session_expiration_secs: u64,
}
