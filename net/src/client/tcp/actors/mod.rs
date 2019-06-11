//! Defines all actors of the websockets client.

pub mod error;
pub mod jsonrpc;

pub use error::Error;
pub use jsonrpc::JsonRpcClient;
