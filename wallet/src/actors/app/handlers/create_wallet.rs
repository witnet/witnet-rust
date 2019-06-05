use actix::prelude::*;

use crate::actors::App;
use crate::api;
use crate::error;

impl Message for api::CreateWalletRequest {
    type Result = Result<(), error::Error>;
}

impl Handler<api::CreateWalletRequest> for App {
    type Result = Result<(), error::Error>;

    fn handle(&mut self, _msg: api::CreateWalletRequest, _ctx: &mut Self::Context) -> Self::Result {
        unimplemented!()
    }
}
