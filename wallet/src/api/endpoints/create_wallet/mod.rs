use serde::{Deserialize, Serialize};

use crate::types;

mod validation;

pub use validation::*;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateWalletRequest {
    pub(crate) name: Option<String>,
    pub(crate) caption: Option<String>,
    pub(crate) password: types::Password,
    pub(crate) seed_source: String,
    pub(crate) seed_data: types::Password,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateWalletResponse {
    pub(crate) wallet_id: types::WalletId,
}
