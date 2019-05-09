//! TODO: doc
use std::fmt;

use actix::MailboxError;
use serde_json::Error as JsonError;

use crate::actors::storage::Error as StorageError;

/// TODO: doc
#[derive(Debug)]
pub enum Error {
    Mailbox(MailboxError),
    Storage(StorageError),
    Serialization(JsonError),
}

impl fmt::Display for Error {
    fn fmt(&self, fmt: &mut fmt::Formatter<'_>) -> fmt::Result {
        match *self {
            Error::Mailbox(ref e) => write!(fmt, "mailbox error: {}", e),
            Error::Storage(ref e) => write!(fmt, "storage error: {}", e),
            Error::Serialization(ref e) => write!(fmt, "(de)serialization error: {}", e),
        }
    }
}
