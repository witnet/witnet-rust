use actix::prelude::*;
use serde::{Deserialize, Serialize};

use crate::actors::app;
use crate::{constants, model, types};

#[derive(Debug, Serialize, Deserialize)]
pub struct GetTransactionsRequest {
    session_id: types::SessionId,
    wallet_id: String,
    offset: Option<u32>,
    limit: Option<u32>,
}

pub type GetTransactionsResponse = model::Transactions;

impl Message for GetTransactionsRequest {
    type Result = app::Result<GetTransactionsResponse>;
}

impl Handler<GetTransactionsRequest> for app::App {
    type Result = app::ResponseActFuture<GetTransactionsResponse>;

    fn handle(&mut self, msg: GetTransactionsRequest, _ctx: &mut Self::Context) -> Self::Result {
        let offset = msg
            .offset
            .unwrap_or_else(|| constants::DEFAULT_PAGINATION_OFFSET);
        let limit = msg
            .limit
            .unwrap_or_else(|| constants::DEFAULT_PAGINATION_LIMIT);
        let f = self.get_transactions(msg.session_id, msg.wallet_id, offset, limit);

        Box::new(f)
    }
}
