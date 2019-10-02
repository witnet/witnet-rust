use actix::prelude::*;

use crate::actors::worker;

pub struct UpdateWalletInfo(
    /// Wallet id
    pub String,
    /// Wallet name
    pub Option<String>,
    /// Wallet caption
    pub Option<String>,
);

impl Message for UpdateWalletInfo {
    type Result = worker::Result<()>;
}

impl Handler<UpdateWalletInfo> for worker::Worker {
    type Result = <UpdateWalletInfo as Message>::Result;

    fn handle(
        &mut self,
        UpdateWalletInfo(wallet_id, name, caption): UpdateWalletInfo,
        _ctx: &mut Self::Context,
    ) -> Self::Result {
        self.update_wallet_info(&wallet_id, name, caption)
    }
}
