use actix::{Actor, Context, Handler, Message, Supervised, SystemService};

use witnet_storage::backends::rocks::RocksStorage;
use witnet_storage::error::StorageResult;
use witnet_storage::storage::Storage;

/// Message to indicate that a value is requested from the storage
pub struct Get {
    /// Requested key
    pub key: &'static [u8],
}

impl Message for Get {
    type Result = StorageResult<Option<Vec<u8>>>;
}

/// Message to indicate that a key-value pair needs to be inserted in the storage
pub struct Put {
    /// Key to be inserted
    pub key: &'static [u8],

    /// Value to be inserted
    pub value: Vec<u8>,
}

impl Message for Put {
    type Result = StorageResult<()>;
}

/// Message to indicate that a key-value pair needs to be removed from the storage
pub struct Delete {
    /// Key to be deleted
    pub key: &'static [u8],
}

impl Message for Delete {
    type Result = StorageResult<()>;
}

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

/// Make actor from `StorageManager`
impl Actor for StorageManager {
    /// Every actor has to provide execution `Context` in which it can run.
    type Context = Context<Self>;
}

/// Required traits for being able to retrieve storage manager address from registry
impl Supervised for StorageManager {}

impl SystemService for StorageManager {
    fn service_started(&mut self, _ctx: &mut Context<Self>) {}
}

/// Handler for Get message.
impl Handler<Get> for StorageManager {
    type Result = StorageResult<Option<Vec<u8>>>;

    fn handle(&mut self, msg: Get, _: &mut Context<Self>) -> Self::Result {
        self.storage.as_ref().unwrap().get(msg.key)
    }
}

/// Handler for Put message.
impl Handler<Put> for StorageManager {
    type Result = StorageResult<()>;

    fn handle(&mut self, msg: Put, _: &mut Context<Self>) -> Self::Result {
        self.storage.as_mut().unwrap().put(msg.key, msg.value)
    }
}

/// Handler for Delete message.
impl Handler<Delete> for StorageManager {
    type Result = StorageResult<()>;

    fn handle(&mut self, msg: Delete, _: &mut Context<Self>) -> Self::Result {
        self.storage.as_mut().unwrap().delete(msg.key)
    }
}
