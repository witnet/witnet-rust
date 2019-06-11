use actix::prelude::*;

use crate::actors::App;
use crate::api;
use crate::error;

impl Message for api::SendDataReqRequest {
    type Result = Result<(), error::Error>;
}

impl Handler<api::SendDataReqRequest> for App {
    type Result = Result<(), error::Error>;

    fn handle(&mut self, _msg: api::SendDataReqRequest, _ctx: &mut Self::Context) -> Self::Result {
        Ok(())
    }
}
