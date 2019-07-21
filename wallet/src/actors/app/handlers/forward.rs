use actix::prelude::*;

use crate::actors::app;
use crate::types;

pub struct ForwardRequest {
    pub method: String,
    pub params: types::RpcParams,
}

impl Message for ForwardRequest {
    type Result = app::Result<types::Json>;
}

impl Handler<ForwardRequest> for app::App {
    type Result = app::ResponseFuture<types::Json>;

    fn handle(&mut self, msg: ForwardRequest, _ctx: &mut Self::Context) -> Self::Result {
        let f = self.forward(msg.method, msg.params);

        Box::new(f)
    }
}
