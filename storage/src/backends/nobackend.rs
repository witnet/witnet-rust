//! # NoBackend storage backend
//!
//! This backend performs no storage at all and always fails to do any operation.
use failure::bail;

use crate::storage::{Result, Storage};

/// A Backend that is not persisted
///
/// This backend fails to perform any operation defined in
/// [`Storage`](Storage)
pub struct Backend;

impl Storage for Backend {
    fn get(&self, _key: &[u8]) -> Result<Option<Vec<u8>>> {
        bail!("This is a no backend storage")
    }

    fn put(&mut self, _key: Vec<u8>, _value: Vec<u8>) -> Result<()> {
        bail!("This is a no backend storage")
    }

    fn delete(&mut self, _key: &[u8]) -> Result<()> {
        bail!("This is a no backend storage")
    }
}
