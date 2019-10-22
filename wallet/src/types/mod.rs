pub use witnet_crypto::{
    hash::HashFunction,
    key::MasterKeyGen,
    key::{
        ExtendedPK, ExtendedSK, KeyDerivationError, KeyError, KeyPath, MasterKeyGenError,
        SignEngine, ONE_KEY, PK, SK,
    },
    mnemonic::{Length as MnemonicLength, Mnemonic, MnemonicGen},
};
pub use witnet_data_structures::chain::RADRequest;
pub use witnet_protected::{Protected, ProtectedString};
pub use witnet_rad::{error::RadError, types::RadonTypes};

#[cfg(test)]
pub mod factories;

pub enum SeedSource {
    Mnemonic(Mnemonic),
    Xprv(ProtectedString),
}

pub struct CreateWallet {
    pub name: String,
    pub caption: Option<String>,
    pub db_url: String,
    pub seed_source: SeedSource,
    pub password: ProtectedString,
}

pub struct Account {
    pub index: u32,
    pub external_key: ExtendedSK,
    pub internal_key: ExtendedSK,
}

#[derive(Debug, Clone)]
pub struct WalletsConfig {
    pub seed_password: ProtectedString,
    pub master_key_salt: Vec<u8>,
}
