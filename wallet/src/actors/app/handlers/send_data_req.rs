use actix::prelude::*;

use crate::actors::App;
use crate::api;

impl Message for api::SendDataReqRequest {
    type Result = Result<(), failure::Error>;
}

impl Handler<api::SendDataReqRequest> for App {
    type Result = Result<(), failure::Error>;

    fn handle(&mut self, _msg: api::SendDataReqRequest, _ctx: &mut Self::Context) -> Self::Result {
        Ok(())
    }
}
