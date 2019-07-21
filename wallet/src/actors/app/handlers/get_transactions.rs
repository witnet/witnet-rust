use actix::prelude::*;
use serde::Deserialize;

use crate::actors::app;

#[derive(Debug, Deserialize)]
pub struct GetTransactionsRequest {
    pub wallet_id: String,
    pub limit: u32,
    pub page: u32,
}

impl Message for GetTransactionsRequest {
    type Result = app::Result<()>;
}

impl Handler<GetTransactionsRequest> for app::App {
    type Result = <GetTransactionsRequest as Message>::Result;

    fn handle(&mut self, _msg: GetTransactionsRequest, _ctx: &mut Self::Context) -> Self::Result {
        Ok(())
    }
}
