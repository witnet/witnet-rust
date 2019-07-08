use actix::prelude::*;

use crate::actors::App;
use crate::api;

impl Message for api::CreateVttRequest {
    type Result = Result<api::CreateDataReqResponse, api::Error>;
}

impl Handler<api::CreateVttRequest> for App {
    type Result = Result<api::CreateVttResponse, api::Error>;

    fn handle(&mut self, _msg: api::CreateVttRequest, _ctx: &mut Self::Context) -> Self::Result {
        Ok(())
    }
}
