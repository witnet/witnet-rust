use std::cmp;

use actix::prelude::*;
use serde::{Deserialize, Serialize};

use crate::actors::app;
use crate::{constants, model};

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetTransactionsRequest {
    session_id: String,
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
        let limit = cmp::min(
            msg.offset
                .unwrap_or_else(|| constants::DEFAULT_PAGINATION_LIMIT),
            constants::MAX_PAGINATION_LIMIT,
        );
        let f = self.get_transactions(msg.session_id, msg.wallet_id, offset, limit);

        Box::new(f)
    }
}
