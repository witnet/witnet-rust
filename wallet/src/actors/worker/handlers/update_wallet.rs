use actix::prelude::*;

use crate::actors::worker;
use crate::types;

pub struct UpdateWallet(
    pub types::SessionWallet,
    /// Wallet name
    pub Option<String>,
    /// Wallet description
    pub Option<String>,
);

impl Message for UpdateWallet {
    type Result = worker::Result<()>;
}

impl Handler<UpdateWallet> for worker::Worker {
    type Result = <UpdateWallet as Message>::Result;

    fn handle(
        &mut self,
        UpdateWallet(wallet, name, description): UpdateWallet,
        _ctx: &mut Self::Context,
    ) -> Self::Result {
        self.update_wallet(&wallet, name, description)
    }
}
