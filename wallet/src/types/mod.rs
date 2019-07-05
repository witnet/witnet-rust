use std::sync::Arc;

use witnet_crypto as crypto;
pub use witnet_data_structures::chain::RADRequest;
use witnet_protected as protected;

pub mod radon;
pub mod sessions;
pub mod storage;

pub use radon::*;
pub use sessions::Sessions;
pub use storage::*;

pub type SubscriptionId = jsonrpc_pubsub::SubscriptionId;

pub type Bytes = Vec<u8>;

pub type WalletId = Arc<String>;

pub type SessionId = Arc<String>;

pub type Secret = protected::Protected;

pub type Password = protected::ProtectedString;

pub type SharedDB = Arc<rocksdb::DB>;

#[derive(Clone)]
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

pub struct UnlockedWallet {
    pub(crate) key: Key,
    pub(crate) wallet: WalletContent,
}
