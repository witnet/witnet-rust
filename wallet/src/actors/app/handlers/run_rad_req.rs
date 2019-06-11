use actix::prelude::*;

use crate::actors::{app::error, rad_executor as executor, App};
use crate::api;

impl Message for api::RunRadReqRequest {
    type Result = Result<api::RunRadReqResponse, failure::Error>;
}

impl Handler<api::RunRadReqRequest> for App {
    type Result = ResponseFuture<api::RunRadReqResponse, failure::Error>;

    fn handle(&mut self, msg: api::RunRadReqRequest, _ctx: &mut Self::Context) -> Self::Result {
        let fut = self
            .rad_executor
            .send(executor::Run(msg.rad_request))
            .map_err(error::Error::RadScheduleFailed)
            .and_then(|result| {
                result
                    .map(|value| api::RunRadReqResponse { result: value })
                    .map_err(error::Error::RadFailed)
            })
            .map_err(failure::Error::from);

        Box::new(fut)
    }
}
