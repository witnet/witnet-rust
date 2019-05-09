//! # Error type for the Storage actor handlers.
use std::fmt;

/// Error type for errors that may occur inside the Storage actor handlers.
#[derive(Debug)]
pub enum Error {
    Serialization(bincode::Error),
    Db(rocksdb::Error),
}

impl fmt::Display for Error {
    fn fmt(&self, fmt: &mut fmt::Formatter<'_>) -> fmt::Result {
        match *self {
            Error::Serialization(ref e) => write!(fmt, "(De)serialization error: {}", e),
            Error::Db(ref e) => write!(fmt, "Database error: {}", e),
        }
    }
}
