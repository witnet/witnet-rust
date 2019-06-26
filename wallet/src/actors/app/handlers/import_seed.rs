use actix::prelude::*;

use crate::actors::App;
use crate::api;

impl Message for api::ImportSeedRequest {
    type Result = Result<(), api::Error>;
}

impl Handler<api::ImportSeedRequest> for App {
    type Result = Result<(), api::Error>;

    fn handle(&mut self, _msg: api::ImportSeedRequest, _ctx: &mut Self::Context) -> Self::Result {
        Ok(())
    }
}
