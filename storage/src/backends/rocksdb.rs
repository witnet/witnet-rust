//! # Rocksdb storage backend
//!
//! Storage backend that persists data in the file system using a RocksDB database.
use failure::Fail;
#[cfg(test)]
use rocksdb_mock as rocksdb;

use crate::storage::{Result, Storage};

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
    use std::sync::RwLock;

    pub type Error = failure::Error;

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
    }
}
