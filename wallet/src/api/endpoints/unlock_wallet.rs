use serde::{Deserialize, Serialize};

use witnet_protected::ProtectedString;

use crate::{app, wallet};

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UnlockWalletRequest {
    pub wallet_id: wallet::WalletId,
    pub password: ProtectedString,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UnlockWalletResponse {
    pub session_id: app::SessionId,
}
