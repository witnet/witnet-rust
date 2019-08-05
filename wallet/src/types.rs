use std::sync::{Arc, Mutex};

pub use jsonrpc_core::Params as RpcParams;
pub use jsonrpc_pubsub::{Sink, Subscriber, SubscriptionId};
pub use serde_json::Value as Json;

pub use witnet_crypto::{
    hash::HashFunction,
    key::{ExtendedPK, ExtendedSK, KeyPath},
    mnemonic::{Length as MnemonicLength, Mnemonic, MnemonicGen},
};
pub use witnet_data_structures::{
    chain::{Block as ChainBlock, RADRequest, ValueTransferOutput},
    transaction::VTTransactionBody,
};
pub use witnet_net::client::tcp::jsonrpc::Request as RpcRequest;
use witnet_protected::{Protected, ProtectedString};
pub use witnet_rad::types::RadonTypes;

pub type Password = ProtectedString;

pub type Secret = Protected;

pub enum SeedSource {
    Mnemonics(Mnemonic),
    Xprv,
}

pub struct Wallet {
    pub id: String,
    pub enc_key: Secret,
    pub iv: Vec<u8>,
    pub account_balance: u64,
    pub account_index: u32,
    pub account_external: ExtendedSK,
    pub account_internal: ExtendedSK,
    pub account_rad: ExtendedSK,
    pub mutex: Arc<Mutex<()>>,
}

pub struct ExternalWallet {
    pub id: String,
    pub enc_key: Secret,
    pub iv: Vec<u8>,
    pub account_index: u32,
    pub account_external: ExtendedSK,
    pub mutex: Arc<Mutex<()>>,
}

pub struct SimpleWallet {
    pub id: String,
    pub enc_key: Secret,
    pub iv: Vec<u8>,
    pub account_index: u32,
    pub mutex: Arc<Mutex<()>>,
}

impl From<&Wallet> for SimpleWallet {
    fn from(wallet: &Wallet) -> Self {
        Self {
            id: wallet.id.clone(),
            enc_key: wallet.enc_key.clone(),
            iv: wallet.iv.clone(),
            account_index: wallet.account_index,
            mutex: wallet.mutex.clone(),
        }
    }
}

pub struct Account {
    pub index: u32,
    pub external: ExtendedSK,
    pub internal: ExtendedSK,
    pub rad: ExtendedSK,
    pub balance: u64,
}
