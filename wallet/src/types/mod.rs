use std::sync::Arc;

use witnet_crypto as crypto;
pub use witnet_data_structures::chain::RADRequest;
use witnet_protected as protected;

mod radon;
mod storage;

pub use radon::*;
pub use storage::*;

pub type Bytes = Vec<u8>;

pub type WalletId = Arc<String>;

pub type SessionId = Arc<String>;

pub type Secret = protected::Protected;

pub type Password = protected::ProtectedString;

pub type SharedKey = Arc<Key>;

pub type SharedDB = Arc<rocksdb::DB>;

pub struct Key {
    pub(crate) secret: Secret,
    pub(crate) salt: Bytes,
}

pub struct CreateWallet {
    pub(crate) name: Option<String>,
    pub(crate) caption: Option<String>,
    pub(crate) password: Password,
    pub(crate) seed_source: SeedSource,
}

pub struct CreateMnemonics {
    pub(crate) length: crypto::mnemonic::Length,
}

pub enum SeedSource {
    Mnemonics(crypto::mnemonic::Mnemonic),
    Xprv,
}
