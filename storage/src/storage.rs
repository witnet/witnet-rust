//! Module containing a `Storage` generic trait that can be implemented for different specific storage
//! backends, and a `Storable` trait which marks types as being convertible to bytes for storage.

use crate::error::{StorageError, StorageErrorKind, StorageResult};
use serde::{de::DeserializeOwned, Serialize};
use std::fmt::Debug;
use witnet_util::error::WitnetError;

/// This is a generic trait that exposes a very simple key/value CRUD API for data storage.
/// This trait can be easily implemented for any specific storage backend solution (databases,
/// volatile memory, flat files, etc.)
pub trait Storage<ConnData: Debug, Key, Value> {
    /// Storage object constructor.
    /// `connection_data` can be used for passing credentials, urls, paths, etc. down to the storage
    /// backend.
    fn new(connection_data: ConnData) -> StorageResult<Box<Self>>;

    /// Create / update entries in the storage, identified by a key.
    fn put(&mut self, key: Key, value: Value) -> StorageResult<()>;

    /// Retrieve an entry from the storage, identified by its key.
    fn get(&self, key: Key) -> StorageResult<Option<Value>>;

    /// Delete an entry from the storage, identified by its key.
    fn delete(&mut self, key: Key) -> StorageResult<()>;
}

/// Trait which marks a type as storable.
/// The simplest way to implement this trait
/// is to add `#[derive(Serialize, Deserialize)]` to the type definition.
/// The storage works on raw bytes, so we need to serialize and deserialize the data.
/// The default implementation uses MessagePack, but the implementor is free to choose
/// a different encoding for their custom types.
pub trait Storable: Sized {
    /// Convert `Self` into `Vec<u8>`
    fn to_bytes(&self) -> StorageResult<Vec<u8>>;
    /// Convert `Vec<u8>` into `Self
    fn from_bytes(x: &[u8]) -> StorageResult<Self>;
}

// By default, mark all the types which can be serialized and deserialized
// using serde as `Storable`
impl<T> Storable for T
where
    T: Serialize + DeserializeOwned,
{
    /// Convert `Self` into `Vec<u8>`
    fn to_bytes(&self) -> StorageResult<Vec<u8>> {
        rmp_serde::to_vec(&self).map_err(|e| {
            WitnetError::from(StorageError::new(
                StorageErrorKind::Encode,
                "Error when encoding value".to_string(),
                format!("{}", e),
            ))
        })
    }
    /// Convert `Vec<u8>` into `Self
    fn from_bytes(x: &[u8]) -> StorageResult<Self> {
        rmp_serde::from_slice(x).map_err(|e| {
            WitnetError::from(StorageError::new(
                StorageErrorKind::Decode,
                "Error when decoding value".to_string(),
                format!("{}", e),
            ))
        })
    }
}

/// Helper trait to work on types instead of raw bytes.
/// The type must implement the `Storable` trait.
///
/// Usage example:
///
/// ```
/// use witnet_storage::storage::{Storage, StorageHelper};
/// use witnet_storage::error::StorageResult;
/// use witnet_storage::backends::in_memory::InMemoryStorage;
///
/// fn main() -> StorageResult<()> {
///     let mut s = InMemoryStorage::new(())?;
///     let v1: String = "hello!".to_string();
///     s.put_t(b"str", v1.clone())?;
///     let v2: String = s.get_t(b"str")?.unwrap();
///     assert_eq!(v1, v2);
///
///     let x1: i32 = 54;
///     s.put_t(b"int", x1.clone())?;
///     let x2 = s.get_t::<i32>(b"int")?.unwrap();
///     assert_eq!(x1, x2);
///
///     Ok(())
/// }
/// ```
///
/// It is the caller's responsibility to make sure that the type signature is correct,
/// as trying to get a value of an incorrect type may lead to unexpected behaviour.
pub trait StorageHelper<'a, ConnData: Debug>: Storage<ConnData, &'a [u8], Vec<u8>> {
    /// Insert an element into the storage
    fn put_t<T: Storable>(&mut self, key: &'a [u8], value: T) -> StorageResult<()> {
        match value.to_bytes() {
            Ok(v) => self.put(key, v),
            Err(e) => Err(WitnetError::from(StorageError::new(
                StorageErrorKind::Put,
                format!("Key: {:?}", key),
                format!("Failed to convert value into bytes: {:?}", e),
            ))),
        }
    }
    /// Get an element from the storage
    fn get_t<T: Storable>(&self, key: &'a [u8]) -> StorageResult<Option<T>> {
        let value = self.get(key)?;
        if value.is_none() {
            return Ok(None);
        }
        let value = value.unwrap();
        match T::from_bytes(&value) {
            Ok(v) => Ok(Some(v)),
            Err(e) => Err(WitnetError::from(StorageError::new(
                StorageErrorKind::Get,
                format!("Key: {:?}", key),
                format!("Failed to create value from bytes: {:?}", e),
            ))),
        }
    }
}

// Implement the above helper trait for all the storage backends that work on raw bytes
impl<'a, ConnData: Debug, T> StorageHelper<'a, ConnData> for T where
    T: Storage<ConnData, &'a [u8], Vec<u8>>
{
}
