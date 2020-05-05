use actix::prelude::*;
use serde::{Deserialize, Serialize};

use crate::actors::app;
use crate::types;

#[derive(Debug, Serialize, Deserialize)]
pub struct LockWalletRequest {
    wallet_id: String,
    session_id: types::SessionId,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct LockWalletResponse {
    pub success: bool,
}

impl Message for LockWalletRequest {
    type Result = app::Result<LockWalletResponse>;
}

impl Handler<LockWalletRequest> for app::App {
    type Result = <LockWalletRequest as Message>::Result;

    fn handle(&mut self, msg: LockWalletRequest, _ctx: &mut Self::Context) -> Self::Result {
        self.lock_wallet(msg.session_id, msg.wallet_id)
            .map(|_| LockWalletResponse { success: true })
    }
}
