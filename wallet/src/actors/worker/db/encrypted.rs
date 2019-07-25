use witnet_crypto::cipher;

use crate::actors::worker::*;

pub struct EncryptedDb<'a, 'b, 'c> {
    engine: CryptoEngine<'a, 'b>,
    db: Db<'c>,
}

impl<'a, 'b, 'c> EncryptedDb<'a, 'b, 'c> {
    pub fn new(db: Db<'c>, key: &'a [u8], params: &'b Params) -> Self {
        let engine = CryptoEngine::new(key, params);

        Self { db, engine }
    }

    pub fn write(&self, WriteBatch { batch, .. }: WriteBatch<'a, 'b>) -> Result<()> {
        self.db.write(batch)?;
        Ok(())
    }

    pub fn batch(&self) -> WriteBatch<'a, 'b> {
        WriteBatch {
            batch: Default::default(),
            engine: self.engine.clone(),
        }
    }

    pub fn get<T>(&self, key: &[u8]) -> Result<T>
    where
        T: serde::de::DeserializeOwned,
    {
        self.db.get(key)
    }

    pub fn get_dec<T>(&self, key: &[u8]) -> Result<T>
    where
        T: serde::de::DeserializeOwned,
    {
        let bytes = self.db.get::<Vec<u8>>(key)?;

        self.engine.decrypt(&bytes)
    }

    pub fn get_or_default_dec<T>(&self, key: &[u8]) -> Result<T>
    where
        T: serde::de::DeserializeOwned + Default,
    {
        match self.db.get_opt::<Vec<u8>>(key)? {
            Some(bytes) => self.engine.decrypt(&bytes),
            None => Ok(Default::default()),
        }
    }
}

pub struct WriteBatch<'a, 'b> {
    batch: super::plain::WriteBatch,
    engine: CryptoEngine<'a, 'b>,
}

impl<'a, 'b> WriteBatch<'a, 'b> {
    pub fn put<T>(&mut self, key: &[u8], value: &T) -> Result<()>
    where
        T: serde::Serialize,
    {
        self.batch.put(key, value)
    }

    pub fn merge<T>(&mut self, key: &[u8], value: &T) -> Result<()>
    where
        T: serde::Serialize,
    {
        self.batch.merge(key, value)
    }

    pub fn put_enc<T>(&mut self, key: &[u8], value: &T) -> Result<()>
    where
        T: serde::Serialize,
    {
        let bytes = self.engine.encrypt(value)?;

        self.batch.put(key, &bytes)
    }
}

#[derive(Clone)]
struct CryptoEngine<'a, 'b> {
    key: &'a [u8],
    params: &'b Params,
}

impl<'a, 'b> CryptoEngine<'a, 'b> {
    pub fn new(key: &'a [u8], params: &'b Params) -> Self {
        Self { key, params }
    }

    fn iv(&self) -> Result<Vec<u8>> {
        let iv = cipher::generate_random(self.params.db_iv_length)?;

        Ok(iv)
    }

    fn encrypt<T>(&self, value: &T) -> Result<Vec<u8>>
    where
        T: serde::Serialize,
    {
        let bytes = bincode::serialize(value)?;
        let iv = self.iv()?;
        let encrypted = cipher::encrypt_aes_cbc(self.key, &bytes, &iv)?;
        let data = [iv, encrypted].concat();

        Ok(data)
    }

    fn decrypt<T>(&self, value: &[u8]) -> Result<T>
    where
        T: serde::de::DeserializeOwned,
    {
        let len = value.len();

        if len < self.params.db_iv_length {
            Err(Error::InvalidDataLen)?
        }

        let (iv, data) = value.split_at(self.params.db_iv_length);
        let bytes = cipher::decrypt_aes_cbc(self.key, data, iv)?;
        let value = bincode::deserialize(&bytes)?;

        Ok(value)
    }
}
