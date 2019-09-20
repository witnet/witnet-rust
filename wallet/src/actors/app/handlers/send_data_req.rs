use actix::prelude::*;
use serde::Deserialize;

use crate::actors::app;
use crate::types;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SendDataReqRequest {
    session_id: types::SessionId,
    wallet_id: String,
    transaction_id: String,
}

impl Message for SendDataReqRequest {
    type Result = app::Result<serde_json::Value>;
}

impl Handler<SendDataReqRequest> for app::App {
    type Result = app::ResponseActFuture<serde_json::Value>;

    fn handle(&mut self, msg: SendDataReqRequest, _ctx: &mut Self::Context) -> Self::Result {
        self.send_data_req(&msg.session_id, &msg.wallet_id, msg.transaction_id)
    }
}
