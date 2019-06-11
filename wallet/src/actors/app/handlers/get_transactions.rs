use actix::prelude::*;

use crate::actors::App;
use crate::api;
use crate::error;

impl Message for api::GetTransactionsRequest {
    type Result = Result<(), error::Error>;
}

impl Handler<api::GetTransactionsRequest> for App {
    type Result = Result<(), error::Error>;

    fn handle(
        &mut self,
        _msg: api::GetTransactionsRequest,
        _ctx: &mut Self::Context,
    ) -> Self::Result {
        Ok(())
    }
}
