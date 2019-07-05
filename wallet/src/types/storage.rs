//! Types that stored in the database.
//!
//! IMPORTAT: Changing these types will introduce a breaking change since created wallets won't be
//! readable.
use serde::{Deserialize, Serialize};

use witnet_crypto as crypto;

use super::*;

pub type WalletMasterSK = crypto::key::ExtendedSK;

#[derive(Debug, Deserialize, Serialize)]
pub struct WalletInfo {
    pub(crate) id: WalletId,
    pub(crate) name: Option<String>,
    pub(crate) caption: Option<String>,
}

#[derive(Serialize, Deserialize)]
pub struct Wallet {
    pub(crate) info: WalletInfo,
    pub(crate) content: WalletContent,
}

impl Wallet {
    pub fn new(info: WalletInfo, content: WalletContent) -> Self {
        Self { info, content }
    }
}

#[derive(Serialize, Deserialize)]
pub struct WalletContent {
    pub(crate) version: u32,
    pub(crate) master_key: WalletMasterSK,
    pub(crate) key_spec: Wip,
    pub(crate) purpose: u32,
    pub(crate) epoch_born: u32,
    pub(crate) epoch_last: u32,
    pub(crate) accounts: Vec<Account>,
}

impl WalletContent {
    const VERSION: u32 = 1;
    const PURPOSE: u32 = 0x8000_0003;

    pub fn new(master_key: WalletMasterSK, key_spec: Wip, accounts: Vec<Account>) -> Self {
        Self {
            master_key,
            key_spec,
            accounts,
            version: Self::VERSION,
            purpose: Self::PURPOSE,
            epoch_born: 0,
            epoch_last: 0,
        }
    }
}

#[derive(Serialize, Deserialize)]
pub struct Account {
    keychains: KeyChains,
    balance: u64,
}

impl Account {
    pub fn new(keychains: KeyChains) -> Self {
        Self {
            keychains,
            balance: 0,
        }
    }
}

#[derive(Serialize, Deserialize)]
pub struct KeyChains {
    external: KeyChain,
    internal: KeyChain,
    rad: KeyChain,
}

impl KeyChains {
    pub fn new(mut path: KeyPath) -> Self {
        let internal = KeyChain::new(path.join(0));
        let external = KeyChain::new(path.join(1));
        let rad = KeyChain::new(path.join(2));
        Self {
            internal,
            external,
            rad,
        }
    }
}

#[derive(Serialize, Deserialize)]
pub struct KeyChain {
    path: KeyPath,
    final_keys: Vec<FinalKey>,
}

impl KeyChain {
    pub fn new(path: KeyPath) -> Self {
        Self {
            path,
            final_keys: Vec::new(),
        }
    }
}

#[derive(Serialize, Deserialize)]
pub struct FinalKey {
    path: KeyPath,
    key: crypto::key::ExtendedSK,
    pkh: String,
    utxos: Vec<Utxo>,
    stxos: Vec<Stxo>,
}

pub type Utxo = ();
pub type Stxo = ();

#[derive(Clone, Serialize, Deserialize)]
pub struct KeyPath {
    path: Vec<u32>,
}

impl KeyPath {
    const HARDENED_KEY_INDEX: u32 = 0x8000_0000;

    pub fn master() -> Self {
        Self { path: vec![] }
    }

    pub fn hardened(self, index: u32) -> Self {
        self.index(Self::HARDENED_KEY_INDEX + index)
    }

    pub fn index(mut self, index: u32) -> Self {
        self.path.push(index);
        self
    }

    pub fn join(&mut self, index: u32) -> KeyPath {
        let path = self.clone();
        path.index(index)
    }
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Wip {
    Wip3,
}
