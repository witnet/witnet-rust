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
