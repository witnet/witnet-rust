use actix::prelude::*;
use serde::Deserialize;

use crate::actors::app;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CloseSessionRequest {
    pub(crate) session_id: String,
}

impl Message for CloseSessionRequest {
    type Result = app::Result<()>;
}

impl Handler<CloseSessionRequest> for app::App {
    type Result = <CloseSessionRequest as Message>::Result;

    fn handle(&mut self, msg: CloseSessionRequest, _ctx: &mut Self::Context) -> Self::Result {
        self.close_session(msg.session_id)
    }
}
