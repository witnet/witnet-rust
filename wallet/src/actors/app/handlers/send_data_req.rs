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
    type Result = app::Result<types::Success>;
}

impl Handler<SendDataReqRequest> for app::App {
    type Result = app::ResponseActFuture<types::Success>;

    fn handle(&mut self, msg: SendDataReqRequest, _ctx: &mut Self::Context) -> Self::Result {
        let f = self
            .send_data_req(&msg.session_id, &msg.wallet_id, msg.transaction_id)
            .map(|_, _, _| types::Success);

        Box::new(f)
    }
}
