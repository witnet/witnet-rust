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
