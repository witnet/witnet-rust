//! Storage backend that persists data in the file system using a RocksDB database.

use crate::error::{Error, Result, StorageErrors};
use crate::storage::Storage;
use rocksdb::DB;
use std::str;

/// Data structure for the RocksDB storage whose only member is a rocksdb::DB object.
pub struct RocksStorage {
    db: DB
}

/// Implement the Storage generic trait for the RocksStorage storage data structure.
impl <'a> Storage<&'a str, &'a [u8], Vec<u8>> for RocksStorage {

    fn new(path: &str) -> Result<Box<Self>> {
        match DB::open_default(&path) {
            Ok(db) => {
                let storage = RocksStorage { db };
                Ok(Box::new(storage))
            },
            Err(e) => {
                let message = e.to_string();
                error!("Error initializing RocksDB storage at \"{}\".\nRocksDB said:\n{}",
                       path, message);
                Err(Error::new(StorageErrors::ConnectionError, message))
            }
        }
    }

    fn put(&mut self, key: &[u8], value: Vec<u8>) -> Result<()> {
        match self.db.put(key, value.as_slice()) {
            Ok(_) => Ok(()),
            Err(e) => {
                let message = e.to_string();
                let key_str = str::from_utf8(key).unwrap();
                error!("Error putting value for key \"{}\".\nRocksDB said:\n{}", key_str, message);
                Err(Error::new(StorageErrors::PutError, message))
            }
        }
    }

    fn get(&self, key: &[u8]) -> Result<Option<Vec<u8>>> {
        match self.db.get(key) {
            Ok(option) => {
                Ok(option.map(|value| value.to_vec()))
            },
            Err(e) => {
                let message = e.to_string();
                let key_str = str::from_utf8(key).unwrap();
                error!("Error getting value for key \"{}\".\nRocksDB said:\n{}", key_str, message);
                Err(Error::new(StorageErrors::GetError, message))
            }
        }
    }

    fn delete(&mut self, key: &[u8]) -> Result<()> {
        match self.db.delete(key) {
            Ok(_) => Ok(()),
            Err(e) => {
                let message = e.to_string();
                let key_str = str::from_utf8(key).unwrap();
                error!("Error deleting key \"{}\".\nRocksDB said:\n{}", key_str, message);
                Err(Error::new(StorageErrors::DeleteError, message))
            }
        }
    }

}