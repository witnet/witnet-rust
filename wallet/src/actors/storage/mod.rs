//! # Storage actor
//!
//! It is charge of managing the connection to the key-value database. This actor is blocking so it
//! must be used with a `SyncArbiter`.

use std::path::PathBuf;

use actix::prelude::*;

mod error;
mod handlers;

pub use error::Error;
pub use handlers::*;

/// Storage actor.
pub struct Storage {
    /// Path where to store the wallet database.
    /// This is used during the wallet generation step.
    // TODO: remove this allow attribute when the wallet-database is implemented.
    #[allow(dead_code)]
    db_path: PathBuf,
    wallets: Result<rocksdb::DB, rocksdb::Error>,
    /// It holds the instance of the currently open wallet database.
    // TODO: remove this allow attribute when the wallet-database is implemented and the client is
    // able to "open" a wallet.
    #[allow(dead_code)]
    wallet: Option<Result<rocksdb::DB, rocksdb::Error>>,
}

impl Storage {
    /// Create a new instance of the Storage actor.
    ///
    /// It receives a path where to store:
    ///
    /// 1. The wallets database, which contains metadata about all created wallets.
    /// 2. Each individual wallet database that is created.
    pub fn new(db_path: PathBuf) -> Self {
        let mut options = rocksdb::Options::default();

        options.create_if_missing(true);

        Self::with_options(db_path, options)
    }

    /// Create a new instance of the Storage actor using the given database options.
    pub fn with_options(db_path: PathBuf, options: rocksdb::Options) -> Self {
        let wallets = rocksdb::DB::open(&options, db_path.join("wallets"));

        Self {
            db_path,
            wallets,
            wallet: None,
        }
    }

    /// Start an instance of the actor inside a SyncArbiter.
    pub fn start(db_path: PathBuf) -> Addr<Self> {
        // Spawn one thread with the storage actor (because is blocking). Do not use more than one
        // thread, otherwise you'll receive and error because RocksDB only allows one connection at a
        // time.
        SyncArbiter::start(1, move || Self::new(db_path.clone()))
    }
}

impl Actor for Storage {
    type Context = SyncContext<Self>;
}

impl Supervised for Storage {}
