mod encrypted;
mod error;
mod plain;
#[cfg(test)]
mod tests;

pub use encrypted::*;
pub use error::Error;
pub use plain::*;
#[cfg(test)]
pub use tests::*;

pub type Result<T> = std::result::Result<T, Error>;

pub trait Database {
    type WriteBatch: WriteBatch;

    fn get<K, V>(&self, key: &K) -> Result<V>
    where
        K: AsRef<[u8]> + ?Sized,
        V: serde::de::DeserializeOwned,
    {
        let opt = self.get_opt(key)?;

        opt.ok_or_else(|| Error::DbKeyNotFound)
    }

    fn get_or_default<K, V>(&self, key: &K) -> Result<V>
    where
        K: AsRef<[u8]> + ?Sized,
        V: serde::de::DeserializeOwned + Default,
    {
        let opt = self.get_opt(key)?;

        Ok(opt.unwrap_or_default())
    }

    fn get_opt<K, V>(&self, key: &K) -> Result<Option<V>>
    where
        K: AsRef<[u8]> + ?Sized,
        V: serde::de::DeserializeOwned;

    fn put<K, V>(&self, key: K, value: V) -> Result<()>
    where
        K: AsRef<[u8]>,
        V: serde::Serialize;

    fn write(&self, batch: Self::WriteBatch) -> Result<()>;

    fn flush(&self) -> Result<()>;

    fn batch(&self) -> Self::WriteBatch;
}

pub trait WriteBatch {
    fn put<K, V>(&mut self, key: K, value: V) -> Result<()>
    where
        K: AsRef<[u8]>,
        V: serde::Serialize;
}
