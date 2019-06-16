use std::sync::Arc;

use actix::prelude::*;

use witnet_protected::ProtectedString;

use super::Crypto;
use crate::wallet;

pub struct Builder {
    params: Params,
    concurrency: usize,
}

pub struct Params {
    pub(super) seed_password: ProtectedString,
    pub(super) master_key_salt: Vec<u8>,
    pub(super) id_hash_iterations: u32,
    pub(super) id_hash_function: wallet::HashFunction,
}

impl Builder {
    pub fn start(self) -> Addr<Crypto> {
        let params = Arc::new(self.params);

        SyncArbiter::start(self.concurrency, move || Crypto {
            params: params.clone(),
        })
    }
}

impl Default for Builder {
    fn default() -> Self {
        let params = Params {
            seed_password: ProtectedString::new(""),
            master_key_salt: b"Bitcoin seed".to_vec(),
            id_hash_iterations: 4096,
            id_hash_function: wallet::HashFunction::Sha256,
        };

        Self {
            params,
            concurrency: 1,
        }
    }
}
