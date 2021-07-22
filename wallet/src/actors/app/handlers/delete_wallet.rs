use actix::prelude::*;
use serde::{Deserialize, Serialize};

use crate::actors::app;
use crate::types;

#[derive(Debug, Serialize, Deserialize)]
pub struct DeleteWalletRequest {
    session_id: types::SessionId,
    wallet_id: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct DeleteWalletResponse {
    pub success: bool,
}

impl Message for DeleteWalletRequest {
    type Result = app::Result<DeleteWalletResponse>;
}

impl Handler<DeleteWalletRequest> for app::App {
    type Result = app::ResponseActFuture<DeleteWalletResponse>;

    fn handle(&mut self, msg: DeleteWalletRequest, _ctx: &mut Self::Context) -> Self::Result {
        let f = self
            .delete_wallet(msg.session_id.clone(), msg.wallet_id.clone())
            .map_ok(|_, act, _ctx| act.lock_wallet(msg.session_id, msg.wallet_id))
            .map_ok(|_, _act, _ctx| DeleteWalletResponse { success: true });

        Box::pin(f)
    }
}
