use actix::Message;

use std::marker::PhantomData;
use witnet_storage::error::StorageResult;
use witnet_storage::storage::Storable;

use super::{UnitStorageResult, ValueStorageResult};
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
    /// Create a `Put` message from raw bytes
    pub fn new(key: &'static [u8], value: Vec<u8>) -> Self {
        Put { key, value }
    }
    /// Create a `Put` message by converting the value into bytes
    pub fn from_value<T: Storable>(key: &'static [u8], value: &T) -> StorageResult<Self> {
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
