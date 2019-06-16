use std::sync::Arc;

use actix::prelude::*;

use witnet_protected::ProtectedString;

use crate::actors::storage::Storage;
use crate::{storage, wallet};

pub struct UnlockWallet(
    pub Arc<rocksdb::DB>,
    pub wallet::WalletId,
    pub ProtectedString,
);

impl Message for UnlockWallet {
    type Result = Result<wallet::UnlockedWallet, storage::Error>;
}

impl Handler<UnlockWallet> for Storage {
    type Result = <UnlockWallet as Message>::Result;

    fn handle(
        &mut self,
        UnlockWallet(db, id, password): UnlockWallet,
        _ctx: &mut Self::Context,
    ) -> Self::Result {
        self.unlock_wallet(db.as_ref(), id.as_ref(), password.as_ref())
    }
}
