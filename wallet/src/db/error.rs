use thiserror::Error;

#[derive(Debug, Error)]
#[error("Database Error")]
pub enum Error {
    #[error("mutex poison error")]
    MutexPoison,
    #[error("db key not found (key: {key:?})")]
    DbKeyNotFound { key: String },
    #[error("rocksdb failed: {0}")]
    Rocksdb(rocksdb::Error),
    #[error("bincode failed: {0}")]
    Bincode(bincode::Error),
    #[error("cipher failed {0}")]
    Cipher(witnet_crypto::cipher::Error),
    #[error("{0}")]
    Failure(anyhow::Error),
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

impl From<anyhow::Error> for Error {
    fn from(err: anyhow::Error) -> Self {
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
