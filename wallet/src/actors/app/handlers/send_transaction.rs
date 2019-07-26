use actix::prelude::*;
use serde::{Deserialize, Serialize};

use crate::actors::app;

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SendTransactionRequest {
    transaction_id: String,
}

impl Message for SendTransactionRequest {
    type Result = app::Result<()>;
}

impl Handler<SendTransactionRequest> for app::App {
    type Result = <SendTransactionRequest as Message>::Result;

    fn handle(&mut self, _msg: SendTransactionRequest, _ctx: &mut Self::Context) -> Self::Result {
        Ok(())
    }
}
