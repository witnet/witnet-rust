use std::sync::Arc;

use super::*;
use crate::types;

#[derive(Clone)]
pub struct Db {
    db: Arc<rocksdb::DB>,
}

impl Db {
    pub fn new(db: Arc<rocksdb::DB>) -> Self {
        Self { db }
    }

    pub fn flush(&self) -> Result<()> {
        self.db.flush()?;
        Ok(())
    }

    pub fn with_key(
        &self,
        key: types::Secret,
        iv: Vec<u8>,
        params: EncryptedDbParams,
    ) -> EncryptedDb {
        EncryptedDb::new(self.clone(), key, iv, params)
    }

    pub fn get<K, V>(&self, key: K) -> Result<V>
    where
        K: AsRef<[u8]>,
        V: serde::de::DeserializeOwned,
    {
        let opt = self.get_opt(&key)?;

        opt.ok_or_else(|| Error::KeyNotFound(hex::encode(key)))
    }

    pub fn get_or_default<K, V>(&self, key: K) -> Result<V>
    where
        K: AsRef<[u8]>,
        V: serde::de::DeserializeOwned + Default,
    {
        let opt = self.get_opt(key)?;

        Ok(opt.unwrap_or_default())
    }

    pub fn get_opt<K, V>(&self, key: K) -> Result<Option<V>>
    where
        K: AsRef<[u8]>,
        V: serde::de::DeserializeOwned,
    {
        if let Some(dbvec) = self.db.get(key)? {
            let value = bincode::deserialize(dbvec.as_ref())?;
            Ok(Some(value))
        } else {
            Ok(None)
        }
    }

    pub fn put<K, V>(&self, key: K, value: V) -> Result<()>
    where
        K: AsRef<[u8]>,
        V: serde::Serialize,
    {
        let bytes = bincode::serialize(&value)?;

        self.db.put(key, bytes)?;

        Ok(())
    }

    pub fn write(&self, WriteBatch { batch }: WriteBatch) -> Result<()> {
        self.db.write(batch)?;
        Ok(())
    }
}

#[derive(Default)]
pub struct WriteBatch {
    batch: rocksdb::WriteBatch,
}

impl WriteBatch {
    pub fn put<K, V>(&mut self, key: K, value: V) -> Result<()>
    where
        K: AsRef<[u8]>,
        V: serde::Serialize,
    {
        let bytes = bincode::serialize(&value)?;

        self.batch.put(key, bytes)?;

        Ok(())
    }
}
