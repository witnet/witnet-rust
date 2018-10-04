//! Storage backend that persists data in the file system using a RocksDB database.

use crate::error::{StorageError, StorageResult};
use crate::storage::Storage;
use rocksdb::DB;

use std::fmt::Debug;
use std::str;

use witnet_util::error::WitnetError;

/// Data structure for the RocksDB storage whose only member is a rocksdb::DB object.
pub struct RocksStorage {
    db: DB,
}

/// Implement the Storage generic trait for the RocksStorage storage data structure.
impl<'a> Storage<&'a [u8], Vec<u8>> for RocksStorage {
    fn new(path: impl Debug) -> StorageResult<Box<Self>> {
        let path = format!("{:?}", path);
        match DB::open_default(&path) {
            Ok(db) => {
                let storage = RocksStorage { db };
                Ok(Box::new(storage))
            }
            Err(e) => Err(WitnetError::storage_err(StorageError::Connection {
                path,
                msg: e.to_string(),
            })),
        }
    }

    fn put(&mut self, key: &[u8], value: Vec<u8>) -> StorageResult<()> {
        match self.db.put(key, value.as_slice()) {
            Ok(_) => Ok(()),
            Err(e) => {
                let key = str::from_utf8(key).unwrap();
                Err(WitnetError::storage_err(StorageError::Put {
                    key: key.to_string(),
                    msg: e.to_string(),
                }))
            }
        }
    }

    fn get(&self, key: &[u8]) -> StorageResult<Option<Vec<u8>>> {
        match self.db.get(key) {
            Ok(option) => Ok(option.map(|value| value.to_vec())),
            Err(e) => {
                let key = str::from_utf8(key).unwrap();
                Err(WitnetError::storage_err(StorageError::Get {
                    key: key.to_string(),
                    msg: e.to_string(),
                }))
            }
        }
    }

    fn delete(&mut self, key: &[u8]) -> StorageResult<()> {
        match self.db.delete(key) {
            Ok(_) => Ok(()),
            Err(e) => {
                let key = str::from_utf8(key).unwrap();
                Err(WitnetError::storage_err(StorageError::Delete {
                    key: key.to_string(),
                    msg: e.to_string(),
                }))
            }
        }
    }
}
