use actix::prelude::*;
use futures::future;

use crate::actors::storage;
use crate::actors::App;
use crate::api;
use crate::error;

impl Message for api::WalletInfosRequest {
    type Result = Result<api::WalletInfosResponse, error::Error>;
}

impl Handler<api::WalletInfosRequest> for App {
    type Result = ResponseFuture<api::WalletInfosResponse, error::Error>;

    fn handle(&mut self, _msg: api::WalletInfosRequest, _ctx: &mut Self::Context) -> Self::Result {
        let fut = self
            .storage
            .send(storage::GetWalletInfos)
            .map_err(error::Error::Mailbox)
            .and_then(|res| future::result(res).map_err(error::Error::Storage))
            .and_then(|infos| future::ok(api::WalletInfosResponse { infos }));

        Box::new(fut)
    }
}
