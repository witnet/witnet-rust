use actix::prelude::*;

use crate::actors::App;
use crate::api;

impl Message for api::LockWalletRequest {
    type Result = Result<(), failure::Error>;
}

impl Handler<api::LockWalletRequest> for App {
    type Result = Result<(), failure::Error>;

    fn handle(&mut self, _msg: api::LockWalletRequest, _ctx: &mut Self::Context) -> Self::Result {
        Ok(())
    }
}
