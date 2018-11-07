use actix::{Actor, ActorContext, Context, Handler, Message, Supervised, SystemService};

use crate::actors::config_manager::send_get_config_request;

use log::{debug, error};
use std::marker::PhantomData;
use witnet_storage::backends::rocks::RocksStorage;
use witnet_storage::error::{StorageError, StorageErrorKind, StorageResult};
use witnet_storage::storage::{Storable, Storage, StorageHelper};
use witnet_util::error::WitnetError;

/// Type aliases for the storage manager results returned
type ValueStorageResult<T> = StorageResult<Option<T>>;
type UnitStorageResult = StorageResult<()>;

/// Message to indicate that a value is requested from the storage
pub struct Get<T> {
    /// Requested key
    pub key: &'static [u8],
    _phantom: PhantomData<T>,
}

impl<T: Storable + 'static> Get<T> {
    /// Create a generic `Get` message which will try to convert the raw bytes from the storage
    /// into `T`
    pub fn new(key: &'static [u8]) -> Self {
        Get {
            key,
            _phantom: PhantomData,
        }
    }
}

impl<T: Storable + 'static> Message for Get<T> {
    type Result = ValueStorageResult<T>;
}

/// Message to indicate that a key-value pair needs to be inserted in the storage
pub struct Put {
    /// Key to be inserted
    pub key: &'static [u8],

    /// Value to be inserted
    pub value: Vec<u8>,
}

impl Put {
    /// Create a `Put` message by converting the value into bytes
    pub fn new<T: Storable>(key: &'static [u8], value: &T) -> StorageResult<Self> {
        let value = value.to_bytes()?;
        Ok(Put { key, value })
    }
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
        debug!("Storage Manager actor has been started!");

        // Send message to config manager and process response
        send_get_config_request(self, ctx, |s, ctx, config| {
            // Get db path from configuration
            let db_path = &config.storage.db_path;

            // Override actor
            *s = Self::new(&db_path.to_string_lossy());

            // Stop context if the storage is not properly initialized
            // FIXME(#72): check error handling
            if s.storage.is_none() {
                error!("Error initializing storage");
                ctx.stop();
            }
        });
    }
}

/// Required traits for being able to retrieve storage manager address from registry
impl Supervised for StorageManager {}

impl SystemService for StorageManager {
    fn service_started(&mut self, _ctx: &mut Context<Self>) {}
}

/// Handler for Get message.
impl<T: Storable + 'static> Handler<Get<T>> for StorageManager {
    type Result = ValueStorageResult<T>;

    fn handle(&mut self, msg: Get<T>, _: &mut Context<Self>) -> Self::Result {
        self.storage.as_ref().map_or(
            Err(WitnetError::from(StorageError::new(
                StorageErrorKind::Get,
                String::from_utf8(msg.key.to_vec()).unwrap(),
                "Storage was not properly initialised".to_string(),
            ))),
            |storage| storage.get_t(msg.key),
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
