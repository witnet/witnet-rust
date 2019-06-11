//! # Error type for the Storage actor handlers.
use failure::Fail;

/// Error type for errors that may originate in the Storage actor.
#[derive(Debug, Fail)]
pub enum Error {
    #[fail(display = "failed to deserialize value from bincode")]
    DeserializeFailed(#[cause] bincode::Error),
    #[fail(display = "couldn't open database file")]
    OpenDbFailed(#[cause] rocksdb::Error),
    #[fail(display = "failed to read key from database")]
    DbGetFailed(#[cause] rocksdb::Error),
}
