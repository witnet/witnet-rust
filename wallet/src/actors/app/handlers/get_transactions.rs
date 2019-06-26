use actix::prelude::*;

use crate::actors::App;
use crate::api;

impl Message for api::GetTransactionsRequest {
    type Result = Result<(), api::Error>;
}

impl Handler<api::GetTransactionsRequest> for App {
    type Result = Result<(), api::Error>;

    fn handle(
        &mut self,
        _msg: api::GetTransactionsRequest,
        _ctx: &mut Self::Context,
    ) -> Self::Result {
        Ok(())
    }
}
