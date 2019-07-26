use std::cmp;

use actix::prelude::*;
use serde::{Deserialize, Serialize};

use crate::actors::app;
use crate::model;

static DEFAULT_OFFSET: u32 = 0;

static DEFAULT_LIMIT: u32 = 10;

static MAX_LIMIT: u32 = 150;

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetAddressesRequest {
    session_id: String,
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
        let offset = msg.offset.unwrap_or_else(|| DEFAULT_OFFSET);
        let limit = cmp::min(msg.offset.unwrap_or_else(|| DEFAULT_LIMIT), MAX_LIMIT);
        let f = self.get_addresses(msg.session_id, msg.wallet_id, offset, limit);

        Box::new(f)
    }
}
