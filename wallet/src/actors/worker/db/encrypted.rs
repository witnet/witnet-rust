use witnet_crypto::cipher;

use crate::actors::worker::*;

pub struct EncryptedDb<'a, 'b, 'c, 'd> {
    engine: CryptoEngine<'a, 'b, 'c>,
    db: Db<'d>,
}

impl<'a, 'b, 'c, 'd> EncryptedDb<'a, 'b, 'c, 'd> {
    pub fn new(db: Db<'d>, key: &'a [u8], iv: &'b [u8], params: &'c Params) -> Self {
        let engine = CryptoEngine::new(key, iv, params);

        Self { db, engine }
    }

    pub fn put<T>(&self, key: &[u8], value: &T) -> Result<()>
    where
        T: serde::Serialize,
    {
        self.db.put(&key, &value)
    }

    pub fn put_enc<T>(&self, key: &[u8], value: &T) -> Result<()>
    where
        T: serde::Serialize,
    {
        let enc_key = self.engine.encrypt(&key)?;
        let enc_val = self.engine.encrypt(value)?;

        self.db.put(&enc_key, &enc_val)
    }

    pub fn get_or_default<T>(&self, key: &[u8]) -> Result<T>
    where
        T: serde::de::DeserializeOwned + Default,
    {
        self.db.get_or_default(&key)
    }

    pub fn get_opt<T>(&self, key: &[u8]) -> Result<Option<T>>
    where
        T: serde::de::DeserializeOwned,
    {
        self.db.get_opt(&key)
    }

    pub fn write(&self, WriteBatch { batch, .. }: WriteBatch<'a, 'b, 'c>) -> Result<()> {
        self.db.write(batch)?;
        Ok(())
    }

    pub fn batch(&self) -> WriteBatch<'a, 'b, 'c> {
        WriteBatch {
            batch: Default::default(),
            engine: self.engine.clone(),
        }
    }

    pub fn get_dec<T>(&self, key: &[u8]) -> Result<T>
    where
        T: serde::de::DeserializeOwned,
    {
        let enc_key = self.engine.encrypt(&key)?;
        let bytes = self.db.get::<Vec<u8>>(&enc_key)?;

        self.engine.decrypt(&bytes)
    }

    pub fn get_or_default_dec<T>(&self, key: &[u8]) -> Result<T>
    where
        T: serde::de::DeserializeOwned + Default,
    {
        let enc_key = self.engine.encrypt(&key)?;
        match self.db.get_opt::<Vec<u8>>(&enc_key)? {
            Some(bytes) => self.engine.decrypt(&bytes),
            None => Ok(Default::default()),
        }
    }
}

pub struct WriteBatch<'a, 'b, 'c> {
    batch: super::plain::WriteBatch,
    engine: CryptoEngine<'a, 'b, 'c>,
}

impl<'a, 'b, 'c> WriteBatch<'a, 'b, 'c> {
    pub fn put<T>(&mut self, key: &[u8], value: &T) -> Result<()>
    where
        T: serde::Serialize,
    {
        self.batch.put(key, value)
    }

    pub fn put_enc<T>(&mut self, key: &[u8], value: &T) -> Result<()>
    where
        T: serde::Serialize,
    {
        let enc_key = self.engine.encrypt(&key)?;
        let enc_val = self.engine.encrypt(value)?;

        self.batch.put(&enc_key, &enc_val)
    }
}

#[derive(Clone)]
struct CryptoEngine<'a, 'b, 'c> {
    key: &'a [u8],
    iv: &'b [u8],
    params: &'c Params,
}

impl<'a, 'b, 'c> CryptoEngine<'a, 'b, 'c> {
    pub fn new(key: &'a [u8], iv: &'b [u8], params: &'c Params) -> Self {
        Self { key, iv, params }
    }

    fn encrypt<T>(&self, value: &T) -> Result<Vec<u8>>
    where
        T: serde::Serialize,
    {
        let bytes = bincode::serialize(value)?;
        let encrypted = cipher::encrypt_aes_cbc(self.key, &bytes, self.iv)?;

        Ok(encrypted)
    }

    fn decrypt<T>(&self, bytes: &[u8]) -> Result<T>
    where
        T: serde::de::DeserializeOwned,
    {
        let decrypted = cipher::decrypt_aes_cbc(self.key, bytes, self.iv)?;
        let value = bincode::deserialize(&decrypted)?;

        Ok(value)
    }
}
