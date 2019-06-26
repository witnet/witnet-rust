//! # Storage-related functions and types.
use witnet_crypto::{cipher, pbkdf2::pbkdf2_sha256};

use crate::wallet;

pub mod error;
pub mod keys;

pub use error::Error;

/// Get a value from the database or its default.
pub fn get_default<T, K>(db: &rocksdb::DB, key: K) -> Result<T, error::Error>
where
    T: serde::de::DeserializeOwned + Default,
    K: AsRef<[u8]>,
{
    get_or(db, key, Some(Default::default()))
}

/// Get a value from the database. If the key does not exists an error is returned.
pub fn get<T, K>(db: &rocksdb::DB, key: K) -> Result<T, error::Error>
where
    T: serde::de::DeserializeOwned,
    K: AsRef<[u8]>,
{
    get_or(db, key, None)
}

/// Put a value in the database under the given key.
pub fn put<T, K>(batch: &mut rocksdb::WriteBatch, key: K, value: &T) -> Result<(), error::Error>
where
    T: serde::Serialize,
    K: AsRef<[u8]>,
{
    let bytes = serialize(value)?;
    batch.put(key, bytes).map_err(error::Error::Db)?;

    Ok(())
}

/// Add a value to the database performing a Rockdb merge operation.
pub fn merge<T, K>(batch: &mut rocksdb::WriteBatch, key: K, value: &T) -> Result<(), error::Error>
where
    T: serde::Serialize,
    K: AsRef<[u8]>,
{
    let bytes = serialize(value)?;
    batch.merge(key, bytes).map_err(error::Error::Db)?;

    Ok(())
}

/// Write all the opertations in the given batch to the database.
pub fn write(db: &rocksdb::DB, batch: rocksdb::WriteBatch) -> Result<(), error::Error> {
    db.write(batch).map_err(error::Error::Db)
}

/// Flush database.
pub fn flush(db: &rocksdb::DB) -> Result<(), error::Error> {
    let mut opts = rocksdb::FlushOptions::default();
    opts.set_wait(true);
    db.flush_opt(&opts).map_err(error::Error::Db)
}

pub type Key = wallet::Key;

/// Generate an encryption key.
pub fn gen_key(
    encrypt_salt_length: usize,
    encrypt_hash_iterations: u32,
    password: &[u8],
) -> Result<Key, error::Error> {
    let salt = cipher::generate_random(encrypt_salt_length).map_err(error::Error::Cipher)?;

    gen_key_salt(encrypt_hash_iterations, password, salt)
}

/// Generate an encryption key without a random salt.
pub fn gen_key_salt(
    encrypt_hash_iterations: u32,
    password: &[u8],
    salt: Vec<u8>,
) -> Result<Key, error::Error> {
    let secret = pbkdf2_sha256(password, salt.as_ref(), encrypt_hash_iterations);

    Ok(Key {
        secret,
        salt: salt.to_vec(),
    })
}

/// Encrypt the given value with the given key.
pub fn encrypt<T>(encrypt_iv_length: usize, key: &Key, value: &T) -> Result<Vec<u8>, error::Error>
where
    T: serde::Serialize,
{
    let bytes = serialize(value)?;
    let iv = cipher::generate_random(encrypt_iv_length).map_err(error::Error::Cipher)?;
    let encrypted = cipher::encrypt_aes_cbc(key.secret.as_ref(), bytes.as_ref(), iv.as_ref())
        .map_err(error::Error::Cipher)?;
    let mut final_value = iv;
    final_value.extend(encrypted);
    final_value.extend_from_slice(key.salt.as_ref());

    Ok(final_value)
}

