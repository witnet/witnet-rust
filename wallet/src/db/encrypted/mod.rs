use std::sync::Arc;

use witnet_crypto::cipher;

use super::*;
use crate::{db::encrypted::write_batch::EncryptedWriteBatch, types};

mod engine;
mod prefix;
mod write_batch;

#[derive(Clone)]
pub struct EncryptedDb {
    engine: engine::CryptoEngine,
    db: Arc<rocksdb::DB>,
    prefixer: prefix::Prefixer,
}

impl EncryptedDb {
    pub fn new(db: Arc<rocksdb::DB>, prefix: Vec<u8>, key: types::Secret, iv: Vec<u8>) -> Self {
        let engine = engine::CryptoEngine::new(key, iv);

        Self {
            db,
            engine,
            prefixer: prefix::Prefixer::new(prefix),
        }
    }
}

impl AsRef<rocksdb::DB> for EncryptedDb {
    fn as_ref(&self) -> &rocksdb::DB {
        self.db.as_ref()
    }
}

impl Database for EncryptedDb {
    type WriteBatch = EncryptedWriteBatch;

    fn get_opt<K, V>(&self, key: &Key<K, V>) -> Result<Option<V>>
    where
        K: AsRef<[u8]>,
        V: serde::de::DeserializeOwned,
    {
        let prefix_key = self.prefixer.prefix(key);
        let enc_key = self.engine.encrypt(&prefix_key)?;
        let res = self.as_ref().get(&enc_key)?;

        match res {
            Some(dbvec) => {
                let value = self.engine.decrypt(dbvec.as_ref())?;

                Ok(Some(value))
            }
            None => Ok(None),
        }
    }

    fn contains<K, V>(&self, key: &Key<K, V>) -> Result<bool>
    where
        K: AsRef<[u8]>,
    {
        let prefix_key = self.prefixer.prefix(key);
        let enc_key = self.engine.encrypt(&prefix_key)?;
        let res = self.as_ref().get(&enc_key)?;

        match res {
            Some(_) => Ok(true),
            None => Ok(false),
        }
    }

    fn put<K, V, Vref>(&self, key: &Key<K, V>, value: Vref) -> Result<()>
    where
        K: AsRef<[u8]>,
        V: serde::Serialize + ?Sized,
        Vref: Borrow<V>,
    {
        let prefix_key = self.prefixer.prefix(key);
        let enc_key = self.engine.encrypt(&prefix_key)?;
        let enc_val = self.engine.encrypt(value.borrow())?;

        self.as_ref().put(enc_key, enc_val)?;

        Ok(())
    }

    fn write(&self, batch: Self::WriteBatch) -> Result<()> {
        self.as_ref().write(batch.into())?;

        Ok(())
    }

    fn flush(&self) -> Result<()> {
        self.as_ref().flush()?;

        Ok(())
    }

    fn batch(&self) -> Self::WriteBatch {
        EncryptedWriteBatch::new(self.prefixer.clone(), self.engine.clone())
    }
}
