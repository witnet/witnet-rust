use actix::prelude::*;
use serde::{Deserialize, Serialize};

use crate::{actors::app, types};

#[derive(Debug, Serialize, Deserialize)]
pub struct RefreshSessionRequest {
    pub session_id: types::SessionId,
}

#[derive(Serialize)]
pub struct RefreshSessionResponse {
    pub success: bool,
}

impl Message for RefreshSessionRequest {
    type Result = Result<RefreshSessionResponse, app::Error>;
}

impl Handler<RefreshSessionRequest> for app::App {
    type Result = <RefreshSessionRequest as Message>::Result;

    fn handle(&mut self, msg: RefreshSessionRequest, ctx: &mut Self::Context) -> Self::Result {
        let session = self
            .state
            .sessions
            .get_mut(&msg.session_id)
            .ok_or(app::Error::SessionNotFound)?;

        if !session.session_extended {
            session.session_extended = true;
            self.set_session_to_expire(msg.session_id.clone())?
                .spawn(ctx);

            Ok(RefreshSessionResponse { success: true })
        } else {
            Ok(RefreshSessionResponse { success: false })
        }
    }
}
