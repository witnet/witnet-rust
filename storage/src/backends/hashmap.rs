//! # HashMap storage backend
//!
//! Storage backend that keeps data in a heap-allocated HashMap.
use std::collections::HashMap;

use crate::storage::{Result, Storage};
use std::sync::RwLock;

/// HashMap backend
pub type Backend = RwLock<HashMap<Vec<u8>, Vec<u8>>>;

impl Storage for Backend {
    fn get(&self, key: &[u8]) -> Result<Option<Vec<u8>>> {
        Ok(self.read().unwrap().get(key).map(|slice| slice.to_vec()))
    }

    fn put(&self, key: Vec<u8>, value: Vec<u8>) -> Result<()> {
        self.write().unwrap().insert(key, value);
        Ok(())
    }

    fn delete(&self, key: &[u8]) -> Result<()> {
        self.write().unwrap().remove(key);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn backend() -> Box<dyn Storage> {
        Box::new(Backend::default())
    }

    #[test]
    fn test_hashmap() {
        let storage = backend();

        assert_eq!(None, storage.get(b"name").unwrap());
        storage.put(b"name".to_vec(), b"john".to_vec()).unwrap();
        assert_eq!(Some("john".into()), storage.get(b"name").unwrap());
        storage.delete(b"name").unwrap();
        assert_eq!(None, storage.get(b"name").unwrap());
    }
}
