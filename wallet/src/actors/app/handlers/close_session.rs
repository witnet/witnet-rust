use actix::prelude::*;
use serde::{Deserialize, Serialize};

use crate::actors::app;
use crate::types;

#[derive(Debug, Serialize, Deserialize)]
pub struct CloseSessionRequest {
    pub(crate) session_id: types::SessionId,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct CloseSessionResponse {
    pub success: bool,
}

impl Message for CloseSessionRequest {
    type Result = app::Result<CloseSessionResponse>;
}

impl Handler<CloseSessionRequest> for app::App {
    type Result = <CloseSessionRequest as Message>::Result;

    fn handle(&mut self, msg: CloseSessionRequest, _ctx: &mut Self::Context) -> Self::Result {
        self.close_session(msg.session_id)
            .map(|_| CloseSessionResponse { success: true })
    }
}
