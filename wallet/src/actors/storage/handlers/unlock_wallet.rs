use actix::prelude::*;

use crate::actors::storage::Storage;
use crate::{storage, types};

pub struct UnlockWallet(
    pub types::SharedDB,
    pub types::WalletId,
    pub types::Password,
);

impl Message for UnlockWallet {
    type Result = Result<types::UnlockedWallet, storage::Error>;
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
