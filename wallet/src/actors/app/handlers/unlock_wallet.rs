use actix::prelude::*;

use crate::actors::App;
use crate::api;

impl Message for api::UnlockWalletRequest {
    type Result = Result<api::UnlockWalletResponse, failure::Error>;
}

impl Handler<api::UnlockWalletRequest> for App {
    type Result = ResponseActFuture<Self, api::UnlockWalletResponse, failure::Error>;

    fn handle(&mut self, msg: api::UnlockWalletRequest, _ctx: &mut Self::Context) -> Self::Result {
        let id = msg.wallet_id.clone();
        let fut = self
            .unlock_wallet(msg.wallet_id, msg.session_id, msg.password)
            .map(move |_, _, _| api::UnlockWalletResponse {
                unlocked_wallet_id: id,
            });

        Box::new(fut)
    }
}
