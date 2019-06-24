use actix::prelude::*;

use crate::{actors::App, api};

impl Message for api::RunRadReqRequest {
    type Result = Result<api::RunRadReqResponse, failure::Error>;
}

impl Handler<api::RunRadReqRequest> for App {
    type Result = ResponseFuture<api::RunRadReqResponse, failure::Error>;

    fn handle(&mut self, msg: api::RunRadReqRequest, _ctx: &mut Self::Context) -> Self::Result {
        let f = self
            .run_rad_request(msg.rad_request)
            .map(|result| api::RunRadReqResponse { result });

        Box::new(f)
    }
}