/// Decrypt the given value with the given password.
pub fn decrypt_password<T>(
    encrypt_salt_length: usize,
    encrypt_iv_length: usize,
    encrypt_hash_iterations: u32,
    password: &[u8],
    encrypted: &[u8],
) -> Result<(T, Key), error::Error>
where
    T: serde::de::DeserializeOwned,
{
    let len = encrypted.len();
    let iv = &encrypted[0..encrypt_iv_length];
    let data = &encrypted[encrypt_iv_length..len - encrypt_salt_length];
    let salt = &encrypted[len - encrypt_salt_length..];
    let key = gen_key_salt(encrypt_hash_iterations, password, salt.to_vec())?;
    let bytes =
        cipher::decrypt_aes_cbc(&key.secret.as_ref(), data, iv).map_err(error::Error::Cipher)?;
    let value = deserialize(bytes.as_ref()).map_err(|_| error::Error::WrongPassword)?;

    Ok((value, key))
}

/// Serialize value to binary.
pub fn serialize<T>(value: &T) -> Result<Vec<u8>, error::Error>
where
    T: serde::Serialize,
{
    bincode::serialize(value).map_err(error::Error::SerializeFailed)
}

/// Deserialize bytes to value of type T.
pub fn deserialize<'a, T>(bytes: &'a [u8]) -> Result<T, error::Error>
where
    T: serde::Deserialize<'a>,
{
    bincode::deserialize(bytes).map_err(error::Error::DeserializeFailed)
}

fn try_merge<T>(values: &mut Vec<T>, slice: &[u8]) -> Result<(), error::Error>
where
    T: serde::de::DeserializeOwned + PartialEq<T>,
{
    log::trace!("merging value");
    let val = deserialize(slice)?;

    if !values.contains(&val) {
        values.push(val);
    }

    Ok(())
}

fn try_merge_vec<T>(values: &mut Vec<T>, slice: &[u8]) -> Result<(), error::Error>
where
    T: serde::de::DeserializeOwned + PartialEq<T>,
{
    log::trace!("merging vec of values");
    let old_values: Vec<T> = deserialize(slice)?;

    for val in old_values {
        if !values.contains(&val) {
            values.push(val);
        }
    }

    Ok(())
}

/// Rocksdb merge operator for wallet database.
pub fn storage_merge_operator(
    new_key: &[u8],
    existing_val: Option<&[u8]>,
    operands: &mut rocksdb::MergeOperands,
) -> Option<Vec<u8>> {
    match new_key {
        b"wallets" => {
            log::trace!("merge starting...");
            let mut infos: Vec<wallet::WalletId> = Vec::with_capacity(operands.size_hint().0);

            if let Some(bytes) = existing_val {
                infos = deserialize(bytes).expect("merge: deserialize ids failed");
            }

            for bytes in operands {
                try_merge_vec(&mut infos, bytes)
                    .or_else(|_| try_merge(&mut infos, bytes))
                    .expect("merge: deserialize operand failed");
            }
            log::trace!("merge finished");
            Some(
                serialize::<Vec<wallet::WalletId>>(infos.as_ref())
                    .expect("merge: serialize ids failed"),
            )
        }
        field => panic!("field {:?} do not support merge", field),
    }
}

pub fn get_or<T, K>(db: &rocksdb::DB, key: K, default: Option<T>) -> Result<T, error::Error>
where
    T: serde::de::DeserializeOwned,
    K: AsRef<[u8]>,
{
    let key = key.as_ref();
    let bytes_opt = db.get(key).map_err(error::Error::Db)?;
    let value = bytes_opt.map_or_else(
        || default.ok_or_else(|| error::Error::DbKeyNotFound(key.to_vec())),
        |bytes| deserialize(bytes.as_ref()),
    )?;

    Ok(value)
}

pub fn get_opt<T, K>(db: &rocksdb::DB, key: K) -> Result<Option<T>, error::Error>
where
    T: serde::de::DeserializeOwned,
    K: AsRef<[u8]>,
{
    let bytes_opt = db.get(key).map_err(Error::Db)?;
    let value =
        bytes_opt.map_or_else(|| Ok(None), |bytes| deserialize(bytes.as_ref()).map(Some))?;

    Ok(value)
}
