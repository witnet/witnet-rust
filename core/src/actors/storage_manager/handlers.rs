use actix::{Context, Handler};

use witnet_storage::error::{StorageError, StorageErrorKind};
use witnet_storage::storage::{Storable, Storage, StorageHelper};
use witnet_util::error::WitnetError;

use super::{
    messages::{Delete, Get, Put},
    StorageManager, UnitStorageResult, ValueStorageResult,
};

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
