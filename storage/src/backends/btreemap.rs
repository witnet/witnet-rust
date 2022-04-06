//! # BTreeMap storage backend
//!
//! Storage backend that keeps data in a heap-allocated BTreeMap.
use std::{
    collections::BTreeMap,
    sync::{RwLock, RwLockReadGuard},
};

use crate::storage::{Result, Storage, StorageIterator, WriteBatch, WriteBatchItem};

/// BTreeMap backend
pub type Backend = RwLock<BTreeMap<Vec<u8>, Vec<u8>>>;

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
        self.prefix_iterator_forward(prefix)
    }

    fn prefix_iterator_forward<'a, 'b: 'a>(
        &'a self,
        prefix: &'b [u8],
    ) -> Result<StorageIterator<'a>> {
        Ok(Box::new(DBIteratorSorted {
            data: self.read().unwrap(),
            prefix,
            skip: 0,
            reverse: false,
        }))
    }

    fn prefix_iterator_reverse<'a, 'b: 'a>(
        &'a self,
        prefix: &'b [u8],
    ) -> Result<StorageIterator<'a>> {
        Ok(Box::new(DBIteratorSorted {
            data: self.read().unwrap(),
            prefix,
            skip: 0,
            reverse: true,
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

struct DBIteratorSorted<'a, 'b> {
    data: RwLockReadGuard<'a, BTreeMap<Vec<u8>, Vec<u8>>>,
    prefix: &'b [u8],
    skip: usize,
    reverse: bool,
}

impl<'a, 'b> Iterator for DBIteratorSorted<'a, 'b> {
    type Item = (Vec<u8>, Vec<u8>);

    fn next(&mut self) -> Option<Self::Item> {
        let mut skip = self.skip;
        let res = if !self.reverse {
            self.data
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
                .next()
        } else {
            self.data
                .iter()
                .rev()
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
                .next()
        };
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
    fn test_btreemap() {
        let storage = backend();

        assert_eq!(None, storage.get(b"name").unwrap());
        storage.put(b"name".to_vec(), b"john".to_vec()).unwrap();
        assert_eq!(Some("john".into()), storage.get(b"name").unwrap());
        storage.delete(b"name").unwrap();
        assert_eq!(None, storage.get(b"name").unwrap());
    }

    #[test]
    fn test_iterator_forward() {
        let storage = backend();

        storage
            .put(b"prefix-a".to_vec(), b"alice".to_vec())
            .unwrap();
        storage.put(b"prefix-b".to_vec(), b"bob".to_vec()).unwrap();
        storage.put(b"noprefix".to_vec(), b"eve".to_vec()).unwrap();

        let iter: Vec<_> = storage
            .prefix_iterator_forward(b"prefix-")
            .unwrap()
            .collect();

        assert_eq!(
            iter,
            vec![
                (b"prefix-a".to_vec(), b"alice".to_vec()),
                (b"prefix-b".to_vec(), b"bob".to_vec())
            ]
        );
    }

    #[test]
    fn test_iterator_reverse() {
        let storage = backend();

        storage
            .put(b"prefix-a".to_vec(), b"alice".to_vec())
            .unwrap();
        storage.put(b"prefix-b".to_vec(), b"bob".to_vec()).unwrap();
        storage.put(b"noprefix".to_vec(), b"eve".to_vec()).unwrap();

        let iter: Vec<_> = storage
            .prefix_iterator_reverse(b"prefix-")
            .unwrap()
            .collect();

        assert_eq!(
            iter,
            vec![
                (b"prefix-b".to_vec(), b"bob".to_vec()),
                (b"prefix-a".to_vec(), b"alice".to_vec())
            ]
        );
    }
}
