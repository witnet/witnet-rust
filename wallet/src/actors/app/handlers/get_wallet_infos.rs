use actix::prelude::*;
use futures::FutureExt;
use serde::{Deserialize, Serialize};

use crate::actors::app;
use crate::model;

#[derive(Debug, Serialize, Deserialize)]
pub struct WalletInfosRequest;

#[derive(Debug, Serialize)]
pub struct WalletInfosResponse {
    pub infos: Vec<model::Wallet>,
}

impl Message for WalletInfosRequest {
    type Result = app::Result<WalletInfosResponse>;
}

impl Handler<WalletInfosRequest> for app::App {
    type Result = app::ResponseFuture<WalletInfosResponse>;

    fn handle(&mut self, _msg: WalletInfosRequest, _ctx: &mut Self::Context) -> Self::Result {
        let f = self
            .wallet_infos()
            .map(|res| res.map(|infos| WalletInfosResponse { infos }));

        Box::pin(f)
    }
}
