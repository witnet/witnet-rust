use failure::Fail;

use crate::{db, types};

#[derive(Debug, Fail)]
#[fail(display = "Database Error")]
pub enum Error {
    #[fail(display = "maximum key index reached for account")]
    IndexOverflow,
    #[fail(display = "mutex poison error")]
    MutexPoison,
    #[fail(display = "database failed: {}", _0)]
    Db(#[cause] db::Error),
    #[fail(display = "cipher failed {}", _0)]
    Cipher(#[cause] witnet_crypto::cipher::Error),
    #[fail(display = "{}", _0)]
    Failure(#[cause] failure::Error),
    #[fail(display = "key derivation failed: {}", _0)]
    KeyDerivation(#[cause] types::KeyDerivationError),
    #[fail(display = "bech32 failed: {}", _0)]
    Bech32(#[cause] bech32::Error),
}

impl From<failure::Error> for Error {
    fn from(err: failure::Error) -> Self {
        Error::Failure(err)
    }
}

impl From<witnet_crypto::cipher::Error> for Error {
    fn from(err: witnet_crypto::cipher::Error) -> Self {
        Error::Cipher(err)
    }
}

impl<T> From<std::sync::PoisonError<T>> for Error {
    fn from(_err: std::sync::PoisonError<T>) -> Self {
        Error::MutexPoison
    }
}

impl From<db::Error> for Error {
    fn from(err: db::Error) -> Self {
        Error::Db(err)
    }
}

impl From<types::KeyDerivationError> for Error {
    fn from(err: types::KeyDerivationError) -> Self {
        Error::KeyDerivation(err)
    }
}

impl From<bech32::Error> for Error {
    fn from(err: bech32::Error) -> Self {
        Error::Bech32(err)
    }
}
