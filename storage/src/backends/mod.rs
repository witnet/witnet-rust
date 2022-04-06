//! # Storage backends
//!
//! These modules implement the Storage trait for whatever struct
//! containing state for specific storage solutions (databases,
//! volatile memory, flat files, etc.).

pub mod hashmap;
pub mod nobackend;
#[cfg(feature = "rocksdb-backend")]
pub mod rocksdb;
