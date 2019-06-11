use actix::prelude::*;

use crate::actors::App;
use crate::api;

impl Message for api::ForwardRequest {
    type Result = Result<api::ForwardResponse, failure::Error>;
}

impl Handler<api::ForwardRequest> for App {
    type Result = ResponseFuture<api::ForwardResponse, failure::Error>;

    fn handle(&mut self, msg: api::ForwardRequest, _ctx: &mut Self::Context) -> Self::Result {
        self.forward(msg.method, msg.params)
    }
}
