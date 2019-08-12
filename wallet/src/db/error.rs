use failure::Fail;

use witnet_crypto as crypto;

#[derive(Debug, Fail)]
#[fail(display = "db error")]
pub enum Error {
    #[fail(display = "mutex poison error")]
    MutexPoison,
    #[fail(display = "db key {:?} not found", _0)]
    KeyNotFound(String),
    #[fail(display = "rocksdb error: {}", _0)]
    Db(#[cause] rocksdb::Error),
    #[fail(display = "db ser/der failed: {}", _0)]
    Serde(#[cause] bincode::Error),
    #[fail(display = "db enc/dec failed: {}", _0)]
    Cipher(#[cause] crypto::cipher::Error),
}

impl From<rocksdb::Error> for Error {
    fn from(err: rocksdb::Error) -> Self {
        Error::Db(err)
    }
}

impl From<bincode::Error> for Error {
    fn from(err: bincode::Error) -> Self {
        Error::Serde(err)
    }
}

impl From<crypto::cipher::Error> for Error {
    fn from(err: crypto::cipher::Error) -> Self {
        Error::Cipher(err)
    }
}

impl<T> From<std::sync::PoisonError<T>> for Error {
    fn from(_err: std::sync::PoisonError<T>) -> Self {
        Error::MutexPoison
    }
}
