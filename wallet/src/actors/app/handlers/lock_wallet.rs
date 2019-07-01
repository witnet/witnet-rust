use actix::prelude::*;

use crate::actors::App;
use crate::{api, app};

impl Message for api::LockWalletRequest {
    type Result = Result<api::LockWalletResponse, api::Error>;
}

impl Handler<api::LockWalletRequest> for App {
    type Result = Result<api::LockWalletResponse, api::Error>;

    fn handle(&mut self, msg: api::LockWalletRequest, _ctx: &mut Self::Context) -> Self::Result {
        self.lock_wallet(msg.session_id, msg.wallet_id)
            .map_err(|err| match err {
                app::Error::UnknownSession => api::Error::Unauthorized,
                app::Error::WrongWallet(_) => api::Error::Forbidden,
                e => api::internal_error(e),
            })
    }
}
