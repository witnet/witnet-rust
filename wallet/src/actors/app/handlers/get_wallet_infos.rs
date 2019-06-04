//! TODO: doc
use actix::prelude::*;
use futures::future;
use serde::Deserialize;

use crate::actors::storage;
use crate::actors::App;
use crate::error;
use crate::wallet;

/// TODO: doc
#[derive(Debug, Deserialize)]
pub struct GetWalletInfos;

impl Message for GetWalletInfos {
    type Result = Result<Vec<wallet::WalletInfo>, error::Error>;
}

impl Handler<GetWalletInfos> for App {
    type Result = ResponseFuture<Vec<wallet::WalletInfo>, error::Error>;

    fn handle(&mut self, _msg: GetWalletInfos, _ctx: &mut Self::Context) -> Self::Result {
        let fut = self
            .storage
            .send(storage::Get::with_static_key("wallet-infos"))
            .map_err(error::Error::Mailbox)
            .and_then(|res| future::result(res).map_err(error::Error::Storage))
            .and_then(|opt| future::ok(opt.unwrap_or_else(Vec::new)));

        Box::new(fut)
    }
}
