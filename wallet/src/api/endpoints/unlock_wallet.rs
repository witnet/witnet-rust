use serde::{Deserialize, Serialize};

use witnet_protected::ProtectedString;

use crate::{app, wallet};

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UnlockWalletRequest {
    pub(crate) wallet_id: wallet::WalletId,
    pub(crate) password: ProtectedString,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UnlockWalletResponse {
    pub(crate) session_id: app::SessionId,
}
