//! Error type definitions for the Storage module.

use witnet_util as util;

use failure::Fail;

/// Storage Error caused be `C`.
#[derive(Debug, Fail)]
pub enum StorageError {
    /// Error while trying to open a connection to the storage backend.
    #[fail(
        display = "connection error: at \"{}\", rocksdb_msg {}",
        path,
        msg
    )]
    Connection {
        /// Connection address
        path: String,
        /// Error message from rocksdb
        msg: String,
    },

    /// Error while trying to put a value for a certain key.
    #[fail(display = "putting error: key={}, rocksdb_msg {}", key, msg)]
    Put {
        /// Key
        key: String,
        /// Error message from rocksdb
        msg: String,
    },

    /// Error while trying to get the value for a certain key.
    #[fail(display = "getting error: key={}, rocksdb_msg {}", key, msg)]
    Get {
        /// Key
        key: String,
        /// Error message from rocksdb
        msg: String,
    },

    #[fail(display = "deletion error: key={}, rocksdb_msg {}", key, msg)]
    /// Error while trying to delete a key/value pair.
    Delete {
        /// Key
        key: String,
        /// Error message from rocksdb
        msg: String,
    },
}

/// Result type for the Storage module.
/// This is the only return type acceptable for any public method in a storage backend.
pub type StorageResult<T> = util::error::WitnetResult<T, StorageError>;
