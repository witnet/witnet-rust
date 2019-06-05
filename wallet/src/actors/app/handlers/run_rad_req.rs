use actix::prelude::*;

use crate::actors::{rad_executor as executor, App};
use crate::api;
use crate::error;

impl Message for api::RunRadReqRequest {
    type Result = Result<api::RunRadReqResponse, error::Error>;
}

impl Handler<api::RunRadReqRequest> for App {
    type Result = ResponseFuture<api::RunRadReqResponse, error::Error>;

    fn handle(&mut self, msg: api::RunRadReqRequest, _ctx: &mut Self::Context) -> Self::Result {
        let fut = self
            .rad_executor
            .send(executor::Run(msg.rad_request))
            .map_err(error::Error::Mailbox)
            .and_then(|result| {
                result
                    .map(|value| api::RunRadReqResponse { result: value })
                    .map_err(error::Error::Rad)
            });

        Box::new(fut)
    }
}
