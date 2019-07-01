use serde::Deserialize;

use crate::{app, wallet};

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LockWalletRequest {
    pub(crate) wallet_id: wallet::WalletId,
    pub(crate) session_id: app::SessionId,
}

pub type LockWalletResponse = ();
