use actix::prelude::*;

use crate::actors::App;
use crate::api;

impl Message for api::GetTransactionsRequest {
    type Result = Result<(), failure::Error>;
}

impl Handler<api::GetTransactionsRequest> for App {
    type Result = Result<(), failure::Error>;

    fn handle(
        &mut self,
        _msg: api::GetTransactionsRequest,
        _ctx: &mut Self::Context,
    ) -> Self::Result {
        Ok(())
    }
}
