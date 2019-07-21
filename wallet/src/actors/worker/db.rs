use std::mem;

use super::*;

pub struct Db {
    db: Arc<rocksdb::DB>,
    batch: rocksdb::WriteBatch,
}

impl Db {
    pub fn new(db: Arc<rocksdb::DB>) -> Self {
        Self {
            db,
            batch: rocksdb::WriteBatch::default(),
        }
    }

    pub fn flush(&self) -> Result<()> {
        self.db.flush()?;
        Ok(())
    }

    pub fn get<T>(&self, key: &[u8]) -> Result<T>
    where
        T: serde::de::DeserializeOwned,
    {
        let opt = self.get_opt(key)?;

        opt.ok_or_else(|| Error::DbKeyNotFound(hex::encode(key)))
    }

    pub fn get_or_default<T>(&self, key: &[u8]) -> Result<T>
    where
        T: serde::de::DeserializeOwned + Default,
    {
        let opt = self.get_opt(key)?;

        Ok(opt.unwrap_or_default())
    }

    pub fn get_opt<T>(&self, key: &[u8]) -> Result<Option<T>>
    where
        T: serde::de::DeserializeOwned,
    {
        if let Some(dbvec) = self.db.get(key)? {
            let value = bincode::deserialize(dbvec.as_ref())?;
            Ok(Some(value))
        } else {
            Ok(None)
        }
    }

    pub fn merge<T>(&mut self, key: &[u8], value: &T) -> Result<()>
    where
        T: serde::Serialize,
    {
        let bytes = bincode::serialize(value)?;

        self.batch.merge(key, bytes)?;

        Ok(())
    }

    pub fn put<T>(&mut self, key: &[u8], value: &T) -> Result<()>
    where
        T: serde::Serialize,
    {
        let bytes = bincode::serialize(value)?;

        self.batch.put(key, bytes)?;

        Ok(())
    }

    pub fn write(&mut self) -> Result<()> {
        self.db.write(mem::replace(
            &mut self.batch,
            rocksdb::WriteBatch::default(),
        ))?;
        Ok(())
    }
}
