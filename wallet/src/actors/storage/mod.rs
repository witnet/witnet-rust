//! # Storage actor
//!
//! It is charge of managing the connection to the key-value database. This actor is blocking so it
//! must be used with a `SyncArbiter`.
use std::sync::Arc;

use actix::prelude::*;

use witnet_protected::ProtectedString;

use crate::{storage, wallet};

pub mod builder;
pub mod handlers;

pub use handlers::*;

/// Expose options for tunning the database.
pub type Options = rocksdb::Options;

/// Storage actor.
pub struct Storage {
    /// Holds the wallets ids in plain text, and the wallets information encrypted with a password.
    db: Arc<rocksdb::DB>,
    params: Arc<storage::Params>,
}

impl Storage {
    pub fn build<'a>() -> builder::Builder<'a> {
        builder::Builder::new()
    }

    pub fn new(params: Arc<storage::Params>, db: Arc<rocksdb::DB>) -> Self {
        Self { db, params }
    }

    pub fn get_wallet_infos(&self) -> Result<Vec<wallet::WalletInfo>, storage::Error> {
        let ids = self.get_wallet_ids()?;
        let len = ids.len();
        let infos = ids
            .into_iter()
            .try_fold(Vec::with_capacity(len), |mut acc, id| {
                let info = storage::get(self.db.as_ref(), storage::keys::wallet_info(id.as_ref()))?;
                acc.push(info);

                Ok(acc)
            })?;

        Ok(infos)
    }

    pub fn get_wallet_ids(&self) -> Result<Vec<wallet::WalletId>, storage::Error> {
        storage::get_default(self.db.as_ref(), storage::keys::wallets())
    }

    pub fn create_wallet(
        &self,
        wallet: wallet::Wallet,
        password: ProtectedString,
    ) -> Result<(), storage::Error> {
        let mut batch = rocksdb::WriteBatch::default();
        let id = &wallet.info.id;

        storage::merge(&mut batch, storage::keys::wallets(), id)?;
        storage::put(&mut batch, storage::keys::wallet_info(id), &wallet.info)?;
        storage::put(
            &mut batch,
            storage::keys::wallet(id),
            &storage::encrypt(self.params.as_ref(), password.as_ref(), &wallet.content)?,
        )?;

        storage::write(self.db.as_ref(), batch)?;

        Ok(())
    }
}

impl Actor for Storage {
    type Context = SyncContext<Self>;
}

impl Supervised for Storage {}
