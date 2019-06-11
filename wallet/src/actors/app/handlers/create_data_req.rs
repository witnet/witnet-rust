use actix::prelude::*;

use crate::actors::App;
use crate::api;
use crate::error;

impl Message for api::CreateDataReqRequest {
    type Result = Result<(), error::Error>;
}

impl Handler<api::CreateDataReqRequest> for App {
    type Result = Result<(), error::Error>;

    fn handle(
        &mut self,
        _msg: api::CreateDataReqRequest,
        _ctx: &mut Self::Context,
    ) -> Self::Result {
        Ok(())
    }
}
