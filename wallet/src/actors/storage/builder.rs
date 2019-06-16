use std::env;
use std::path::PathBuf;
use std::sync::Arc;

use actix::prelude::*;
use failure::Error;

use super::Storage;
use crate::storage;

pub struct Builder<'a> {
    options: Option<rocksdb::Options>,
    path: Option<PathBuf>,
    name: Option<&'a str>,
    params: storage::Params,
}

impl<'a> Builder<'a> {
    pub fn new() -> Self {
        let params = storage::Params {
            encrypt_hash_iterations: 10_000,
            encrypt_iv_length: 16,
            encrypt_salt_length: 32,
        };

        Self {
            params,
            path: None,
            name: None,
            options: None,
        }
    }
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
        let mut options = self.options.unwrap_or_default();
        options.set_merge_operator("merge operator", storage::storage_merge, None);
        let path = self.path.map_or_else(env::current_dir, Ok)?;
        let file_name = self.name.unwrap_or_else(|| "witnet_wallets.db");
        let db = rocksdb::DB::open(&options, path.join(file_name))
            .map_err(storage::Error::OpenDbFailed)?;
        let db_ref = Arc::new(db);
        let params_ref = Arc::new(self.params);

        // Spawn one thread with the storage actor (because is blocking). Do not use more than one
        // thread, otherwise you'll receive and error because RocksDB only allows one connection at a
        // time.
        let addr = SyncArbiter::start(1, move || Storage::new(params_ref.clone(), db_ref.clone()));

        Ok(addr)
    }
}
