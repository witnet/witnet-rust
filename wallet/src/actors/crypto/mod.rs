//! # Crypto actor
//!
//! This actor is in charge of performing blocking crypto operations.
use std::sync::Arc;

use actix::prelude::*;

use witnet_crypto::{key::MasterKeyGen, mnemonic::Mnemonic, pbkdf2::pbkdf2_sha256};

use crate::wallet;

pub mod builder;
pub mod error;
pub mod handlers;

pub use error::Error;
pub use handlers::*;

pub struct Crypto {
    params: Arc<builder::Params>,
}

impl Crypto {
    pub fn build() -> builder::Builder {
        builder::Builder::default()
    }

    /// Generates the HD Master ExtendedKey for a wallet
    pub fn gen_master_key(
        &self,
        seed_source: wallet::SeedSource,
    ) -> Result<wallet::MasterKey, Error> {
        let key = match seed_source.source {
            wallet::SeedFrom::Mnemonics => {
                let mnemonic =
                    Mnemonic::from_phrase(seed_source.data).map_err(error::Error::WrongMnemonic)?;
                let seed = mnemonic.seed(&self.params.seed_password);

                MasterKeyGen::new(seed)
                    .with_key(self.params.master_key_salt.as_ref())
                    .generate()
                    .map_err(Error::KeyGenFailed)?
            }
            wallet::SeedFrom::Xprv => unimplemented!(),
        };

        Ok(key)
    }

    /// Generate an ID for a wallet
    pub fn gen_id(&self, key: &wallet::MasterKey) -> String {
        match self.params.id_hash_function {
            wallet::HashFunction::Sha256 => {
                let password = [key.secret(), key.chain_code].concat();
                let id_bytes = pbkdf2_sha256(
                    password.as_ref(),
                    self.params.master_key_salt.as_ref(),
                    self.params.id_hash_iterations,
                );

                hex::encode(id_bytes)
            }
        }
    }
}

impl Actor for Crypto {
    type Context = SyncContext<Self>;
}
