use actix::prelude::*;

use crate::actors::App;
use crate::{api, app, storage, validation};

impl Message for api::UnlockWalletRequest {
    type Result = Result<api::UnlockWalletResponse, api::Error>;
}

impl Handler<api::UnlockWalletRequest> for App {
    type Result = ResponseActFuture<Self, api::UnlockWalletResponse, api::Error>;

    fn handle(&mut self, msg: api::UnlockWalletRequest, _ctx: &mut Self::Context) -> Self::Result {
        let fut = self
            .unlock_wallet(msg.wallet_id, msg.password)
            .map_err(|err, _slf, _ctx| match err {
                err @ app::Error::Storage(storage::Error::WalletNotFound) => {
                    api::validation_error(validation::error("walletId", format!("{}", err)))
                }
                err @ app::Error::Storage(storage::Error::WrongPassword) => {
                    api::validation_error(validation::error("password", format!("{}", err)))
                }
                _ => api::internal_error(err),
            })
            .map(|session_id, slf, ctx| {
                slf.set_session_to_expire(session_id.clone()).spawn(ctx);

                api::UnlockWalletResponse { session_id }
            });

        Box::new(fut)
    }
}
