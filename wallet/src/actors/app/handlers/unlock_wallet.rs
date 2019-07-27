use actix::prelude::*;
use serde::{Deserialize, Serialize};

use crate::actors::app;
use crate::{model, types};

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UnlockWalletRequest {
    pub wallet_id: String,
    pub password: types::Password,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UnlockWalletResponse {
    session_id: String,
    name: Option<String>,
    caption: Option<String>,
    accounts: Vec<u32>,
    session_expiration_secs: u64,
}

impl Message for UnlockWalletRequest {
    type Result = Result<UnlockWalletResponse, app::Error>;
}

impl Handler<UnlockWalletRequest> for app::App {
    type Result = app::ResponseActFuture<UnlockWalletResponse>;

    fn handle(&mut self, msg: UnlockWalletRequest, _ctx: &mut Self::Context) -> Self::Result {
        let f = self.unlock_wallet(msg.wallet_id, msg.password).map(
            |model::WalletUnlocked {
                 name,
                 caption,
                 session_id,
                 accounts,
                 ..
             },
             slf,
             ctx| {
                slf.set_session_to_expire(session_id.clone()).spawn(ctx);

                UnlockWalletResponse {
                    session_id,
                    accounts,
                    name,
                    caption,
                    session_expiration_secs: slf.params.session_expires_in.as_secs(),
                }
            },
        );

        Box::new(f)
    }
}
