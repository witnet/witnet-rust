mod encrypted;
mod error;
mod plain;
#[cfg(test)]
mod tests;

use std::fmt::Debug;

use crate::repository::keys::Key;

pub use encrypted::*;
pub use error::Error;
pub use plain::*;
pub use std::borrow::Borrow;
#[cfg(test)]
pub use tests::*;

pub type Result<T> = std::result::Result<T, Error>;

pub trait Database {
    type WriteBatch: WriteBatch;

    fn get<K, V>(&self, key: &Key<K, V>) -> Result<V>
    where
        K: AsRef<[u8]> + Debug,
        V: serde::de::DeserializeOwned,
    {
        let opt = self.get_opt(key)?;

        opt.ok_or_else(|| Error::DbKeyNotFound {
            key: format!("{key:?}"),
        })
    }

    fn get_or_default<K, V>(&self, key: &Key<K, V>) -> Result<V>
    where
        K: AsRef<[u8]>,
        V: serde::de::DeserializeOwned + Default,
    {
        let opt = self.get_opt(key)?;

        Ok(opt.unwrap_or_default())
    }

    fn get_opt<K, V>(&self, key: &Key<K, V>) -> Result<Option<V>>
    where
        K: AsRef<[u8]>,
        V: serde::de::DeserializeOwned;

    fn get_opt_with<K, V, F>(&self, key: &Key<K, V>, with: F) -> Result<Option<V>>
    where
        K: AsRef<[u8]>,
        V: serde::de::DeserializeOwned,
        F: Fn(&[u8]) -> Vec<u8>;

    #[allow(dead_code)]
    fn contains<K, V>(&self, key: &Key<K, V>) -> Result<bool>
    where
        K: AsRef<[u8]>;

    fn put<K, V, Vref>(&self, key: &Key<K, V>, value: Vref) -> Result<()>
    where
        K: AsRef<[u8]>,
        V: serde::Serialize + ?Sized,
        Vref: Borrow<V>;

    fn write(&self, batch: Self::WriteBatch) -> Result<()>;

    fn flush(&self) -> Result<()>;

    fn batch(&self) -> Self::WriteBatch;
}

pub trait WriteBatch {
    fn put<K, V, Vref>(&mut self, key: &Key<K, V>, value: Vref) -> Result<()>
    where
        K: AsRef<[u8]>,
        V: serde::Serialize + ?Sized,
        Vref: Borrow<V>;
}

pub trait GetWith {
    fn get_with<K, V, F>(&self, key: &Key<K, V>, with: F) -> Result<V>
    where
        K: AsRef<[u8]> + Debug,
        V: serde::de::DeserializeOwned,
        F: Fn(&[u8]) -> Vec<u8>,
    {
        let opt = self.get_with_opt(key, with)?;

        opt.ok_or_else(|| Error::DbKeyNotFound {
            key: format!("{key:?}"),
        })
    }
    fn get_with_opt<K, V, F>(&self, key: &Key<K, V>, with: F) -> Result<Option<V>>
    where
        K: AsRef<[u8]>,
        V: serde::de::DeserializeOwned,
        F: Fn(&[u8]) -> Vec<u8>;
}
