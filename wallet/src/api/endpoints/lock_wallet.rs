use serde::Deserialize;

use crate::types;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LockWalletRequest {
    pub(crate) wallet_id: types::WalletId,
    pub(crate) session_id: types::SessionId,
}

pub type LockWalletResponse = ();
