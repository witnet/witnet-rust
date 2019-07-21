pub use jsonrpc_core::Params as RpcParams;
pub use jsonrpc_pubsub::{Sink, Subscriber, SubscriptionId};
pub use serde_json::Value as Json;

pub use witnet_crypto::{
    hash::HashFunction,
    key::{ExtendedPK, ExtendedSK, KeyPath},
    mnemonic::{Length as MnemonicLength, Mnemonic, MnemonicGen},
};
pub use witnet_data_structures::chain::Block as ChainBlock;
pub use witnet_data_structures::chain::RADRequest;
pub use witnet_net::client::tcp::jsonrpc::Request as RpcRequest;
use witnet_protected::{Protected, ProtectedString};
pub use witnet_rad::types::RadonTypes;

use crate::model;

pub type Password = ProtectedString;

pub type Secret = Protected;

pub enum SeedSource {
    Mnemonics(Mnemonic),
    Xprv,
}

#[derive(Clone)]
pub struct Wallet {
    pub name: Option<String>,
    pub caption: Option<String>,
    pub accounts: Vec<u32>,
    pub account: model::Account,
    pub enc_key: Secret,
    pub last_receive_index: u32,
}

impl Wallet {
    /// Increment the receive index and return the old one.
    /// This function panics if an overflow happens.
    pub fn increment_receive_index(&mut self) -> u32 {
        let index = self.last_receive_index;
        self.last_receive_index = self
            .last_receive_index
            .checked_add(1)
            .expect("receive index overflow");

        index
    }
}

pub struct ReceiveKey {
    pub address: String,
    pub path: String,
}

impl ReceiveKey {
    pub fn address(&self) -> String {
        "todo implement".to_string()
    }
}
