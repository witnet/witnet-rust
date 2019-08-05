use actix::prelude::*;
use serde::{Deserialize, Serialize};

use crate::actors::app;
use crate::types;

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UnlockWalletRequest {
    pub wallet_id: String,
    pub password: types::Password,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UnlockWalletResponse {
    pub wallet: UnlockedWallet,
    pub session: Session,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Session {
    pub id: String,
    pub expiration_secs: u64,
}

#[derive(Serialize)]
pub struct UnlockedWallet {
    pub name: Option<String>,
    pub caption: Option<String>,
    pub balance: u64,
    pub account: u32,
    pub accounts: Vec<u32>,
}

impl Message for UnlockWalletRequest {
    type Result = Result<UnlockWalletResponse, app::Error>;
}

impl Handler<UnlockWalletRequest> for app::App {
    type Result = app::ResponseActFuture<UnlockWalletResponse>;

    fn handle(&mut self, msg: UnlockWalletRequest, _ctx: &mut Self::Context) -> Self::Result {
        let f =
            self.unlock_wallet(msg.wallet_id, msg.password)
                .map(|(session, wallet), slf, ctx| {
                    slf.set_session_to_expire(session.id.clone()).spawn(ctx);

                    UnlockWalletResponse { wallet, session }
                });

        Box::new(f)
    }
}
