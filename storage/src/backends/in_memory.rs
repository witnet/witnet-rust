//! Storage backend that keeps data in a heap-allocated HashMap.
//!
//! Please note that this backend lacks persistence. Data is preserved only for the lifetime of
//! references to the storage object.

use crate::error::StorageResult;
use crate::storage::Storage;
use std::collections::HashMap;

/// Data structure for the in-memory storage.
/// Only member is a HashMap that uses references to u8 slices as keys and vectors of u8 as values.
#[derive(Debug, Eq, PartialEq)]
pub struct InMemoryStorage<'a> {
    /// A HashMap to implement easy and fast K/V lookup
    pub memory: HashMap<&'a [u8], Vec<u8>>,
}

/// Implement the Storage generic trait for the InMemoryStorage storage data structure.
impl<'a> Storage<&'a [u8], Vec<u8>> for InMemoryStorage<'a> {
    fn new(_: String) -> StorageResult<Box<Self>> {
        Ok(Box::new(InMemoryStorage {
            memory: HashMap::new(),
        }))
    }

    fn put(&mut self, key: &'a [u8], value: Vec<u8>) -> StorageResult<()> {
        self.memory.insert(key, value);
        Ok(())
    }

    fn get(&self, key: &[u8]) -> StorageResult<Option<Vec<u8>>> {
        Ok(self.memory.get(key).map(|value| value.to_owned()))
    }

    fn delete(&mut self, key: &[u8]) -> StorageResult<()> {
        self.memory.remove(key);
        Ok(())
    }
}
