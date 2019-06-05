use actix::prelude::*;

use crate::actors::App;
use crate::api;
use crate::error;

impl Message for api::UnlockWalletRequest {
    type Result = Result<(), error::Error>;
}

impl Handler<api::UnlockWalletRequest> for App {
    type Result = Result<(), error::Error>;

    fn handle(&mut self, _msg: api::UnlockWalletRequest, _ctx: &mut Self::Context) -> Self::Result {
        unimplemented!()
    }
}
