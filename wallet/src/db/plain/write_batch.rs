use super::*;

#[derive(Default)]
pub struct PlainWriteBatch {
    batch: rocksdb::WriteBatch,
}

impl WriteBatch for PlainWriteBatch {
    fn put<K, V>(&mut self, key: K, value: V) -> Result<()>
    where
        K: AsRef<[u8]>,
        V: serde::Serialize,
    {
        let bytes = bincode::serialize(&value)?;

        self.batch.put(key, bytes)?;

        Ok(())
    }
}

impl Into<rocksdb::WriteBatch> for PlainWriteBatch {
    fn into(self) -> rocksdb::WriteBatch {
        self.batch
    }
}
