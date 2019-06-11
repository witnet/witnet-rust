use actix::prelude::*;

use crate::actors::App;
use crate::api;

impl Message for api::UnlockWalletRequest {
    type Result = Result<(), failure::Error>;
}

impl Handler<api::UnlockWalletRequest> for App {
    type Result = Result<(), failure::Error>;

    fn handle(&mut self, _msg: api::UnlockWalletRequest, _ctx: &mut Self::Context) -> Self::Result {
        Ok(())
    }
}
