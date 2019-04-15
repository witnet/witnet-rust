//! # BIP32 Key generation and derivation
//!
//! Example
//!
//! ```
//! # use witnet_crypto::{key, mnemonic};
//! let passphrase = "";
//! let seed = mnemonic::MnemonicGen::new().generate().seed(passphrase);
//! let ext_key = key::MasterKeyGen::new(seed).generate();
//! ```

use failure::Fail;
use hmac::{Hmac, Mac};
use secp256k1::{PublicKey, Secp256k1, SecretKey};
use sha2;

const HARDENED_BIT: u32 = 1 << 31;

/// Default HMAC key used when generating a Master Key with
/// [generate_master](generate_master)
pub static DEFAULT_HMAC_KEY: &[u8] = b"Bitcoin seed";

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
    /// Create a new master key generator
    pub fn new(seed: S) -> Self {
        Self {
            key: DEFAULT_HMAC_KEY,
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
pub struct ExtendedSK {
    /// Secret key
    pub secret_key: SK,
    /// Chain code
    pub chain_code: [u8; 32],
}

/// A child number for a derived key
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub struct ChildNumber(u32);

impl ChildNumber {
    /// check if a child is hardened
    pub fn is_hardened(self) -> bool {
        self.0 & HARDENED_BIT == HARDENED_BIT
    }

    /// Serialize a child
    pub fn to_bytes(self) -> [u8; 4] {
        self.0.to_be_bytes()
    }
}

impl ExtendedSK {
    /// Try to derive an extended private key from a given path
    pub fn derive(&self, path: Vec<ChildNumber>) -> Result<ExtendedSK, KeyDerivationError> {
        let mut extended_sk = self.clone();
        for child in path {
            extended_sk = extended_sk.child(child)?
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
    pub fn child(&self, child: ChildNumber) -> Result<ExtendedSK, KeyDerivationError> {
        let mut hmac512: Hmac<sha2::Sha512> =
            Hmac::new_varkey(&self.chain_code).map_err(|_| KeyDerivationError::InvalidKeyLength)?;
        if child.is_hardened() {
            hmac512.input(&[0]);
            hmac512.input(&self.secret_key[..]);
        } else {
            hmac512.input(
                &PublicKey::from_secret_key(&secp256k1::Secp256k1::new(), &self.secret_key)
                    .serialize(),
            );
        }

        let (chain_code, mut secret_key) = get_chain_code_and_secret(&child.to_bytes(), hmac512)?;

        secret_key
            .add_assign(&self.secret_key[..])
            .map_err(KeyDerivationError::Secp256k1Error)?;

        Ok(ExtendedSK {
            secret_key,
            chain_code,
        })
    }
}

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

        let mnemonic =
            bip39::Mnemonic::from_phrase(phrase.to_string(), bip39::Lang::English).unwrap();

        let seed = bip39::Mnemonic::seed(&mnemonic, "");

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

        let account = extended_sk
            .derive(vec![
                ChildNumber(0x8000_002c), // purpose: BIP-44
                ChildNumber(0x8000_0000), // coin_type: Bitcoin
                ChildNumber(0x8000_0000), // account: hardened 0
                ChildNumber(0),           // change: 0
                ChildNumber(0),           // address: 0
            ])
            .unwrap();

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
