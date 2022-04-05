//! # HashMap storage backend
//!
//! Storage backend that keeps data in a heap-allocated HashMap.
use std::{
    collections::HashMap,
    sync::{RwLock, RwLockReadGuard},
};

use crate::storage::{Result, Storage, StorageIterator, WriteBatch, WriteBatchItem};

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

    fn prefix_iterator<'a, 'b: 'a>(&'a self, prefix: &'b [u8]) -> Result<StorageIterator<'a>> {
        Ok(Box::new(DBIterator {
            data: self.read().unwrap(),
            prefix,
            skip: 0,
        }))
    }

    fn write(&self, batch: WriteBatch) -> Result<()> {
        let mut map = self.write().unwrap();

        for item in batch.batch {
            match item {
                WriteBatchItem::Put(key, value) => {
                    map.insert(key, value);
                }
                WriteBatchItem::Delete(key) => {
                    map.remove(&key);
                }
            }
        }

        Ok(())
    }
}

struct DBIterator<'a, 'b> {
    data: RwLockReadGuard<'a, HashMap<Vec<u8>, Vec<u8>>>,
    prefix: &'b [u8],
    skip: usize,
}

impl<'a, 'b> Iterator for DBIterator<'a, 'b> {
    type Item = (Vec<u8>, Vec<u8>);

    fn next(&mut self) -> Option<Self::Item> {
        // TODO: is this correct? Add tests
        let mut skip = self.skip;
        let res = self
            .data
            .iter()
            .skip(skip)
            .map(|x| {
                skip += 1;
                x
            })
            .filter_map(|(k, v)| {
                if k.starts_with(self.prefix.as_ref()) {
                    Some((k.clone(), v.clone()))
                } else {
                    None
                }
            })
            .next();
        self.skip = skip;
        res
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
