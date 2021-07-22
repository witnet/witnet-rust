use super::*;

#[derive(Default)]
pub struct PlainWriteBatch {
    batch: rocksdb::WriteBatch,
}

impl WriteBatch for PlainWriteBatch {
    fn put<K, V, Vref>(&mut self, key: &Key<K, V>, value: Vref) -> Result<()>
    where
        K: AsRef<[u8]>,
        V: serde::Serialize + ?Sized,
        Vref: Borrow<V>,
    {
        let bytes = bincode::serialize(value.borrow())?;

        self.batch.put(key, bytes)?;

        Ok(())
    }

    fn delete<K, V>(&mut self, key: &Key<K, V>) -> Result<()>
    where
        K: AsRef<[u8]>,
        V: serde::Serialize + ?Sized,
    {
        self.batch.delete(key)?;

        Ok(())
    }
}

impl From<PlainWriteBatch> for rocksdb::WriteBatch {
    fn from(x: PlainWriteBatch) -> Self {
        x.batch
    }
}
