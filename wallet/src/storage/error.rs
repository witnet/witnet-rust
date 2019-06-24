//! # Error type for the Storage actor handlers.
use failure::Fail;

use witnet_crypto::cipher;

/// Error type for errors that may originate in the Storage actor.
#[derive(Debug, Fail)]
pub enum Error {
    #[fail(display = "failed to deserialize value from bincode")]
    DeserializeFailed(#[cause] bincode::Error),
    #[fail(display = "failed to serialize value from bincode")]
    SerializeFailed(#[cause] bincode::Error),
    #[fail(display = "database operation failed: {}", _0)]
    DbOpFailed(#[cause] rocksdb::Error),
    #[fail(display = "db key not found")]
    DbKeyNotFound,
    #[fail(display = "cipher operation failed: {}", _0)]
    CipherOpFailed(#[cause] cipher::Error),
    #[fail(display = "could not find a wallet with the given id: {}", _0)]
    UnknownWalletId(String),
    #[fail(display = "wrong password")]
    WrongPassword,
}
