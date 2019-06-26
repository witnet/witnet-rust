use serde::{Deserialize, Serialize};

use witnet_protected::ProtectedString;

use crate::wallet;

mod validation;

pub use validation::*;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateWalletRequest {
    pub(crate) name: Option<String>,
    pub(crate) caption: Option<String>,
    pub(crate) password: ProtectedString,
    pub(crate) seed_source: String,
    pub(crate) seed_data: ProtectedString,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateWalletResponse {
    pub(crate) wallet_id: wallet::WalletId,
}
