use actix::prelude::*;
use serde::{Deserialize, Serialize};

use crate::actors::app;
use crate::{model, types};

#[derive(Debug, Serialize, Deserialize)]
pub struct GetBalanceRequest {
    session_id: types::SessionId,
    wallet_id: String,
}

pub type GetBalanceResponse = model::WalletBalance;

impl Message for GetBalanceRequest {
    type Result = app::Result<GetBalanceResponse>;
}

impl Handler<GetBalanceRequest> for app::App {
    type Result = app::ResponseActFuture<GetBalanceResponse>;

    fn handle(&mut self, msg: GetBalanceRequest, _ctx: &mut Self::Context) -> Self::Result {
        let f = self.get_balance(msg.session_id, msg.wallet_id);

        Box::pin(f)
    }
}
