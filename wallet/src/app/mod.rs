//! # App Actor specific data types
use std::sync::Arc;

use witnet_crypto::mnemonic::{Length, Mnemonic};
use witnet_protected::ProtectedString;

pub mod error;

pub use error::Error;

pub struct CreateWallet {
    pub(crate) name: Option<String>,
    pub(crate) caption: Option<String>,
    pub(crate) password: ProtectedString,
    pub(crate) seed_source: SeedSource,
}

pub struct CreateMnemonics {
    pub(crate) length: Length,
}

pub enum SeedSource {
    Mnemonics(Mnemonic),
    Xprv,
}

pub type SessionId = Arc<String>;
