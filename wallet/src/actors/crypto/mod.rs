//! # Crypto actor
//!
//! This actor is in charge of performing blocking crypto operations.

use actix::prelude::*;

use witnet_crypto::mnemonic::{Mnemonic, Seed};
use witnet_protected::ProtectedString;

use crate::wallet;

pub mod builder;
pub mod error;
pub mod handlers;

pub use error::Error;
pub use handlers::*;

pub struct Crypto {
    seed_password: ProtectedString,
}

impl Crypto {
    pub fn build() -> builder::CryptoBuilder {
        builder::CryptoBuilder::default()
    }

    pub fn gen_seed(&self, seed_source: wallet::SeedSource) -> Result<Seed, Error> {
        let seed = match seed_source.source {
            wallet::SeedFrom::Mnemonics => {
                let mnemonic =
                    Mnemonic::from_phrase(seed_source.data).map_err(error::Error::WrongMnemonic)?;

                mnemonic.seed(&self.seed_password)
            }
            wallet::SeedFrom::Xprv => unimplemented!(),
        };

        Ok(seed)
    }
}

impl Actor for Crypto {
    type Context = SyncContext<Self>;
}
