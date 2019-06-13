use actix::prelude::*;

use witnet_protected::ProtectedString;

use super::Crypto;

pub struct CryptoBuilder {
    seed_password: ProtectedString,
    concurrency: usize,
}

impl CryptoBuilder {
    pub fn start(self) -> Addr<Crypto> {
        let passwd = self.seed_password;
        SyncArbiter::start(self.concurrency, move || Crypto {
            seed_password: passwd.clone(),
        })
    }
}

impl Default for CryptoBuilder {
    fn default() -> Self {
        Self {
            seed_password: ProtectedString::new(""),
            concurrency: 1,
        }
    }
}
