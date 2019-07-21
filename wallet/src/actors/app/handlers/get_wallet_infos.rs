use actix::prelude::*;
use futures::future;
use serde::{Deserialize, Serialize};

use crate::actors::app;
use crate::model;

#[derive(Debug, Deserialize)]
pub struct WalletInfosRequest;

#[derive(Debug, Serialize)]
pub struct WalletInfosResponse {
    pub infos: Vec<model::WalletInfo>,
}

impl Message for WalletInfosRequest {
    type Result = app::Result<WalletInfosResponse>;
}

impl Handler<WalletInfosRequest> for app::App {
    type Result = app::ResponseFuture<WalletInfosResponse>;

    fn handle(&mut self, _msg: WalletInfosRequest, _ctx: &mut Self::Context) -> Self::Result {
        let f = self
            .get_wallet_infos()
            .and_then(|infos| future::ok(WalletInfosResponse { infos }));

        Box::new(f)
    }
}
