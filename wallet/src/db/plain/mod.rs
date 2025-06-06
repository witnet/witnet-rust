use super::*;
use serde::de::DeserializeOwned;
use std::sync::Arc;

mod write_batch;

pub use write_batch::*;

#[derive(Clone)]
pub struct PlainDb {
    db: Arc<rocksdb::DB>,
}

impl PlainDb {
    pub fn new(db: Arc<rocksdb::DB>) -> Self {
        Self { db }
    }
}

impl AsRef<rocksdb::DB> for PlainDb {
    fn as_ref(&self) -> &rocksdb::DB {
        self.db.as_ref()
    }
}

impl Database for PlainDb {
    type WriteBatch = PlainWriteBatch;

    fn get_opt<K, V>(&self, key: &Key<K, V>) -> Result<Option<V>>
    where
        K: AsRef<[u8]>,
        V: DeserializeOwned,
    {
        self.get_opt_with(key, |bytes| Vec::from(bytes))
    }

    fn get_opt_with<K, V, F>(&self, key: &Key<K, V>, with: F) -> Result<Option<V>>
    where
        K: AsRef<[u8]>,
        V: DeserializeOwned,
        F: Fn(&[u8]) -> Vec<u8>,
    {
        let res = self.as_ref().get(key)?;
        match res {
            Some(dbvec) => {
                let mapped = with(&dbvec);
                let value = bincode::deserialize(&mapped)?;
                Ok(Some(value))
            }
            None => Ok(None),
        }
    }

    fn contains<K, V>(&self, key: &Key<K, V>) -> Result<bool>
    where
        K: AsRef<[u8]>,
    {
        let res = self.as_ref().get(key)?;
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
        let bytes = bincode::serialize(value.borrow())?;

        self.as_ref().put(key, bytes)?;

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
        PlainWriteBatch::default()
    }
}
