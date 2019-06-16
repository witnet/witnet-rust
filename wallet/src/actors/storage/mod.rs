//! # Storage actor
//!
//! It is charge of managing the connection to the key-value database. This actor is blocking so it
//! must be used with a `SyncArbiter`.
use std::sync::Arc;

use actix::prelude::*;

use witnet_crypto::{cipher, pbkdf2::pbkdf2_sha256};
use witnet_protected::{Protected, ProtectedString};

use crate::wallet;

pub mod builder;
pub mod error;
mod handlers;
mod keys;

pub use handlers::*;

/// Expose options for tunning the database.
pub type Options = rocksdb::Options;

/// Storage actor.
pub struct Storage {
    /// Holds the wallets ids in plain text, and the wallets information encrypted with a password.
    db: Arc<rocksdb::DB>,
    params: Arc<builder::Params>,
}

impl Storage {
    pub fn build<'a>() -> builder::Builder<'a> {
        builder::Builder::new()
    }

    pub fn new(params: Arc<builder::Params>, db: Arc<rocksdb::DB>) -> Self {
        Self { db, params }
    }

    pub fn get_wallet_infos(&self) -> Result<Vec<wallet::WalletInfo>, error::Error> {
        let ids = self.get_wallet_ids()?;
        let len = ids.len();
        let infos = ids
            .into_iter()
            .try_fold(Vec::with_capacity(len), |mut acc, id| {
                let info = get(self.db.as_ref(), keys::wallet_info(id.as_ref()))?;
                acc.push(info);

                Ok(acc)
            })?;

        Ok(infos)
    }

    pub fn get_wallet_ids(&self) -> Result<Vec<wallet::WalletId>, error::Error> {
        get_default(self.db.as_ref(), keys::wallets())
    }

    pub fn create_wallet(
        &self,
        wallet: wallet::Wallet,
        password: ProtectedString,
    ) -> Result<(), error::Error> {
        let mut batch = rocksdb::WriteBatch::default();
        let id = &wallet.info.id;

        merge(&mut batch, keys::wallets(), id)?;
        put(&mut batch, keys::wallet_info(id), &wallet.info)?;
        put(
            &mut batch,
            keys::wallet(id),
            &encrypt(self.params.as_ref(), password.as_ref(), &wallet.content)?,
        )?;

        write(self.db.as_ref(), batch)?;

        Ok(())
    }
}

impl Actor for Storage {
    type Context = SyncContext<Self>;
}

impl Supervised for Storage {}

fn get_default<T, K>(db: &rocksdb::DB, key: K) -> Result<T, error::Error>
where
    T: serde::de::DeserializeOwned + Default,
    K: AsRef<[u8]>,
{
    get_or(db, key, Some(Default::default()))
}

fn get<T, K>(db: &rocksdb::DB, key: K) -> Result<T, error::Error>
where
    T: serde::de::DeserializeOwned,
    K: AsRef<[u8]>,
{
    get_or(db, key, None)
}

fn get_or<T, K>(db: &rocksdb::DB, key: K, default: Option<T>) -> Result<T, error::Error>
where
    T: serde::de::DeserializeOwned,
    K: AsRef<[u8]>,
{
    let bytes_opt = db.get(key).map_err(error::Error::DbOpFailed)?;
    let value = bytes_opt.map_or_else(
        || default.ok_or_else(|| error::Error::DbKeyNotFound),
        |bytes| deserialize(bytes.as_ref()),
    )?;

    Ok(value)
}

fn put<T, K>(batch: &mut rocksdb::WriteBatch, key: K, value: &T) -> Result<(), error::Error>
where
    T: serde::Serialize,
    K: AsRef<[u8]>,
{
    let bytes = serialize(value)?;
    batch.put(key, bytes).map_err(error::Error::DbOpFailed)?;

    Ok(())
}

fn merge<T, K>(batch: &mut rocksdb::WriteBatch, key: K, value: &T) -> Result<(), error::Error>
where
    T: serde::Serialize,
    K: AsRef<[u8]>,
{
    let bytes = serialize(value)?;
    batch.merge(key, bytes).map_err(error::Error::DbOpFailed)?;

    Ok(())
}

fn write(db: &rocksdb::DB, batch: rocksdb::WriteBatch) -> Result<(), error::Error> {
    db.write(batch).map_err(error::Error::DbOpFailed)
}

fn get_secret(password: &[u8], salt: &[u8], iter_count: u32) -> Protected {
    pbkdf2_sha256(password, salt, iter_count)
}

fn encrypt<T>(params: &builder::Params, password: &[u8], value: &T) -> Result<Vec<u8>, error::Error>
where
    T: serde::Serialize,
{
    let bytes = serialize(value)?;
    let iv =
        cipher::generate_random(params.encrypt_iv_length).map_err(error::Error::CipherOpFailed)?;
    let salt = cipher::generate_random(params.encrypt_salt_length)
        .map_err(error::Error::CipherOpFailed)?;
    let secret = get_secret(password, &salt, params.encrypt_hash_iterations);
    let encrypted = cipher::encrypt_aes_cbc(&secret, bytes.as_ref(), iv.as_ref())
        .map_err(error::Error::CipherOpFailed)?;
    let mut final_value = iv;
    final_value.extend(encrypted);
    final_value.extend(salt);

    Ok(final_value)
}

fn serialize<T>(value: &T) -> Result<Vec<u8>, error::Error>
where
    T: serde::Serialize,
{
    bincode::serialize(value).map_err(error::Error::SerializeFailed)
}

fn deserialize<'a, T>(bytes: &'a [u8]) -> Result<T, error::Error>
where
    T: serde::Deserialize<'a>,
{
    bincode::deserialize(bytes).map_err(error::Error::DeserializeFailed)
}

fn storage_merge(
    new_key: &[u8],
    existing_val: Option<&[u8]>,
    operands: &mut rocksdb::MergeOperands,
) -> Option<Vec<u8>> {
    match new_key {
        b"wallets" => {
            let mut ids: Vec<wallet::WalletId> =
                existing_val.map_or_else(Vec::new, |bytes| deserialize(bytes).expect("foo"));

            for bytes in operands {
                let id = deserialize(bytes).expect("bar");
                if !ids.contains(&id) {
                    ids.push(id)
                }
            }

            Some(serialize::<Vec<wallet::WalletId>>(ids.as_ref()).expect("baz"))
        }
        field => panic!("field {:?} do not support merge", field),
    }
}
