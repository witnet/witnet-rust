use std::sync::Arc;

use actix::prelude::*;

use witnet_protected::ProtectedString;

use crate::actors::storage::Storage;
use crate::{storage::Error, wallet};

pub struct CreateWallet(
    pub Arc<rocksdb::DB>,
    /// Wallet to save
    pub wallet::Wallet,
    /// Encryption password
    pub ProtectedString,
);

impl Message for CreateWallet {
    type Result = Result<(), Error>;
}

impl Handler<CreateWallet> for Storage {
    type Result = <CreateWallet as Message>::Result;

    fn handle(
        &mut self,
        CreateWallet(db, wallet, password): CreateWallet,
        _ctx: &mut Self::Context,
    ) -> Self::Result {
        self.create_wallet(db.as_ref(), wallet, password)
    }
}
