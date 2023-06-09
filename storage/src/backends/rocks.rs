//! # Rocksdb storage backend
//!
//! Storage backend that persists data in the file system using a RocksDB database.
use failure::Fail;

use crate::storage::{Result, Storage, StorageIterator, WriteBatch, WriteBatchItem};

/// Rocksdb backend
pub type Backend = rocksdb::DB;

/// Rocksdb Options
pub type Options = rocksdb::Options;

#[derive(Debug, Fail)]
#[fail(display = "RocksDB error: {}", _0)]
struct Error(#[fail(cause)] rocksdb::Error);

impl Storage for Backend {
    fn get(&self, key: &[u8]) -> Result<Option<Vec<u8>>> {
        let result = Backend::get(self, key)
            .map(|opt| opt.map(|dbvec| dbvec.to_vec()))
            .map_err(Error)?;
        Ok(result)
    }

    fn put(&self, key: Vec<u8>, value: Vec<u8>) -> Result<()> {
        Backend::put(self, key, value).map_err(Error)?;
        Ok(())
    }

    fn delete(&self, key: &[u8]) -> Result<()> {
        Backend::delete(self, key).map_err(Error)?;
        Ok(())
    }

    fn prefix_iterator<'a, 'b: 'a>(&'a self, prefix: &'b [u8]) -> Result<StorageIterator<'a>> {
        let iterator = Backend::prefix_iterator(self, prefix);

        Ok(Box::new(iterator.filter_map(|result| match result {
            Ok((k, v)) => Some((k.into(), v.into())),
            _ => None,
        })))
    }
    /// Atomically write a batch of operations
    fn write(&self, batch: WriteBatch) -> Result<()> {
        let mut rocksdb_batch = rocksdb::WriteBatch::default();

        for item in batch.batch {
            match item {
                WriteBatchItem::Put(key, value) => {
                    rocksdb_batch.put(key, value);
                }
                WriteBatchItem::Delete(key) => {
                    rocksdb_batch.delete(key);
                }
            }
        }

        self.write(rocksdb_batch)?;

        Ok(())
    }
}
