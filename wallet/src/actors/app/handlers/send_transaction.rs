use actix::prelude::*;

use crate::actors::App;
use crate::api;

impl Message for api::SendTransactionRequest {
    type Result = Result<api::CreateDataReqResponse, api::Error>;
}

impl Handler<api::SendTransactionRequest> for App {
    type Result = Result<api::SendTransactionResponse, api::Error>;

    fn handle(
        &mut self,
        _msg: api::SendTransactionRequest,
        _ctx: &mut Self::Context,
    ) -> Self::Result {
        Ok(())
    }
}
