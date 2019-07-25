use std::sync::Arc;

use actix::prelude::*;

use crate::actors::worker;
use crate::types;

pub struct UnlockWallet(
    pub Arc<rocksdb::DB>,
    /// Wallet id
    pub String,
    /// Wallet password
    pub types::Password,
);

impl Message for UnlockWallet {
    type Result = worker::Result<types::WalletUnlocked>;
}

impl Handler<UnlockWallet> for worker::Worker {
    type Result = <UnlockWallet as Message>::Result;

    fn handle(
        &mut self,
        UnlockWallet(db, id, password): UnlockWallet,
        _ctx: &mut Self::Context,
    ) -> Self::Result {
        self.unlock_wallet(worker::Db::new(db.as_ref()), &id, password.as_ref())
    }
}
