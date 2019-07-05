use serde::{Deserialize, Serialize};

use crate::types;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UnlockWalletRequest {
    pub(crate) wallet_id: types::WalletId,
    pub(crate) password: types::Password,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UnlockWalletResponse {
    pub(crate) session_id: types::SessionId,
}
