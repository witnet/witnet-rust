use actix::prelude::*;

use crate::{actors::App, api};

impl Message for api::RunRadReqRequest {
    type Result = Result<api::RunRadReqResponse, api::Error>;
}

impl Handler<api::RunRadReqRequest> for App {
    type Result = ResponseFuture<api::RunRadReqResponse, api::Error>;

    fn handle(&mut self, msg: api::RunRadReqRequest, _ctx: &mut Self::Context) -> Self::Result {
        let f = self
            .run_rad_request(msg.rad_request)
            .map_err(api::internal_error)
            .map(|result| api::RunRadReqResponse { result });

        Box::new(f)
    }
}
