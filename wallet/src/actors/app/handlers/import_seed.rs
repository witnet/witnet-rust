use actix::prelude::*;

use crate::actors::App;
use crate::api;
use crate::error;

impl Message for api::ImportSeedRequest {
    type Result = Result<(), error::Error>;
}

impl Handler<api::ImportSeedRequest> for App {
    type Result = Result<(), error::Error>;

    fn handle(&mut self, _msg: api::ImportSeedRequest, _ctx: &mut Self::Context) -> Self::Result {
        Ok(())
    }
}
