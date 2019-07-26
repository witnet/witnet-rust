use actix::prelude::*;
use serde::{Deserialize, Serialize};

use crate::actors::app;

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LockWalletRequest {
    wallet_id: String,
    session_id: String,
}

impl Message for LockWalletRequest {
    type Result = app::Result<()>;
}

impl Handler<LockWalletRequest> for app::App {
    type Result = <LockWalletRequest as Message>::Result;

    fn handle(&mut self, msg: LockWalletRequest, _ctx: &mut Self::Context) -> Self::Result {
        self.lock_wallet(msg.session_id, msg.wallet_id)
    }
}
