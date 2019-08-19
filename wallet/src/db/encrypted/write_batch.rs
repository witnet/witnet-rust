use super::*;

pub struct EncryptedWriteBatch {
    prefixer: prefix::Prefixer,
    batch: rocksdb::WriteBatch,
    engine: engine::CryptoEngine,
}

impl EncryptedWriteBatch {
    pub fn new(prefixer: prefix::Prefixer, engine: engine::CryptoEngine) -> Self {
        Self {
            prefixer,
            engine,
            batch: Default::default(),
        }
    }
}

impl WriteBatch for EncryptedWriteBatch {
    fn put<K, V>(&mut self, key: K, value: V) -> Result<()>
    where
        K: AsRef<[u8]>,
        V: serde::Serialize,
    {
        let prefix_key = self.prefixer.prefix(key.as_ref());
        let enc_key = self.engine.encrypt(&prefix_key)?;
        let enc_val = self.engine.encrypt(&value)?;

        self.batch.put(enc_key, enc_val)?;

        Ok(())
    }
}

impl Into<rocksdb::WriteBatch> for EncryptedWriteBatch {
    fn into(self) -> rocksdb::WriteBatch {
        self.batch
    }
}
