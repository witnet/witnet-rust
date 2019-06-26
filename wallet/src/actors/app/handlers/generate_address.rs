use actix::prelude::*;

use crate::actors::App;
use crate::api;

impl Message for api::GenerateAddressRequest {
    type Result = Result<(), api::Error>;
}

impl Handler<api::GenerateAddressRequest> for App {
    type Result = Result<(), api::Error>;

    fn handle(
        &mut self,
        _msg: api::GenerateAddressRequest,
        _ctx: &mut Self::Context,
    ) -> Self::Result {
        Ok(())
    }
}
