use actix::prelude::*;
use serde::Deserialize;

use crate::actors::app;
use crate::types;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SendVttRequest {
    session_id: types::SessionId,
    wallet_id: String,
    transaction_id: String,
}

impl Message for SendVttRequest {
    type Result = app::Result<types::Success>;
}

impl Handler<SendVttRequest> for app::App {
    type Result = app::ResponseActFuture<types::Success>;

    fn handle(&mut self, msg: SendVttRequest, _ctx: &mut Self::Context) -> Self::Result {
        let f = self
            .send_vtt(&msg.session_id, &msg.wallet_id, msg.transaction_id)
            .map(|_, _, _| types::Success);

        Box::new(f)
    }
}
