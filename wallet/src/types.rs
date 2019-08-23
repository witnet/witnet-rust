use std::sync::Arc;

pub use jsonrpc_core::{Params as RpcParams, Value as RpcValue};
pub use jsonrpc_pubsub::{Sink, Subscriber, SubscriptionId};
pub use serde_json::Value as Json;

pub use witnet_crypto::{
    hash::HashFunction,
    key::{ExtendedPK, ExtendedSK, KeyDerivationError, KeyPath, SignEngine},
    mnemonic::{Length as MnemonicLength, Mnemonic, MnemonicGen},
};
pub use witnet_data_structures::{
    chain::{Block as ChainBlock, Hashable, RADRequest, ValueTransferOutput},
    transaction::VTTransactionBody,
};
pub use witnet_net::client::tcp::jsonrpc::Request as RpcRequest;
use witnet_protected::{Protected, ProtectedString};
pub use witnet_rad::types::RadonTypes;

use super::{db, repository};

pub type Password = ProtectedString;

pub type Secret = Protected;

pub type SessionWallet = Arc<repository::Wallet<db::EncryptedDb>>;

pub type Wallet = repository::Wallet<db::EncryptedDb>;

pub enum SeedSource {
    Mnemonics(Mnemonic),
    Xprv,
}

pub struct UnlockedSessionWallet {
    pub wallet: repository::Wallet<db::EncryptedDb>,
    pub data: WalletData,
    pub session_id: String,
}

pub struct UnlockedWallet {
    pub data: WalletData,
    pub session_id: String,
}

pub struct Account {
    pub index: u32,
    pub external: ExtendedSK,
    pub internal: ExtendedSK,
    pub rad: ExtendedSK,
}

pub struct WalletData {
    pub name: Option<String>,
    pub caption: Option<String>,
    pub balance: u64,
    pub current_account: u32,
    pub available_accounts: Vec<u32>,
}
