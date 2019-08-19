use failure::Fail;

#[derive(Debug, Fail)]
#[fail(display = "Database Error")]
pub enum Error {
    #[fail(display = "mutex poison error")]
    MutexPoison,
    #[fail(display = "db key not found")]
    DbKeyNotFound,
    #[fail(display = "rocksdb failed: {}", _0)]
    Rocksdb(#[cause] rocksdb::Error),
    #[fail(display = "bincode failed: {}", _0)]
    Bincode(#[cause] bincode::Error),
    #[fail(display = "cipher failed {}", _0)]
    Cipher(#[cause] witnet_crypto::cipher::Error),
    #[fail(display = "{}", _0)]
    Failure(#[cause] failure::Error),
}

impl From<rocksdb::Error> for Error {
    fn from(err: rocksdb::Error) -> Self {
        Error::Rocksdb(err)
    }
}

impl From<bincode::Error> for Error {
    fn from(err: bincode::Error) -> Self {
        Error::Bincode(err)
    }
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
