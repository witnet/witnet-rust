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
        self.prefix_iterator_forward(prefix)
    }

    fn prefix_iterator_forward<'a, 'b: 'a>(
        &'a self,
        prefix: &'b [u8],
    ) -> Result<StorageIterator<'a>> {
        Ok(Box::new(
            Backend::iterator(
                self,
                rocksdb::IteratorMode::From(prefix, rocksdb::Direction::Forward),
            )
            .take_while(move |(k, _v)| k.starts_with(prefix))
            .map(|(k, v)| (k.into(), v.into())),
        ))
    }

    fn prefix_iterator_reverse<'a, 'b: 'a>(
        &'a self,
        prefix: &'b [u8],
    ) -> Result<StorageIterator<'a>> {
        Ok(Box::new(
            Backend::iterator(
                self,
                rocksdb::IteratorMode::From(prefix, rocksdb::Direction::Reverse),
            )
            .take_while(move |(k, _v)| k.starts_with(prefix))
            .map(|(k, v)| (k.into(), v.into())),
        ))
    }

    /// Atomically write a batch of operations
    fn write(&self, batch: WriteBatch) -> Result<()> {
        self.write(batch.into())?;

        Ok(())
    }
}

impl From<WriteBatch> for rocksdb::WriteBatch {
    fn from(batch: WriteBatch) -> Self {
        let mut rocksdb_batch = rocksdb::WriteBatch::default();

        for item in batch.batch {
            match item {
                WriteBatchItem::Put(key, value) => {
                    rocksdb_batch.put(key, value).unwrap();
                }
                WriteBatchItem::Delete(key) => {
                    rocksdb_batch.delete(key).unwrap();
                }
            }
        }

        rocksdb_batch
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn backend() -> Box<dyn Storage> {
        Box::new(Backend::open_default("test_rocksdb_path").unwrap())
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
    use std::path::Path;

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

    pub struct DB {
        backend: crate::backends::btreemap::Backend,
    }

    impl DB {
        /// `path` is not used by this mock because the database will be in memory
        pub fn open_default<P: AsRef<Path>>(_path: P) -> Result<Self> {
            Ok(DB {
                backend: Default::default(),
            })
        }

        pub fn get<K: AsRef<[u8]>>(&self, key: &K) -> Result<Option<Vec<u8>>> {
            self.backend.get(key.as_ref())
        }

        pub fn put<K: AsRef<[u8]>, V: AsRef<[u8]>>(&self, key: K, value: V) -> Result<()> {
            self.backend
                .put(key.as_ref().to_vec(), value.as_ref().to_vec())
        }

        pub fn delete<K: AsRef<[u8]>>(&self, key: &K) -> Result<()> {
            self.backend.delete(key.as_ref())
        }

        pub fn iterator<'a, 'b: 'a>(
            &'a self,
            iterator_mode: IteratorMode<'b>,
        ) -> impl Iterator<Item = (Box<[u8]>, Box<[u8]>)> + 'a {
            match iterator_mode {
                IteratorMode::Start => unimplemented!(),
                IteratorMode::End => unimplemented!(),
                IteratorMode::From(prefix, Direction::Forward) => {
                    self.backend.prefix_iterator_forward(prefix).unwrap()
                }
                IteratorMode::From(prefix, Direction::Reverse) => {
                    self.backend.prefix_iterator_reverse(prefix).unwrap()
                }
            }
            .map(|(k, v)| (k.into(), v.into()))
        }

        pub fn write(&self, batch: WriteBatch) -> Result<()> {
            Storage::write(&self.backend, batch.into())
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

    impl From<crate::backends::rocksdb::rocksdb_mock::WriteBatch> for crate::storage::WriteBatch {
        fn from(x: WriteBatch) -> Self {
            x.inner
        }
    }
}
