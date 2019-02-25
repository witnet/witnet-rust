use actix::{Context, Supervised, SystemService};

use witnet_storage::{backends::rocks::RocksStorage, storage::Storage};

mod actor;
mod handlers;

/// Storage manager actor
#[derive(Default)]
pub struct StorageManager {
    /// DB storage
    storage: Option<RocksStorage>,
}

impl StorageManager {
    /// Method to create a new storage manager
    pub fn new(db_root: &str) -> StorageManager {
        // Build rocks db storage
        match RocksStorage::new(db_root.to_string()) {
            Ok(db) => StorageManager { storage: Some(*db) },
            Err(_) => StorageManager { storage: None },
        }
    }
}

/// Required traits for being able to retrieve storage manager address from registry
impl Supervised for StorageManager {}

impl SystemService for StorageManager {
    fn service_started(&mut self, _ctx: &mut Context<Self>) {}
}
