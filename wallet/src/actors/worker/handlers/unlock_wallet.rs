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

pub struct WalletUnlocked {
    pub id: String,
    pub name: Option<String>,
    pub caption: Option<String>,
    pub account_index: u32,
    pub account_external: types::ExtendedSK,
    pub account_internal: types::ExtendedSK,
    pub account_rad: types::ExtendedSK,
    pub account_balance: u64,
    pub accounts: Vec<u32>,
    pub enc_key: types::Secret,
    pub iv: Vec<u8>,
}

impl Message for UnlockWallet {
    type Result = worker::Result<(String, WalletUnlocked)>;
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
