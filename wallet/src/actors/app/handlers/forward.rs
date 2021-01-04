use actix::prelude::*;

use crate::actors::app;

pub struct ForwardRequest {
    pub method: String,
    pub params: jsonrpc_core::Params,
}

impl Message for ForwardRequest {
    type Result = app::Result<serde_json::Value>;
}

impl Handler<ForwardRequest> for app::App {
    type Result = app::ResponseFuture<serde_json::Value>;

    fn handle(&mut self, msg: ForwardRequest, _ctx: &mut Self::Context) -> Self::Result {
        let f = self.forward(msg.method, msg.params);

        Box::pin(f)
    }
}
