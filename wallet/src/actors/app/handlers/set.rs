use actix::prelude::*;
use serde::{Deserialize, Serialize};

use crate::actors::app;
use crate::types;

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SetRequest {
    session_id: types::SessionId,
    wallet_id: String,
    key: String,
    value: types::RpcParams,
}

impl Message for SetRequest {
    type Result = app::Result<()>;
}

impl Handler<SetRequest> for app::App {
    type Result = app::ResponseActFuture<()>;

    fn handle(&mut self, req: SetRequest, _ctx: &mut Self::Context) -> Self::Result {
        let f = self.set(req.session_id, req.wallet_id, req.key, req.value);

        Box::new(f)
    }
}
