use actix::prelude::*;
use serde::{Deserialize, Serialize};

use crate::actors::app;
use crate::types;

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SendTransactionRequest {
    session_id: types::SessionId,
    wallet_id: String,
    transaction_id: String,
}

impl Message for SendTransactionRequest {
    type Result = app::Result<serde_json::Value>;
}

impl Handler<SendTransactionRequest> for app::App {
    type Result = app::ResponseActFuture<serde_json::Value>;

    fn handle(&mut self, msg: SendTransactionRequest, _ctx: &mut Self::Context) -> Self::Result {
        self.send_transaction(&msg.session_id, &msg.wallet_id, msg.transaction_id)
    }
}
