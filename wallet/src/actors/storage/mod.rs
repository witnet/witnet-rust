//! # Storage actor
//!
//! It is charge of managing the connection to the key-value database. This actor is blocking so it
//! must be used with a `SyncArbiter`.
use bincode::deserialize;
use failure::Error;
use std::env;
use std::path::PathBuf;
use std::sync::Arc;

use actix::prelude::*;

use crate::wallet;

pub mod error;
mod handlers;

pub use handlers::*;

/// Expose options for tunning the database.
pub type Options = rocksdb::Options;

/// Storage actor.
pub struct Storage {
    /// Holds the wallets ids in plain text, and the wallets information encrypted with a password.
    db: Arc<rocksdb::DB>,
}

impl Storage {
    pub fn build<'a>() -> StorageBuilder<'a> {
        StorageBuilder::default()
    }

    pub fn new(db: Arc<rocksdb::DB>) -> Self {
        Self { db }
    }

    pub fn get_wallet_infos(&self) -> Result<Vec<wallet::WalletInfo>, error::Error> {
        let result = self
            .db
            .get("wallet-infos")
            .map_err(error::Error::DbGetFailed)?;

        match result {
            Some(db_vec) => {
                let value =
                    deserialize(db_vec.as_ref()).map_err(error::Error::DeserializeFailed)?;
                Ok(value)
            }
            None => Ok(Vec::new()),
        }
    }
}

#[derive(Default)]
pub struct StorageBuilder<'a> {
    options: Option<rocksdb::Options>,
    path: Option<PathBuf>,
    name: Option<&'a str>,
}

impl<'a> StorageBuilder<'a> {
    /// Create a new instance of the Storage actor using the given database options.
    pub fn with_options(mut self, options: rocksdb::Options) -> Self {
        self.options = Some(options);
        self
    }

    /// Set the path where to store the database files.
    pub fn with_path(mut self, path: PathBuf) -> Self {
        self.path = Some(path);
        self
    }

    /// Set the filename of the database.
    pub fn with_file_name(mut self, name: &'a str) -> Self {
        self.name = Some(name);
        self
    }

    /// Start an instance of the actor inside a SyncArbiter.
    pub fn start(self) -> Result<Addr<Storage>, Error> {
        let options = self.options.unwrap_or_default();
        let path = self.path.map_or_else(env::current_dir, Ok)?;
        let file_name = self.name.unwrap_or_else(|| "witnet_wallets.db");
        let db = rocksdb::DB::open(&options, path.join(file_name))
            .map_err(error::Error::OpenDbFailed)?;
        let db_ref = Arc::new(db);

        // Spawn one thread with the storage actor (because is blocking). Do not use more than one
        // thread, otherwise you'll receive and error because RocksDB only allows one connection at a
        // time.
        let addr = SyncArbiter::start(1, move || Storage::new(db_ref.clone()));

        Ok(addr)
    }
}

impl Actor for Storage {
    type Context = SyncContext<Self>;
}

impl Supervised for Storage {}
