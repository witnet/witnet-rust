use std::cmp;

use actix::prelude::*;
use serde::{Deserialize, Serialize};

use crate::actors::app;
use crate::{constants, model, types};

#[derive(Debug, Serialize, Deserialize)]
pub struct GetAddressesRequest {
    session_id: types::SessionId,
    wallet_id: String,
    offset: Option<u32>,
    limit: Option<u32>,
}

pub type GetAddressesResponse = model::Addresses;

impl Message for GetAddressesRequest {
    type Result = app::Result<GetAddressesResponse>;
}

impl Handler<GetAddressesRequest> for app::App {
    type Result = app::ResponseActFuture<GetAddressesResponse>;

    fn handle(&mut self, msg: GetAddressesRequest, _ctx: &mut Self::Context) -> Self::Result {
        let offset = msg
            .offset
            .unwrap_or_else(|| constants::DEFAULT_PAGINATION_OFFSET);
        let limit = cmp::min(
            msg.offset
                .unwrap_or_else(|| constants::DEFAULT_PAGINATION_LIMIT),
            constants::MAX_PAGINATION_LIMIT,
        );
        let f = self.get_addresses(msg.session_id, msg.wallet_id, offset, limit);

        Box::new(f)
    }
}
