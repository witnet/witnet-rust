//! # Crypto actor
//!
//! This actor is in charge of performing blocking crypto operations.
use std::cell::RefCell;

use actix::prelude::*;
use rand::Rng as _;

use witnet_crypto::{hash::HashFunction, key::MasterKeyGen, pbkdf2::pbkdf2_sha256};
use witnet_protected::ProtectedString;

use crate::{crypto, types};

mod handlers;

pub use handlers::*;

pub struct Crypto {
    seed_password: ProtectedString,
    master_key_salt: Vec<u8>,
    id_hash_iterations: u32,
    id_hash_function: HashFunction,
    rng: RefCell<rand::rngs::ThreadRng>,
}

impl Crypto {
    /// Start the actor.
    pub fn start(
        seed_password: ProtectedString,
        master_key_salt: Vec<u8>,
        id_hash_iterations: u32,
        id_hash_function: HashFunction,
    ) -> Addr<Self> {
        SyncArbiter::start(1, move || Self {
            seed_password: seed_password.clone(),
            master_key_salt: master_key_salt.clone(),
            id_hash_iterations,
            id_hash_function: id_hash_function.clone(),
            rng: RefCell::new(rand::thread_rng()),
        })
    }

    /// Generates the HD Master ExtendedKey for a wallet
    pub fn gen_master_key(
        &self,
        seed_source: types::SeedSource,
    ) -> Result<types::WalletMasterSK, crypto::Error> {
        let key = match seed_source {
            types::SeedSource::Mnemonics(mnemonic) => {
                let seed = mnemonic.seed(&self.seed_password);

                MasterKeyGen::new(seed)
                    .with_key(self.master_key_salt.as_ref())
                    .generate()
                    .map_err(crypto::Error::KeyGenFailed)?
            }
            types::SeedSource::Xprv => {
                // TODO: Implement key generation from xprv
                unimplemented!("xprv not implemented yet")
            }
        };

        Ok(key)
    }

    /// Generate an ID for a wallet
    pub fn gen_id(&self, key: &types::WalletMasterSK) -> types::WalletId {
        match self.id_hash_function {
            HashFunction::Sha256 => {
                let password = [key.secret(), key.chain_code].concat();
                let id_bytes = pbkdf2_sha256(
                    password.as_ref(),
                    self.master_key_salt.as_ref(),
                    self.id_hash_iterations,
                );

                types::WalletId::new(hex::encode(id_bytes))
            }
        }
    }

    /// Generate a Session ID for an unlocked wallet
    pub fn gen_session_id(&self, key: &types::Key) -> types::SessionId {
        match self.id_hash_function {
            HashFunction::Sha256 => {
                let rand_bytes: [u8; 32] = self.rng.borrow_mut().gen();
                let password =
                    [key.secret.as_ref(), key.salt.as_ref(), rand_bytes.as_ref()].concat();
                let id_bytes = pbkdf2_sha256(
                    password.as_ref(),
                    self.master_key_salt.as_ref(),
                    self.id_hash_iterations,
                );

                types::SessionId::new(hex::encode(id_bytes))
            }
        }
    }
}

impl Actor for Crypto {
    type Context = SyncContext<Self>;
}
