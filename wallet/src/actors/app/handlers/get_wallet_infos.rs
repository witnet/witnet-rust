use actix::prelude::*;
use futures::future;

use crate::actors::storage;
use crate::actors::{app::error, App};
use crate::api;

impl Message for api::WalletInfosRequest {
    type Result = Result<api::WalletInfosResponse, failure::Error>;
}

impl Handler<api::WalletInfosRequest> for App {
    type Result = ResponseFuture<api::WalletInfosResponse, failure::Error>;

    fn handle(&mut self, _msg: api::WalletInfosRequest, _ctx: &mut Self::Context) -> Self::Result {
        let fut = self
            .storage
            .send(storage::GetWalletInfos)
            .map_err(error::Error::StorageCommFailed)
            .and_then(|res| future::result(res).map_err(error::Error::StorageOpFailed))
            .and_then(|infos| {
                future::ok(api::WalletInfosResponse {
                    total: infos.len(),
                    infos,
                })
            })
            .map_err(failure::Error::from);

        Box::new(fut)
    }
}
