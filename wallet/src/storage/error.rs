//! # Error type for the Storage actor.
use failure::Fail;

use witnet_crypto::cipher;

/// Error type for errors that may originate in the Storage actor.
#[derive(Debug, Fail)]
pub enum Error {
    #[fail(display = "failed to deserialize value from bincode: {}", _0)]
    DeserializeFailed(#[cause] bincode::Error),
    #[fail(display = "failed to serialize value to bincode: {}", _0)]
    SerializeFailed(#[cause] bincode::Error),
    #[fail(display = "db error: {}", _0)]
    Db(#[cause] rocksdb::Error),
    #[fail(display = "db key {:?} not found", _0)]
    DbKeyNotFound(Vec<u8>),
    #[fail(display = "cipher operation failed: {}", _0)]
    Cipher(#[cause] cipher::Error),
    #[fail(display = "No wallet found with the given ID")]
    WalletNotFound,
    #[fail(display = "Wrong Password")]
    WrongPassword(#[cause] cipher::Error),
}
