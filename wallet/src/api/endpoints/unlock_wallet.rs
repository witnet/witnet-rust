use serde::{Deserialize, Serialize};

use witnet_protected::ProtectedString;

use crate::wallet;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UnlockWalletRequest {
    pub wallet_id: wallet::WalletId,
    pub session_id: String,
    pub password: ProtectedString,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UnlockWalletResponse {
    pub unlocked_wallet_id: wallet::WalletId,
}
