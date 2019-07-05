//! # Storage actor
//!
//! It is charge of managing the connection to the key-value database. This actor is blocking so it
//! must be used with a `SyncArbiter`.

use actix::prelude::*;
use rocksdb::DB;

use witnet_protected::ProtectedString;

use crate::{storage, types};

pub mod handlers;

pub use handlers::*;

/// Storage actor.
pub struct Storage {
    encrypt_salt_length: usize,
    encrypt_iv_length: usize,
    encrypt_hash_iterations: u32,
}

impl Storage {
    /// Start actor.
    pub fn start(
        db_encrypt_hash_iterations: u32,
        db_encrypt_iv_length: usize,
        db_encrypt_salt_length: usize,
    ) -> Addr<Self> {
        SyncArbiter::start(1, move || Storage {
            encrypt_salt_length: db_encrypt_salt_length,
            encrypt_iv_length: db_encrypt_iv_length,
            encrypt_hash_iterations: db_encrypt_hash_iterations,
        })
    }

    pub fn get_wallet_infos(&self, db: &DB) -> Result<Vec<types::WalletInfo>, storage::Error> {
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

    pub fn get_wallet_ids(&self, db: &DB) -> Result<Vec<types::WalletId>, storage::Error> {
        storage::get_default(db, storage::keys::wallets())
    }

    pub fn create_wallet(
        &self,
        db: &DB,
        wallet: types::Wallet,
        password: ProtectedString,
    ) -> Result<(), storage::Error> {
        let mut batch = rocksdb::WriteBatch::default();
        let id = &wallet.info.id;
        let key = storage::gen_key(
            self.encrypt_salt_length,
            self.encrypt_hash_iterations,
            password.as_ref(),
        )?;

        storage::merge(&mut batch, storage::keys::wallets(), id)?;
        storage::put(&mut batch, storage::keys::wallet_info(id), &wallet.info)?;
        storage::put(
            &mut batch,
            storage::keys::wallet(id),
            &storage::encrypt(self.encrypt_iv_length, &key, &wallet.content)?,
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
    ) -> Result<types::UnlockedWallet, storage::Error> {
        let encrypted: Vec<u8> = storage::get_opt(db, storage::keys::wallet(id))?
            .ok_or_else(|| storage::Error::WalletNotFound)?;
        let (wallet, key) = storage::decrypt_password::<types::WalletContent>(
            self.encrypt_salt_length,
            self.encrypt_iv_length,
            self.encrypt_hash_iterations,
            password.as_bytes(),
            encrypted.as_ref(),
        )?;

        Ok(types::UnlockedWallet { key, wallet })
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
