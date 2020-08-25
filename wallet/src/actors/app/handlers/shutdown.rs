use actix::prelude::*;
use serde::{Deserialize, Serialize};

use crate::actors::app;
use crate::types;

pub struct Shutdown;

impl Message for Shutdown {
    type Result = ();
}

impl Handler<Shutdown> for app::App {
    type Result = ();

    fn handle(&mut self, _msg: Shutdown, ctx: &mut Self::Context) -> Self::Result {
        self.stop(ctx);
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ShutdownRequest {
    session_id: Option<types::SessionId>,
}

impl Message for ShutdownRequest {
    type Result = app::Result<()>;
}

impl Handler<ShutdownRequest> for app::App {
    type Result = <ShutdownRequest as Message>::Result;

    fn handle(&mut self, msg: ShutdownRequest, ctx: &mut Self::Context) -> Self::Result {
        self.shutdown_request(msg.session_id, ctx)
    }
}
