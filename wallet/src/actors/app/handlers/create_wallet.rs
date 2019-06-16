use actix::prelude::*;

use crate::actors::App;
use crate::api;

impl Message for api::CreateWalletRequest {
    type Result = Result<api::CreateWalletResponse, failure::Error>;
}

impl Handler<api::CreateWalletRequest> for App {
    type Result = ResponseActFuture<Self, api::CreateWalletResponse, failure::Error>;

    fn handle(&mut self, msg: api::CreateWalletRequest, _ctx: &mut Self::Context) -> Self::Result {
        let fut = self
            .create_wallet(msg.caption, msg.password, msg.seed_source)
            .map(|wallet_id, _slf, _ctx| api::CreateWalletResponse { wallet_id });

        Box::new(fut)
    }
}
