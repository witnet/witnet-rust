use actix::prelude::*;

use crate::actors::App;
use crate::{api, app, storage, validation};

impl Message for api::UnlockWalletRequest {
    type Result = Result<api::UnlockWalletResponse, api::Error>;
}

impl Handler<api::UnlockWalletRequest> for App {
    type Result = ResponseActFuture<Self, api::UnlockWalletResponse, api::Error>;

    fn handle(&mut self, msg: api::UnlockWalletRequest, _ctx: &mut Self::Context) -> Self::Result {
        let id = msg.wallet_id.clone();
        let fut = self
            .unlock_wallet(msg.wallet_id, msg.session_id, msg.password)
            .map_err(|err, _, _| match err {
                err @ app::Error::Storage(storage::Error::WalletNotFound) => {
                    api::validation_error(validation::error("walletId", format!("{}", err)))
                }
                err @ app::Error::Storage(storage::Error::WrongPassword) => {
                    api::validation_error(validation::error("password", format!("{}", err)))
                }
                _ => api::internal_error(err),
            })
            .map(move |_, _, _| api::UnlockWalletResponse {
                unlocked_wallet_id: id,
            });

        Box::new(fut)
    }
}
