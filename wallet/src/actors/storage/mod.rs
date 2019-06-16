//! # Storage actor
//!
//! It is charge of managing the connection to the key-value database. This actor is blocking so it
//! must be used with a `SyncArbiter`.

use actix::prelude::*;
use rocksdb::DB;

use witnet_protected::ProtectedString;

use crate::{storage, wallet};

pub mod builder;
pub mod handlers;

pub use handlers::*;

/// Storage actor.
pub struct Storage {
    params: storage::Params,
}

impl Storage {
    pub fn build() -> builder::Builder {
        builder::Builder::new()
    }

    pub fn new(params: storage::Params) -> Self {
        Self { params }
    }

    pub fn get_wallet_infos(&self, db: &DB) -> Result<Vec<wallet::WalletInfo>, storage::Error> {
        let ids = self.get_wallet_ids(db)?;
        let len = ids.len();
        let infos = ids
            .into_iter()
            .try_fold(Vec::with_capacity(len), |mut acc, id| {
                let info = storage::get(db, storage::keys::wallet_info(id.as_ref()))?;
                acc.push(info);

                Ok(acc)
            })?;

        Ok(infos)
    }

    pub fn get_wallet_ids(&self, db: &DB) -> Result<Vec<wallet::WalletId>, storage::Error> {
        storage::get_default(db, storage::keys::wallets())
    }

    pub fn create_wallet(
        &self,
        db: &DB,
        wallet: wallet::Wallet,
        password: ProtectedString,
    ) -> Result<(), storage::Error> {
        let mut batch = rocksdb::WriteBatch::default();
        let id = &wallet.info.id;
        let key = storage::gen_key(&self.params, password.as_ref())?;

        storage::merge(&mut batch, storage::keys::wallets(), id)?;
        storage::put(&mut batch, storage::keys::wallet_info(id), &wallet.info)?;
        storage::put(
            &mut batch,
            storage::keys::wallet(id),
            &storage::encrypt(&self.params, &key, &wallet.content)?,
        )?;

        storage::write(db, batch)?;

        Ok(())
    }

    fn flush(&self, db: &DB) -> Result<(), storage::Error> {
        storage::flush(db)
    }

    pub fn unlock_wallet(
        &self,
        db: &DB,
        id: &str,
        password: &str,
    ) -> Result<wallet::UnlockedWallet, storage::Error> {
        let encrypted: Vec<u8> = storage::get(db, storage::keys::wallet(id))
            .map_err(|_| storage::Error::UnknownWalletId(id.to_string()))?;
        let (content, key) =
            storage::decrypt_password(&self.params, password.as_bytes(), encrypted.as_ref())
                .map_err(|_| storage::Error::WrongPassword)?;
        let info = storage::get(db, storage::keys::wallet_info(id))?;

        let wallet = wallet::Wallet::new(info, content);
        let unlocked_wallet = wallet::UnlockedWallet {
            id: wallet.info.id,
            key,
        };

        Ok(unlocked_wallet)
    }
}

impl Actor for Storage {
    type Context = SyncContext<Self>;

    fn started(&mut self, _ctx: &mut Self::Context) {
        log::trace!("storage actor started");
    }

    fn stopped(&mut self, _ctx: &mut Self::Context) {
        log::trace!("storage actor stopped");
    }
}

impl Supervised for Storage {}
