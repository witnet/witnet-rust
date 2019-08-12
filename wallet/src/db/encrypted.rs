use witnet_crypto::cipher;

use super::*;
use crate::types;

#[derive(Clone)]
pub struct EncryptedDb {
    engine: CryptoEngine,
    db: Db,
}

impl EncryptedDb {
    pub fn new(db: Db, key: types::Secret, iv: Vec<u8>, params: EncryptedDbParams) -> Self {
        let engine = CryptoEngine { key, iv, params };

        Self { db, engine }
    }

    pub fn get<K, V>(&self, key: K) -> Result<V>
    where
        K: AsRef<[u8]>,
        V: serde::de::DeserializeOwned,
    {
        let enc_key = self.engine.encrypt(key.as_ref())?;
        let bytes = self.db.get::<_, Vec<u8>>(enc_key)?;

        self.engine.decrypt(&bytes)
    }

    pub fn get_or_default<K, V>(&self, key: K) -> Result<V>
    where
        K: AsRef<[u8]>,
        V: serde::de::DeserializeOwned + Default,
    {
        let enc_key = self.engine.encrypt(key.as_ref())?;
        match self.db.get_opt::<_, Vec<u8>>(enc_key)? {
            Some(bytes) => self.engine.decrypt(bytes.as_ref()),
            None => Ok(Default::default()),
        }
    }

    pub fn get_opt<K, V>(&self, key: K) -> Result<Option<V>>
    where
        K: AsRef<[u8]>,
        V: serde::de::DeserializeOwned,
    {
        match self.db.get_opt::<_, Vec<u8>>(key.as_ref())? {
            Some(bytes) => self.engine.decrypt(bytes.as_ref()),
            None => Ok(None),
        }
    }

    pub fn batch<'a>(&'a self) -> EncryptedWriteBatch<'a> {
        EncryptedWriteBatch {
            batch: Default::default(),
            engine: &self.engine,
        }
    }

    pub fn write(&self, enc_batch: EncryptedWriteBatch<'_>) -> Result<()> {
        self.db.write(enc_batch.batch)?;

        Ok(())
    }
}

pub struct EncryptedWriteBatch<'a> {
    batch: WriteBatch,
    engine: &'a CryptoEngine,
}

impl<'a> EncryptedWriteBatch<'a> {
    pub fn put<K, V>(&mut self, key: K, value: V) -> Result<()>
    where
        K: AsRef<[u8]>,
        V: serde::Serialize,
    {
        let enc_key = self.engine.encrypt(key.as_ref())?;
        let enc_val = self.engine.encrypt(&value)?;

        self.batch.put(enc_key, enc_val)?;

        Ok(())
    }
}

#[derive(Clone)]
struct CryptoEngine {
    key: types::Secret,
    iv: Vec<u8>,
    params: EncryptedDbParams,
}

impl CryptoEngine {
    pub fn new(key: types::Secret, iv: Vec<u8>, params: EncryptedDbParams) -> Self {
        Self { key, iv, params }
    }

    fn encrypt<T>(&self, value: &T) -> Result<Vec<u8>>
    where
        T: serde::Serialize + ?Sized,
    {
        let bytes = bincode::serialize(value)?;
        let encrypted = cipher::encrypt_aes_cbc(self.key.as_ref(), &bytes, &self.iv)?;

        Ok(encrypted)
    }

    fn decrypt<T>(&self, bytes: &[u8]) -> Result<T>
    where
        T: serde::de::DeserializeOwned,
    {
        let decrypted = cipher::decrypt_aes_cbc(self.key.as_ref(), bytes, &self.iv)?;
        let value = bincode::deserialize(&decrypted)?;

        Ok(value)
    }
}

#[derive(Clone)]
pub struct EncryptedDbParams {
    pub id_hash_iterations: u32,
    pub id_hash_function: types::HashFunction,
    pub db_hash_iterations: u32,
    pub db_iv_length: usize,
    pub db_salt_length: usize,
}
