//! Error type definitions for the Storage module.

use failure::Fail;
use std::fmt;
use witnet_util::error::WitnetResult;

/// Storage Error
#[derive(Debug, Fail)]
#[fail(display = "{} : at \"{}\", msg {}", kind, info, msg)]
pub struct StorageError {
    /// Operation kind
    kind: StorageErrorKind,
    /// Operation parameter
    info: String,
    /// Error message from database
    msg: String,
}

impl StorageError {
    /// Create a storage error based on operation kind and related info.
    pub fn new(kind: StorageErrorKind, info: String, msg: String) -> Self {
        Self { kind, info, msg }
    }
}

/// Storage Errors while operating on database
#[derive(Debug)]
pub enum StorageErrorKind {
    /// Errors when create a connection to backend database
    Connection,
    /// Errors when adding a key to database
    Put,
    /// Errors when getting a value of a key from database
    Get,
    /// Errors when deleting a key/value pair
    Delete,
    /// Errors when converting a value into bytes
    Encode,
    /// Errors when creating a value from bytes
    Decode,
}

impl fmt::Display for StorageErrorKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "StorageError::{:?}", self)
    }
}

/// Result type for the Storage module.
/// This is the only return type acceptable for any public method in a storage backend.
pub type StorageResult<T> = WitnetResult<T, StorageError>;
