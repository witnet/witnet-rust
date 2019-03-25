//! # Storage backends
//!
//! These modules implement the Storage trait for whatever struct
//! containing state for specific storage solutions (databases,
//! volatile memory, flat files, etc.).

#[cfg(feature = "crypto-backend")]
pub mod crypto;
pub mod hashmap;
pub mod nobackend;
#[cfg(feature = "rocksdb-backend")]
pub mod rocksdb;
