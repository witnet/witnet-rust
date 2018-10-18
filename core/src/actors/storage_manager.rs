use actix::{Actor, ActorContext, Context, Handler, Message, Supervised, SystemService};

use witnet_storage::backends::rocks::RocksStorage;
use witnet_storage::error::{StorageError, StorageErrorKind, StorageResult};
use witnet_storage::storage::Storage;
use witnet_util::error::WitnetError;

/// Type aliases for the storage manager results returned
type ValueStorageResult = StorageResult<Option<Vec<u8>>>;
type UnitStorageResult = StorageResult<()>;

/// Message to indicate that a value is requested from the storage
pub struct Get {
    /// Requested key
    pub key: &'static [u8],
}

impl Message for Get {
    type Result = ValueStorageResult;
}

/// Message to indicate that a key-value pair needs to be inserted in the storage
pub struct Put {
    /// Key to be inserted
    pub key: &'static [u8],

    /// Value to be inserted
    pub value: Vec<u8>,
}

impl Message for Put {
    type Result = UnitStorageResult;
}

/// Message to indicate that a key-value pair needs to be removed from the storage
pub struct Delete {
    /// Key to be deleted
    pub key: &'static [u8],
}

impl Message for Delete {
    type Result = UnitStorageResult;
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

    /// Method to be executed when the actor is started
    fn started(&mut self, ctx: &mut Self::Context) {
        // Stop context if the storage is not properly initialized
        if self.storage.is_none() {
            ctx.stop();
        }
    }
}

/// Required traits for being able to retrieve storage manager address from registry
impl Supervised for StorageManager {}

impl SystemService for StorageManager {
    fn service_started(&mut self, _ctx: &mut Context<Self>) {}
}

/// Handler for Get message.
impl Handler<Get> for StorageManager {
    type Result = ValueStorageResult;

    fn handle(&mut self, msg: Get, _: &mut Context<Self>) -> Self::Result {
        self.storage.as_ref().map_or(
            Err(WitnetError::from(StorageError::new(
                StorageErrorKind::Get,
                String::from_utf8(msg.key.to_vec()).unwrap(),
                "Storage was not properly initialised".to_string(),
            ))),
            |storage| storage.get(msg.key),
        )
    }
}

/// Handler for Put message.
impl Handler<Put> for StorageManager {
    type Result = UnitStorageResult;

    fn handle(&mut self, msg: Put, _: &mut Context<Self>) -> Self::Result {
        self.storage.as_mut().map_or(
            Err(WitnetError::from(StorageError::new(
                StorageErrorKind::Put,
                String::from_utf8(msg.key.to_vec()).unwrap(),
                "Storage was not properly initialised".to_string(),
            ))),
            |storage| storage.put(msg.key, msg.value),
        )
    }
}

/// Handler for Delete message.
impl Handler<Delete> for StorageManager {
    type Result = UnitStorageResult;

    fn handle(&mut self, msg: Delete, _: &mut Context<Self>) -> Self::Result {
        self.storage.as_mut().map_or(
            Err(WitnetError::from(StorageError::new(
                StorageErrorKind::Delete,
                String::from_utf8(msg.key.to_vec()).unwrap(),
                "Storage was not properly initialised".to_string(),
            ))),
            |storage| storage.delete(msg.key),
        )
    }
}
