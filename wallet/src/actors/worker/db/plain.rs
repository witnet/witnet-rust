use crate::actors::worker::*;

pub struct Db<'a> {
    db: &'a rocksdb::DB,
}

impl<'a> Db<'a> {
    pub fn new(db: &'a rocksdb::DB) -> Self {
        Self { db }
    }

    pub fn put<T>(&self, key: &[u8], value: &T) -> Result<()>
    where
        T: serde::Serialize,
    {
        let bytes = bincode::serialize(value)?;

        self.db.put(key, bytes)?;

        Ok(())
    }

    pub fn flush(&self) -> Result<()> {
        self.db.flush()?;
        Ok(())
    }

    pub fn write(&self, WriteBatch { batch }: WriteBatch) -> Result<()> {
        self.db.write(batch)?;
        Ok(())
    }

    pub fn with_key<'b, 'c>(
        self,
        key: &'b [u8],
        params: &'c Params,
    ) -> super::EncryptedDb<'b, 'c, 'a> {
        super::EncryptedDb::new(self, key, params)
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
}

#[derive(Default)]
pub struct WriteBatch {
    batch: rocksdb::WriteBatch,
}

impl WriteBatch {
    pub fn put<T>(&mut self, key: &[u8], value: &T) -> Result<()>
    where
        T: serde::Serialize,
    {
        let bytes = bincode::serialize(value)?;

        self.batch.put(key, bytes)?;

        Ok(())
    }
}
