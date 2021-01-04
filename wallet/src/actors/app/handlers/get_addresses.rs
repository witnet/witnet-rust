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
    external: Option<bool>,
}

pub type GetAddressesResponse = model::Addresses;

impl Message for GetAddressesRequest {
    type Result = app::Result<GetAddressesResponse>;
}

impl Handler<GetAddressesRequest> for app::App {
    type Result = app::ResponseActFuture<GetAddressesResponse>;

    fn handle(&mut self, msg: GetAddressesRequest, _ctx: &mut Self::Context) -> Self::Result {
        let offset = msg.offset.unwrap_or(constants::DEFAULT_PAGINATION_OFFSET);
        let limit = msg.limit.unwrap_or(constants::DEFAULT_PAGINATION_LIMIT);
        let external = msg.external.unwrap_or(true);
        let f = self.get_addresses(msg.session_id, msg.wallet_id, offset, limit, external);

        Box::pin(f)
    }
}
