//! TODO: doc
use std::fmt;

use actix::MailboxError;
use serde_json::Error as JsonError;

use crate::actors::storage::Error as StorageError;
use witnet_rad::error::RadError;

/// TODO: doc
#[derive(Debug)]
pub enum Error {
    Mailbox(MailboxError),
    Storage(StorageError),
    Serialization(JsonError),
    Rad(RadError),
}

impl fmt::Display for Error {
    fn fmt(&self, fmt: &mut fmt::Formatter<'_>) -> fmt::Result {
        match *self {
            Error::Mailbox(ref e) => write!(fmt, "mailbox error: {}", e),
            Error::Storage(ref e) => write!(fmt, "storage error: {}", e),
            Error::Serialization(ref e) => write!(fmt, "(de)serialization error: {}", e),
            Error::Rad(ref e) => write!(fmt, "rad error: {}", e),
        }
    }
}
