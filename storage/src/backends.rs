//! Storage backend modules.
//! These modules implement the Storage trait for whatever struct containing state for specific
//! storage solutions (databases, volatile memory, flat files, etc.).

pub mod in_memory;
#[cfg(feature = "rocksdb-backend")]
pub mod rocks;
