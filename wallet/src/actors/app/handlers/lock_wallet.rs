use actix::prelude::*;

use crate::actors::App;
use crate::api;
use crate::error;

impl Message for api::LockWalletRequest {
    type Result = Result<(), error::Error>;
}

impl Handler<api::LockWalletRequest> for App {
    type Result = Result<(), error::Error>;

    fn handle(&mut self, _msg: api::LockWalletRequest, _ctx: &mut Self::Context) -> Self::Result {
        Ok(())
    }
}
