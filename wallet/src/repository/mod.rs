mod error;
pub mod keys;
mod wallet;
mod wallets;

pub use error::Error;
pub use wallet::Wallet;
pub use wallets::Wallets;

pub type Result<T> = std::result::Result<T, Error>;
