use actix::prelude::*;

use crate::actors::App;
use crate::api;

impl Message for api::SendVttRequest {
    type Result = Result<(), api::Error>;
}

impl Handler<api::SendVttRequest> for App {
    type Result = Result<(), api::Error>;

    fn handle(&mut self, _msg: api::SendVttRequest, _ctx: &mut Self::Context) -> Self::Result {
        Ok(())
    }
}
