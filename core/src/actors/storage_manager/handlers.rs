use actix::{Context, Handler};

use witnet_storage::{
    error::{StorageError, StorageErrorKind, StorageResult},
    storage::{Storable, Storage, StorageHelper},
};
use witnet_util::error::WitnetError;

use super::StorageManager;
use crate::actors::messages::{Delete, Get, Put};

/// Type aliases for the storage manager results returned
type ValueStorageResult<T> = StorageResult<Option<T>>;
type UnitStorageResult = StorageResult<()>;

/// Handler for Get message.
impl<T: Storable + 'static> Handler<Get<T>> for StorageManager {
    type Result = ValueStorageResult<T>;

    fn handle(&mut self, msg: Get<T>, _: &mut Context<Self>) -> Self::Result {
        self.storage.as_ref().map_or(
            Err(WitnetError::from(StorageError::new(
                StorageErrorKind::Get,
                format!("{:?}", msg.key),
                "Storage was not properly initialised".to_string(),
            ))),
            |storage| storage.get_t(&msg.key),
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
                format!("{:?}", msg.key),
                "Storage was not properly initialised".to_string(),
            ))),
            |storage| storage.put(&msg.key, msg.value),
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
                format!("{:?}", msg.key),
                "Storage was not properly initialised".to_string(),
            ))),
            |storage| storage.delete(&msg.key),
        )
    }
}
