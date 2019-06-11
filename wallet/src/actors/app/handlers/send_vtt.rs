use actix::prelude::*;

use crate::actors::App;
use crate::api;

impl Message for api::SendVttRequest {
    type Result = Result<(), failure::Error>;
}

impl Handler<api::SendVttRequest> for App {
    type Result = Result<(), failure::Error>;

    fn handle(&mut self, _msg: api::SendVttRequest, _ctx: &mut Self::Context) -> Self::Result {
        Ok(())
    }
}
