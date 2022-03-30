//! # Rocksdb storage backend
//!
//! Storage backend that persists data in the file system using a RocksDB database.
use failure::Fail;
#[cfg(test)]
use rocksdb_mock as rocksdb;

use crate::storage::{Result, Storage, StorageIterator, WriteBatch, WriteBatchItem};

/// Rocksdb backend
pub type Backend = rocksdb::DB;

#[derive(Debug, Fail)]
#[fail(display = "RocksDB error: {}", _0)]
struct Error(#[fail(cause)] rocksdb::Error);

impl Storage for Backend {
    fn get(&self, key: &[u8]) -> Result<Option<Vec<u8>>> {
        let result = Backend::get(self, &key)
            .map(|opt| opt.map(|dbvec| dbvec.to_vec()))
            .map_err(Error)?;
        Ok(result)
    }

    fn put(&self, key: Vec<u8>, value: Vec<u8>) -> Result<()> {
        Backend::put(self, key, value).map_err(Error)?;
        Ok(())
    }

    fn delete(&self, key: &[u8]) -> Result<()> {
        Backend::delete(self, &key).map_err(Error)?;
        Ok(())
    }

    fn prefix_iterator<'a, 'b: 'a>(&'a self, prefix: &'b [u8]) -> Result<StorageIterator<'a>> {
        Ok(Box::new(
            Backend::iterator(
                self,
                rocksdb::IteratorMode::From(prefix, rocksdb::Direction::Forward),
            )
            .take_while(move |(k, _v)| k.starts_with(prefix))
            .map(|(k, v)| (k.into(), v.into())),
        ))
    }
    /// Atomically write a batch of operations
    fn write(&self, batch: WriteBatch) -> Result<()> {
        let mut rocksdb_batch = rocksdb::WriteBatch::default();

        for item in batch.batch {
            match item {
                WriteBatchItem::Put(key, value) => {
                    rocksdb_batch.put(key, value)?;
                }
                WriteBatchItem::Delete(key) => {
                    rocksdb_batch.delete(key)?;
                }
            }
        }

        self.write(rocksdb_batch)?;

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
    fn test_rocksdb() {
        let storage = backend();

        assert_eq!(None, storage.get(b"name").unwrap());
        storage.put(b"name".to_vec(), b"john".to_vec()).unwrap();
        assert_eq!(Some("john".into()), storage.get(b"name").unwrap());
        storage.delete(b"name").unwrap();
        assert_eq!(None, storage.get(b"name").unwrap());
    }
}

#[cfg(test)]
mod rocksdb_mock {
    use super::*;
    use std::sync::{RwLock, RwLockReadGuard};

    pub type Error = failure::Error;

    pub enum IteratorMode<'a> {
        Start,
        End,
        From(&'a [u8], Direction),
    }

    pub enum Direction {
        Forward,
        Reverse,
    }

    #[derive(Default)]
    pub struct DB {
        data: RwLock<Vec<(Vec<u8>, Vec<u8>)>>,
    }

    impl DB {
        pub fn new() -> Self {
            DB::default()
        }

        fn search<K: AsRef<[u8]>>(&self, key: &K) -> Option<usize> {
            for (i, (k, _)) in self.data.read().unwrap().iter().enumerate() {
                if key.as_ref() == k.as_slice() {
                    return Some(i);
                }
            }
            None
        }

        pub fn get<K: AsRef<[u8]>>(&self, key: &K) -> Result<Option<Vec<u8>>> {
            Ok(self
                .search(key)
                .map(|idx| self.data.read().unwrap()[idx].1.clone()))
        }

        pub fn put<K: AsRef<[u8]>, V: AsRef<[u8]>>(&self, key: K, value: V) -> Result<()> {
            match self.search(&key) {
                Some(idx) => self.data.write().unwrap()[idx].1 = value.as_ref().to_vec(),
                None => self
                    .data
                    .write()
                    .unwrap()
                    .push((key.as_ref().to_vec(), value.as_ref().to_vec())),
            }
            Ok(())
        }

        pub fn delete<K: AsRef<[u8]>>(&self, key: &K) -> Result<()> {
            self.search(key)
                .map(|idx| self.data.write().unwrap().remove(idx));
            Ok(())
        }

        pub fn iterator<'a, 'b: 'a>(
            &'a self,
            iterator_mode: IteratorMode<'b>,
        ) -> DBIterator<'a, 'b> {
            match iterator_mode {
                IteratorMode::Start => unimplemented!(),
                IteratorMode::End => unimplemented!(),
                IteratorMode::From(prefix, _direction) => self.prefix_iterator(prefix),
            }
        }

        pub fn prefix_iterator<'a, 'b: 'a, P: AsRef<[u8]> + ?Sized>(
            &'a self,
            prefix: &'b P,
        ) -> DBIterator<'a, 'b> {
            DBIterator {
                data: self.data.read().unwrap(),
                prefix: prefix.as_ref(),
                skip: 0,
            }
        }

        pub fn write(&self, batch: WriteBatch) -> Result<()> {
            // TODO: this is not atomic, but it shouldn't matter as it is not used, not even in tests
            for item in batch.inner.batch {
                match item {
                    WriteBatchItem::Put(key, value) => {
                        self.put(key, value)?;
                    }
                    WriteBatchItem::Delete(key) => {
                        self.delete(&key)?;
                    }
                }
            }

            Ok(())
        }
    }

    pub struct DBIterator<'a, 'b> {
        data: RwLockReadGuard<'a, Vec<(Vec<u8>, Vec<u8>)>>,
        prefix: &'b [u8],
        skip: usize,
    }

    impl<'a, 'b> Iterator for DBIterator<'a, 'b> {
        type Item = (Box<[u8]>, Box<[u8]>);

        fn next(&mut self) -> Option<Self::Item> {
            // TODO: is this even used somewhere?
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
                        Some((k.clone().into_boxed_slice(), v.clone().into_boxed_slice()))
                    } else {
                        None
                    }
                })
                .next();
            self.skip = skip;
            res
        }
    }

    #[derive(Default)]
    pub struct WriteBatch {
        inner: super::WriteBatch,
    }

    impl WriteBatch {
        pub fn put(&mut self, key: Vec<u8>, value: Vec<u8>) -> Result<()> {
            self.inner.put(key, value);

            Ok(())
        }

        pub fn delete(&mut self, key: Vec<u8>) -> Result<()> {
            self.inner.delete(key);

            Ok(())
        }
    }
}
