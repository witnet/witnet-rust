use actix::prelude::*;

use crate::actors::App;
use crate::api;

impl Message for api::GenerateAddressRequest {
    type Result = Result<(), failure::Error>;
}

impl Handler<api::GenerateAddressRequest> for App {
    type Result = Result<(), failure::Error>;

    fn handle(
        &mut self,
        _msg: api::GenerateAddressRequest,
        _ctx: &mut Self::Context,
    ) -> Self::Result {
        Ok(())
    }
}
