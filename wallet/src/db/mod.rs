pub mod encrypted;
pub mod error;
mod keys;
pub mod model;
pub mod params;
pub mod plain;
pub mod wallet;
pub mod wallets;

pub use encrypted::*;
pub use error::*;
pub use model::*;
pub use params::*;
pub use plain::*;
pub use wallet::*;
pub use wallets::*;

pub type Result<T> = std::result::Result<T, Error>;
