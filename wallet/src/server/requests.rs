use crate::types;
use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub struct GetWalletInfos;

#[derive(Debug, Deserialize)]
pub struct CreateMnemonics {
    pub length: u8,
}

#[derive(Debug, Deserialize)]
pub struct RunRadRequest {
    #[serde(rename = "radRequest")]
    pub rad_request: types::RADRequest,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateWallet {
    pub name: String,
    pub caption: Option<String>,
    pub password: types::ProtectedString,
    pub seed_source: String,
    pub seed_data: types::ProtectedString,
}
