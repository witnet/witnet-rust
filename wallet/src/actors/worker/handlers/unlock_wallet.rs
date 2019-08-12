use actix::prelude::*;

use crate::actors::worker;
use crate::{model, types};

pub struct UnlockWallet(
    /// Wallet id
    pub String,
    /// Wallet password
    pub types::Password,
);

impl Message for UnlockWallet {
    type Result = worker::Result<model::WalletUnlocked>;
}

impl Handler<UnlockWallet> for worker::Worker {
    type Result = <UnlockWallet as Message>::Result;

    fn handle(
        &mut self,
        UnlockWallet(id, password): UnlockWallet,
        _ctx: &mut Self::Context,
    ) -> Self::Result {
        self.unlock_wallet(&id, password.as_ref())
    }
}
