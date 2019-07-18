//! # BIP32 Key generation and derivation
//!
//! Example
//!
//! ```
//! # use witnet_crypto::{key, mnemonic};
//! let passphrase = "".into();
//! let seed = mnemonic::MnemonicGen::new().generate().seed(&passphrase);
//! let ext_key = key::MasterKeyGen::new(seed).generate();
//! ```
use std::{fmt, slice};

use failure::Fail;
use hmac::{Hmac, Mac};
use secp256k1::{PublicKey, Secp256k1, SecretKey};
#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};
use sha2;

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
    key: &'a [u8],
}

impl<'a, S> MasterKeyGen<'a, S>
where
    S: AsRef<[u8]>,
{
    /// Default HMAC key used when generating a Master Key with
    /// [generate_master](generate_master)
    const DEFAULT_HMAC_KEY: &'static [u8] = b"Bitcoin seed";

    /// Create a new master key generator
    pub fn new(seed: S) -> Self {
        Self {
            key: Self::DEFAULT_HMAC_KEY,
            seed,
        }
    }

    /// Use the given key as the HMAC key
    pub fn with_key(mut self, key: &'a [u8]) -> Self {
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

        let key_bytes = self.key;
        let mut mac = Hmac::<sha2::Sha512>::new_varkey(key_bytes)
            .map_err(|_| MasterKeyGenError::InvalidKeyLength)?;
        mac.input(seed_bytes);
        let result = mac.result().code();
        let (sk_bytes, chain_code_bytes) = result.split_at(32);

        // secret/chain_code computation might panic if length returned by hmac is wrong
        let secret_key = SecretKey::from_slice(sk_bytes).expect("Secret Key length error");
        let mut chain_code = [0u8; 32];
        chain_code.copy_from_slice(chain_code_bytes);

        Ok(ExtendedSK {
            secret_key,
            chain_code,
        })
    }
}
/// Error returned trying to derivate a key
#[derive(Debug, PartialEq, Fail)]
pub enum KeyDerivationError {
    /// Invalid hmac key length
    #[fail(display = "The length of the hmac key is invalid")]
    InvalidKeyLength,
    /// Invalid seed length
    #[fail(display = "The length of the seed is invalid, must be between 128/256 bits")]
    InvalidSeedLength,
    /// Secp256k1 internal error
    #[fail(display = "Error in secp256k1 crate")]
    Secp256k1Error(secp256k1::Error),
}

/// Secret Key
pub type SK = SecretKey;

/// Public Key
pub type PK = PublicKey;

/// Signing context for signature operations
///
/// `SignContext::new()`: all capabilities
/// `SignContext::signing_only()`: only be used for signing
/// `SignContext::verification_only()`: only be used for verification
pub type SignContext<C> = Secp256k1<C>;

/// Extended Key is just a Key with a Chain Code
#[derive(Clone, PartialEq, Eq, Debug)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct ExtendedSK {
    /// Secret key
    pub secret_key: SK,
    /// Chain code
    pub chain_code: [u8; 32],
}

impl ExtendedSK {
    /// Try to derive an extended private key from a given path
    pub fn derive(&self, path: &KeyPath) -> Result<ExtendedSK, KeyDerivationError> {
        let mut extended_sk = self.clone();
        for index in path.iter() {
            extended_sk = extended_sk.child(index)?
        }

        Ok(extended_sk)
    }
    /// get the secret
    pub fn secret(&self) -> [u8; 32] {
        let mut secret: [u8; 32] = [0; 32];
        secret.copy_from_slice(&self.secret_key[..]);

        secret
    }

    /// Try to get a private child key from parent
    pub fn child(&self, index: &KeyPathIndex) -> Result<ExtendedSK, KeyDerivationError> {
        let mut hmac512: Hmac<sha2::Sha512> =
            Hmac::new_varkey(&self.chain_code).map_err(|_| KeyDerivationError::InvalidKeyLength)?;
        let index_bytes = index.as_ref().to_be_bytes();

        if index.is_hardened() {
            hmac512.input(&[0]); // BIP-32 padding that makes key 33 bytes long
            hmac512.input(&self.secret_key[..]);
        } else {
            hmac512.input(
                &PublicKey::from_secret_key(&secp256k1::Secp256k1::new(), &self.secret_key)
                    .serialize(),
            );
        }

        let (chain_code, mut secret_key) = get_chain_code_and_secret(&index_bytes, hmac512)?;

        secret_key
            .add_assign(&self.secret_key[..])
            .map_err(KeyDerivationError::Secp256k1Error)?;

        Ok(ExtendedSK {
            secret_key,
            chain_code,
        })
    }
}

/// Represents an index inside a key derivation path.
/// See BIP-32 spec for more information.
#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct KeyPathIndex(u32);

impl KeyPathIndex {
    /// Check if the index is hardened or not.
    /// A hardened key index is a number falling in the range: index + 2^31.
    pub fn is_hardened(&self) -> bool {
        self.0 & KeyPath::HARDENED_KEY_INDEX == KeyPath::HARDENED_KEY_INDEX
    }
}

impl AsRef<u32> for KeyPathIndex {
    fn as_ref(&self) -> &u32 {
        &self.0
    }
}

impl fmt::Display for KeyPathIndex {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.is_hardened() {
            write!(f, "{}'", self.0 - KeyPath::HARDENED_KEY_INDEX)
        } else {
            write!(f, "{}", self.0)
        }
    }
}

/// Represents a key derivation path that can be used to derive extended private keys.
/// See BIP-32 spec for more information.
#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct KeyPath {
    path: Vec<KeyPathIndex>,
}

impl KeyPath {
    const HARDENED_KEY_INDEX: u32 = 0x8000_0000;

