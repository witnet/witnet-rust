use std::collections::HashMap;
use std::path;
use std::sync::{Arc, Mutex, RwLock};
use std::time;

use serde::{Deserialize, Serialize};

pub use witnet_crypto::{
    hash::HashFunction,
    key::MasterKeyGen,
    key::{
        ExtendedPK, ExtendedSK, KeyDerivationError, KeyError, KeyPath, MasterKeyGenError,
        SignEngine, ONE_KEY, PK, SK,
    },
    mnemonic::{Length as MnemonicLength, Mnemonic, MnemonicGen},
};
pub use witnet_data_structures::chain::RADRequest;
pub use witnet_protected::{Protected, ProtectedString};
pub use witnet_rad::{error::RadError, types::RadonTypes};

use crate::db;

#[cfg(test)]
pub mod factories;
mod session_id_impls;
mod session_impls;

pub enum SeedSource {
    Mnemonic(Mnemonic),
    Xprv(ProtectedString),
}

pub struct CreateWallet {
    pub name: String,
    pub caption: Option<String>,
    pub db_url: String,
    pub seed_source: SeedSource,
    pub password: ProtectedString,
}

pub struct Account {
    pub index: u32,
    pub external_key: ExtendedSK,
    pub internal_key: ExtendedSK,
}

#[derive(Debug, Clone, Default)]
pub struct WalletsConfig {
    pub testnet: bool,
    pub seed_password: ProtectedString,
    pub master_key_salt: Vec<u8>,
    pub session_expires_in: time::Duration,
    pub requests_timeout: time::Duration,
}

#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct SessionId(String);

pub struct Session {
    pub expiration: time::Instant,
    pub wallets: HashMap<i32, db::Database>,
}

#[derive(Clone)]
pub struct State {
    /// Wallets Info database which contains public info of available
    /// wallets.
    pub db: db::Database,
    /// Path where wallet databases are stored.
    pub db_path: path::PathBuf,
    /// Configuration params used when creating a new wallet.
    pub wallets_config: WalletsConfig,
    pub sign_engine: SignEngine,
    pub rng: Arc<Mutex<rand::rngs::OsRng>>,
    pub sessions: Arc<RwLock<HashMap<SessionId, Session>>>,
}
