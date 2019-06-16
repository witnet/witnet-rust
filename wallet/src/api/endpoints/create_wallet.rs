use serde::{Deserialize, Serialize};

use witnet_protected::ProtectedString;

use crate::wallet;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateWalletRequest {
    pub(crate) caption: String,
    pub(crate) password: ProtectedString,
    pub(crate) seed_source: wallet::SeedSource,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateWalletResponse {
    pub(crate) wallet_id: wallet::WalletId,
}
