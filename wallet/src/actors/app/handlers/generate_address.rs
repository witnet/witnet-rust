use actix::prelude::*;

use crate::actors::App;
use crate::api;
use crate::error;

impl Message for api::GenerateAddressRequest {
    type Result = Result<(), error::Error>;
}

impl Handler<api::GenerateAddressRequest> for App {
    type Result = Result<(), error::Error>;

    fn handle(
        &mut self,
        _msg: api::GenerateAddressRequest,
        _ctx: &mut Self::Context,
    ) -> Self::Result {
        unimplemented!()
    }
}
