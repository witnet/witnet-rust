use actix::prelude::*;

use crate::actors::App;
use crate::api;
use crate::error;

impl Message for api::SendVttRequest {
    type Result = Result<(), error::Error>;
}

impl Handler<api::SendVttRequest> for App {
    type Result = Result<(), error::Error>;

    fn handle(&mut self, _msg: api::SendVttRequest, _ctx: &mut Self::Context) -> Self::Result {
        Ok(())
    }
}
