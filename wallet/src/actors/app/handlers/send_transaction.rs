use actix::prelude::*;
use serde::{Deserialize, Serialize};

use crate::{actors::app, model, types};

#[derive(Debug, Serialize, Deserialize)]
pub struct SendTransactionRequest {
    session_id: types::SessionId,
    wallet_id: String,
    transaction: types::Transaction,
}

#[derive(Debug, Serialize)]
pub struct SendTransactionResponse {
    pub jsonrpc_result: serde_json::Value,
    pub balance_movement: Option<model::BalanceMovement>,
}

impl Message for SendTransactionRequest {
    type Result = app::Result<SendTransactionResponse>;
}

impl Handler<SendTransactionRequest> for app::App {
    type Result = app::ResponseActFuture<SendTransactionResponse>;

    fn handle(&mut self, msg: SendTransactionRequest, _ctx: &mut Self::Context) -> Self::Result {
        self.send_transaction(msg.session_id, msg.wallet_id, msg.transaction)
    }
}
