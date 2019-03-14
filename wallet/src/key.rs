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
use secp256k1::{PublicKey, SecretKey};
use sha2;

const HARDENED_BIT: u32 = 1 << 31;

/// Default HMAC key used when generating a Master Key with
/// [generate_master](generate_master)
pub static DEFAULT_HMAC_KEY: &[u8] = b"Bitcoin seed";

/// secp256k1 Secret Key with Chain Code
// pub type ExtendedSK = ExtendedKey<SecretKey>;

/// secp256k1 Public Key with Chain Code
// pub type ExtendedPK = ExtendedKey<secp256k1::PublicKey>;

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

        let key_bytes = self.key.as_ref();
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

#[derive(Debug, PartialEq, Fail)]
pub enum KeyDerivationError {
    /// Invalid hmac key length
    #[fail(display = "The length of the hmac key is invalid")]
    InvalidKeyLength,
    /// Invalid seed length
    #[fail(display = "The length of the seed is invalid, must be between 128/256 bits")]
    InvalidSeedLength,

    #[fail(display = "Error in secp256k1 crate")]
    Secp256k1Error,
}

/// Extended Key is just a Key with a Chain Code
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct ExtendedSK {
    pub secret_key: SecretKey,
    pub chain_code: [u8; 32],
}

/// A child number for a derived key
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub struct ChildNumber(u32);

impl ChildNumber {
    pub fn is_hardened(&self) -> bool {
        self.0 & HARDENED_BIT == HARDENED_BIT
    }

    pub fn is_normal(&self) -> bool {
        self.0 & HARDENED_BIT == 0
    }

    pub fn to_bytes(&self) -> [u8; 4] {
        self.0.to_be_bytes()
    }
}

impl ExtendedSK {
    /// Try to derive an extended private key from a given path
    pub fn derive(seed: &[u8], path: Vec<ChildNumber>) -> Result<ExtendedSK, KeyDerivationError> {
        let key_bytes = DEFAULT_HMAC_KEY.as_ref();
        let mut hmac512 = Hmac::<sha2::Sha512>::new_varkey(key_bytes)
            .map_err(|_| KeyDerivationError::InvalidKeyLength)?;
        hmac512.input(seed);

        let i = hmac512.result().code();
        let (il, ir) = i.split_at(32);
        let chain_code: [u8; 32] = {
            let mut array: [u8; 32] = [0; 32];
            array.copy_from_slice(&ir);
            array
        };
        let secret_key =
            SecretKey::from_slice(&il).map_err(|_| KeyDerivationError::Secp256k1Error)?;

        let mut extended_sk = ExtendedSK {
            secret_key,
            chain_code,
        };

        for child in path {
            extended_sk = extended_sk.child(child)?
        }

        Ok(extended_sk)
    }

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
        hmac512.input(&child.to_bytes());
        let i = hmac512.result().code();
        let (il, ir) = i.split_at(32);
        let chain_code: [u8; 32] = {
            let mut array: [u8; 32] = [0; 32];
            array.copy_from_slice(&ir);
            array
        };

        let mut secret_key =
            SecretKey::from_slice(&il).map_err(|_| KeyDerivationError::Secp256k1Error)?;
        secret_key
            .add_assign(&self.secret_key[..])
            .map_err(|_| KeyDerivationError::Secp256k1Error)?;

        Ok(ExtendedSK {
            secret_key,
            chain_code,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::super::mnemonic as bip39;
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
        let result = MasterKeyGen::new(seed).with_key(b"Bitcoin seed").generate();

        assert!(result.is_ok());
    }

    #[test]
    fn test_key_derivation() {
        let phrase = "panda eyebrow bullet gorilla call smoke muffin taste mesh discover soft ostrich alcohol speed nation flash devote level hobby quick inner drive ghost inside";
        let expected_secret_key = b"\xff\x1e\x68\xeb\x7b\xf2\xf4\x86\x51\xc4\x7e\xf0\x17\x7e\xb8\x15\x85\x73\x22\x25\x7c\x58\x94\xbb\x4c\xfd\x11\x76\xc9\x98\x93\x14";

        let mnemonic =
            bip39::Mnemonic::from_phrase(phrase.to_string(), bip39::Lang::English).unwrap();
        let seed = bip39::Mnemonic::seed(&mnemonic, "");
        let account = ExtendedSK::derive(
            seed.as_bytes(),
            vec![
                ChildNumber(2147483692),
                ChildNumber(2147483708),
                ChildNumber(2147483648),
                ChildNumber(0),
                ChildNumber(0),
            ],
        )
        .unwrap();

        assert_eq!(
            expected_secret_key,
            &account.secret(),
            "Secret key is invalid"
        );
    }

    #[test]
    fn test_seed() {
        let phrase = "panda eyebrow bullet gorilla call smoke muffin taste mesh discover soft ostrich alcohol speed nation flash devote level hobby quick inner drive ghost inside";

        let mnemonic =
            bip39::Mnemonic::from_phrase(phrase.to_string(), bip39::Lang::English).unwrap();

        let seed = bip39::Mnemonic::seed(&mnemonic, "");

        let expected_seed = [
            62, 6, 109, 125, 238, 45, 191, 143, 205, 63, 226, 64, 163, 151, 86, 88, 202, 17, 138,
            143, 111, 76, 168, 28, 249, 145, 4, 148, 70, 4, 176, 90, 80, 144, 167, 157, 153, 229,
            69, 112, 75, 145, 76, 160, 57, 127, 237, 184, 47, 208, 15, 214, 167, 32, 152, 112, 55,
            9, 200, 145, 160, 101, 238, 73,
        ];

        assert_eq!(&expected_seed[..], seed.as_bytes());
    }
}
