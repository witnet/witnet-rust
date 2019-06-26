use actix::prelude::*;

use crate::actors::App;
use crate::api;

impl Message for api::CreateDataReqRequest {
    type Result = Result<(), api::Error>;
}

impl Handler<api::CreateDataReqRequest> for App {
    type Result = Result<(), api::Error>;

    fn handle(
        &mut self,
        _msg: api::CreateDataReqRequest,
        _ctx: &mut Self::Context,
    ) -> Self::Result {
        Ok(())
    }
}