    /// Start a new key path starting at the master root: m/...
    pub fn new() -> Self {
        Self { path: vec![] }
    }

    /// Add a hardened-index index to the current path.
    ///
    /// Example
    /// ```
    /// # use witnet_crypto::key::KeyPath;
    /// let path = KeyPath::new().hardened(3).hardened(4);
    /// assert_eq!("m/3'/4'", format!("{}", path));
    /// ```
    pub fn hardened(mut self, idx: u32) -> Self {
        let index = Self::HARDENED_KEY_INDEX
            .checked_add(idx)
            .expect("key path hardened index overflow");
        self.path.push(KeyPathIndex(index));
        self
    }

    /// Add a normal (non-hardened) child index to the current path.
    ///
    /// Example
    /// ```
    /// # use witnet_crypto::key::KeyPath;
    /// let path = KeyPath::new().index(3).index(4);
    /// assert_eq!("m/3/4", format!("{}", path));
    /// ```
    pub fn index(mut self, index: u32) -> Self {
        assert!(index < Self::HARDENED_KEY_INDEX, "key path index overflow");
        self.path.push(KeyPathIndex(index));
        self
    }

    /// Returns an iterator over the indices.
    pub fn iter(&self) -> slice::Iter<'_, KeyPathIndex> {
        self.path.iter()
    }
}

impl fmt::Display for KeyPath {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let path = self.iter().fold("m".to_string(), |mut path, index| {
            path.push_str(format!("/{}", index).as_ref());
            path
        });

        write!(f, "{}", path)
    }
}

#[inline]
fn get_chain_code_and_secret(
    seed: &[u8],
    mut hmac512: Hmac<sha2::Sha512>,
) -> Result<([u8; 32], SecretKey), KeyDerivationError> {
    hmac512.input(seed);
    let i = hmac512.result().code();
    let (il, ir) = i.split_at(32);
    let chain_code: [u8; 32] = {
        let mut array: [u8; 32] = [0; 32];
        array.copy_from_slice(&ir);
        array
    };
    let secret_key = SecretKey::from_slice(&il).map_err(KeyDerivationError::Secp256k1Error)?;

    Ok((chain_code, secret_key))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mnemonic as bip39;

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
        let result = MasterKeyGen::new(seed).with_key(b"Bitcoin seed").generate();

        assert!(result.is_ok());
    }

    #[test]
    fn test_seed() {
        let phrase = "panda eyebrow bullet gorilla call smoke muffin taste mesh discover soft ostrich alcohol speed nation flash devote level hobby quick inner drive ghost inside";

        let mnemonic = bip39::Mnemonic::from_phrase(phrase.into()).unwrap();

        let seed = bip39::Mnemonic::seed(&mnemonic, &"".into());

        // Expected seed calculated in https://iancoleman.io/bip39/
        let expected_seed = [
            62, 6, 109, 125, 238, 45, 191, 143, 205, 63, 226, 64, 163, 151, 86, 88, 202, 17, 138,
            143, 111, 76, 168, 28, 249, 145, 4, 148, 70, 4, 176, 90, 80, 144, 167, 157, 153, 229,
            69, 112, 75, 145, 76, 160, 57, 127, 237, 184, 47, 208, 15, 214, 167, 32, 152, 112, 55,
            9, 200, 145, 160, 101, 238, 73,
        ];

        assert_eq!(&expected_seed[..], seed.as_bytes());
    }

    #[test]
    fn test_secret_key() {
        let seed = [
            62, 6, 109, 125, 238, 45, 191, 143, 205, 63, 226, 64, 163, 151, 86, 88, 202, 17, 138,
            143, 111, 76, 168, 28, 249, 145, 4, 148, 70, 4, 176, 90, 80, 144, 167, 157, 153, 229,
            69, 112, 75, 145, 76, 160, 57, 127, 237, 184, 47, 208, 15, 214, 167, 32, 152, 112, 55,
            9, 200, 145, 160, 101, 238, 73,
        ];

        let master_key = MasterKeyGen::new(&seed[..]).generate().unwrap();

        let expected_secret_key = [
            79, 67, 227, 208, 107, 229, 51, 169, 104, 61, 121, 142, 8, 143, 75, 74, 235, 179, 67,
            213, 108, 252, 255, 16, 32, 162, 57, 21, 195, 162, 115, 128,
        ];
        assert_eq!(expected_secret_key, &master_key.secret_key[..]);
    }

    #[test]
    fn test_key_derivation() {
        let seed = [
            62, 6, 109, 125, 238, 45, 191, 143, 205, 63, 226, 64, 163, 151, 86, 88, 202, 17, 138,
            143, 111, 76, 168, 28, 249, 145, 4, 148, 70, 4, 176, 90, 80, 144, 167, 157, 153, 229,
            69, 112, 75, 145, 76, 160, 57, 127, 237, 184, 47, 208, 15, 214, 167, 32, 152, 112, 55,
            9, 200, 145, 160, 101, 238, 73,
        ];

        let extended_sk = MasterKeyGen::new(&seed[..]).generate().unwrap();
        let path = KeyPath::new()
            .hardened(44) // purpose: BIP-44
            .hardened(0) // coin_type: Bitcoin
            .hardened(0) // account: hardened 0
            .index(0) // change: 0
            .index(0); // address: 0

        let account = extended_sk.derive(&path).unwrap();

        let expected_account = [
            137, 174, 230, 121, 4, 190, 53, 238, 47, 181, 52, 226, 109, 68, 153, 170, 112, 150, 84,
            84, 26, 177, 194, 157, 76, 80, 136, 25, 6, 79, 247, 43,
        ];

        assert_eq!(
            expected_account,
            &account.secret()[..],
            "Secret key is invalid"
        );
    }
}
