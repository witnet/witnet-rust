//! # BIP32 Key generation and derivation
//!
//! Example
//!
//! ```
//! # use witnet_wallet::mnemonic;
//! # use witnet_wallet::key;
//! let passphrase = "";
//! let seed = mnemonic::MnemonicGen::new().generate().seed(passphrase);
//! let ext_key = key::MasterKeyGen::new(seed).generate();
//! ```

use failure::Fail;
use hmac::{Hmac, Mac};
use secp256k1;
use sha2;

/// Default HMAC key used when generating a Master Key with
/// [generate_master](generate_master)
pub static DEFAULT_HMAC_KEY: &str = "Witnet seed";

/// secp256k1 Secret Key with Chain Code
pub type ExtendedSK = ExtendedKey<secp256k1::SecretKey>;

/// secp256k1 Public Key with Chain Code
pub type ExtendedPK = ExtendedKey<secp256k1::PublicKey>;

/// The error type for [generate_master](generate_master)
#[derive(Debug, PartialEq, Fail)]
pub enum MasterKeyGenError {
    /// Invalid hmac key length
    #[fail(display = "The length of the hmac key is invalid")]
    InvalidKeyLength,
    /// Invalid seed length
    #[fail(display = "The length of the seed is invalid, must be between 128/256 bits")]
    InvalidSeedLength,
}

/// BIP32 Master Secret Key generator
pub struct MasterKeyGen<'a, S> {
    seed: S,
    key: &'a str,
}

impl<'a, S> MasterKeyGen<'a, S>
where
    S: AsRef<[u8]>,
{
    /// Create a new master key generator
    pub fn new(seed: S) -> Self {
        Self {
            key: DEFAULT_HMAC_KEY,
            seed: seed,
        }
    }

    /// Use the given key as the HMAC key
    pub fn with_key(mut self, key: &'a str) -> Self {
        self.key = key;
        self
    }

    /// Consume this generator and return the BIP32 extended Master Secret Key
    /// [Extended Key](ExtendedSK)
    pub fn generate(self) -> Result<ExtendedSK, MasterKeyGenError> {
        let seed_bytes = self.seed.as_ref();
        let seed_len = seed_bytes.len();

        if seed_len < 16 || seed_len > 64 {
            Err(MasterKeyGenError::InvalidSeedLength)?
        }

        let key_bytes = self.key.as_ref();
        let mut mac = Hmac::<sha2::Sha512>::new_varkey(key_bytes)
            .map_err(|_| MasterKeyGenError::InvalidKeyLength)?;
        mac.input(seed_bytes);
        let result = mac.result().code();
        let (sk_bytes, chain_code_bytes) = result.split_at(32);

        // secret/chain_code computation might panic if length returned by hmac is wrong
        let secret = secp256k1::SecretKey::from_slice(sk_bytes).expect("Secret Key length error");
        let mut chain_code = [0u8; 32];
        chain_code.copy_from_slice(chain_code_bytes);

        Ok(ExtendedKey { secret, chain_code })
    }
}

/// Extended Key is just a Key with a Chain Code
pub struct ExtendedKey<K> {
    pub secret: K,
    pub chain_code: [u8; 32],
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_master_invalid_seed() {
        let seed = "too short seed";
        let result = MasterKeyGen::new(seed).generate();

        assert!(result.is_err());
    }

    #[test]
    fn test_generate_master() {
        let seed = [0; 32];
        let result = MasterKeyGen::new(seed).generate();

        assert!(result.is_ok());
    }

    #[test]
    fn test_generate_master_other_key() {
        let seed = [0; 32];
        let result = MasterKeyGen::new(seed).with_key("Bitcoin seed").generate();

        assert!(result.is_ok());
    }
}
