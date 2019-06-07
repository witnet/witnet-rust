#[cfg(feature = "wallet")]
mod with_wallet;
#[cfg(not(feature = "wallet"))]
mod without_wallet;

#[cfg(feature = "wallet")]
pub use with_wallet::*;

#[cfg(not(feature = "wallet"))]
pub use without_wallet::*;
