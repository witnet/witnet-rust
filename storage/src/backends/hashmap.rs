//! # HashMap storage backend
//!
//! Storage backend that keeps data in a heap-allocated HashMap.
use std::collections::HashMap;

use crate::storage::{Result, Storage};

/// HashMap backend
pub type Backend = HashMap<Vec<u8>, Vec<u8>>;

impl Storage for Backend {
    fn get(&self, key: &[u8]) -> Result<Option<Vec<u8>>> {
        Ok(Backend::get(self, key).map(|slice| slice.to_vec()))
    }

    fn put(&mut self, key: Vec<u8>, value: Vec<u8>) -> Result<()> {
        Backend::insert(self, key, value);
        Ok(())
    }

    fn delete(&mut self, key: &[u8]) -> Result<()> {
        Backend::remove(self, key);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn backend() -> Box<dyn Storage> {
        Box::new(Backend::new())
    }

    #[test]
    fn test_hashmap() {
        let mut storage = backend();

        assert_eq!(None, storage.get(b"name").unwrap());
        assert_eq!((), storage.put(b"name".to_vec(), b"john".to_vec()).unwrap());
        assert_eq!(Some("john".into()), storage.get(b"name").unwrap());
        assert_eq!((), storage.delete(b"name").unwrap());
        assert_eq!(None, storage.get(b"name").unwrap());
    }
}
