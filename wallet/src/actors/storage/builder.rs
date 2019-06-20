use actix::prelude::*;
use failure::Error;

use super::Storage;
use crate::storage;

pub struct Builder {
    params: storage::Params,
}

impl Builder {
    pub fn new() -> Self {
        let params = storage::Params {
            encrypt_hash_iterations: 10_000,
            encrypt_iv_length: 16,
            encrypt_salt_length: 32,
        };

        Self { params }
    }

    /// Start an instance of the actor inside a SyncArbiter.
    pub fn start(self) -> Result<Addr<Storage>, Error> {
        // Spawn one thread with the storage actor (because is blocking). Do not use more than one
        // thread, otherwise you'll receive and error because RocksDB only allows one connection at a
        // time.
        let addr = SyncArbiter::start(1, move || Storage::new(self.params.clone()));

        Ok(addr)
    }
}
