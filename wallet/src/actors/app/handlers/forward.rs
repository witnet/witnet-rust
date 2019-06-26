use actix::prelude::*;

use crate::actors::App;
use crate::api;

impl Message for api::ForwardRequest {
    type Result = Result<api::ForwardResponse, api::Error>;
}

impl Handler<api::ForwardRequest> for App {
    type Result = ResponseFuture<api::ForwardResponse, api::Error>;

    fn handle(&mut self, msg: api::ForwardRequest, _ctx: &mut Self::Context) -> Self::Result {
        let f = self
            .forward(msg.method, msg.params)
            .map_err(api::node_error);

        Box::new(f)
    }
}
