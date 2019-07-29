pub use jsonrpc_core::{Params as RpcParams, Value as RpcValue};
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

pub type Password = ProtectedString;

pub type Secret = Protected;

pub enum SeedSource {
    Mnemonics(Mnemonic),
    Xprv,
}
