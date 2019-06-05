use actix::prelude::*;

use crate::actors::App;
use crate::api;
use crate::error;

impl Message for api::ForwardRequest {
    type Result = Result<api::ForwardResponse, error::Error>;
}

impl Handler<api::ForwardRequest> for App {
    type Result = ResponseFuture<api::ForwardResponse, error::Error>;

    fn handle(&mut self, msg: api::ForwardRequest, _ctx: &mut Self::Context) -> Self::Result {
        self.forward(msg.method, msg.params)
    }
}
