use actix::prelude::*;
use futures::future;

use crate::actors::App;
use crate::api;

impl Message for api::WalletInfosRequest {
    type Result = Result<api::WalletInfosResponse, api::Error>;
}

impl Handler<api::WalletInfosRequest> for App {
    type Result = ResponseFuture<api::WalletInfosResponse, api::Error>;

    fn handle(&mut self, _msg: api::WalletInfosRequest, _ctx: &mut Self::Context) -> Self::Result {
        let fut = self
            .get_wallet_infos()
            .map_err(api::internal_error)
            .and_then(|infos| {
                future::ok(api::WalletInfosResponse {
                    total: infos.len(),
                    infos,
                })
            });

        Box::new(fut)
    }
}
