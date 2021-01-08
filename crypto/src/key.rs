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
use std::{
    convert::TryFrom,
    fmt,
    io::{self, Read as _, Write as _},
    slice,
};

use bech32::{FromBase32, ToBase32 as _};
use byteorder::{BigEndian, ReadBytesExt as _};
use failure::Fail;
use hmac::{Hmac, Mac};
use secp256k1::{PublicKey, Secp256k1, SecretKey, SignOnly, Signing, VerifyOnly};
#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

use witnet_protected::Protected;

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

/// Error type for errors ocurring when serializing an extended secret
/// key.
#[derive(Debug, Fail)]
pub enum KeyError {
    /// Error that might happen when serializing the key.
    #[fail(display = "Serialization error: {}", _0)]
    Serialization(#[cause] failure::Error),
    /// Error that might happen when decoding the key.
    #[fail(display = "Decoding error: {}", _0)]
    Deserialization(#[cause] failure::Error),
}

impl KeyError {
    /// Turn an std::error::Error into a serialization error.
    pub fn serialization_err<E>(err: E) -> Self
    where
        E: std::error::Error + Sync + Send + 'static,
    {
        Self::Serialization(failure::Error::from_boxed_compat(Box::new(err)))
    }

    /// Turn an std::error::Error into a deserialization error.
    pub fn deserialization_err<E>(err: E) -> Self
    where
        E: std::error::Error + Sync + Send + 'static,
    {
        Self::Deserialization(failure::Error::from_boxed_compat(Box::new(err)))
    }
}

impl From<io::Error> for KeyError {
    fn from(err: io::Error) -> Self {
        Self::serialization_err(err)
    }
}

impl From<hex::FromHexError> for KeyError {
    fn from(err: hex::FromHexError) -> Self {
        Self::deserialization_err(err)
    }
}

impl From<secp256k1::Error> for KeyError {
    fn from(err: secp256k1::Error) -> Self {
        Self::deserialization_err(err)
    }
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

        // Seed length must be between 16 and 64 bytes (128 and 512 bits)
        if !(16..=64).contains(&seed_len) {
            return Err(MasterKeyGenError::InvalidSeedLength);
        }

        let key_bytes = self.key;
        let mut mac = Hmac::<sha2::Sha512>::new_varkey(key_bytes)
            .map_err(|_| MasterKeyGenError::InvalidKeyLength)?;
        mac.input(seed_bytes);
        let result = mac.result().code();
        let (sk_bytes, chain_code_bytes) = result.split_at(32);

        // secret/chain_code computation might panic if length returned by hmac is wrong
        let secret_key = SecretKey::from_slice(sk_bytes).expect("Secret Key length error");
        let chain_code = Protected::from(chain_code_bytes);

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
    #[fail(display = "The length of the seed is invalid, must be between 128/512 bits")]
    InvalidSeedLength,
    /// Secp256k1 internal error
    #[fail(display = "Error in secp256k1 crate")]
    Secp256k1Error(secp256k1::Error),
}

/// Secret Key
pub type SK = SecretKey;

/// Public Key
pub type PK = PublicKey;

/// The secp256k1 engine, used to execute all signature operations.
///
/// `Engine::new()`: all capabilities
/// `Engine::signing_only()`: only be used for signing
/// `Engine::verification_only()`: only be used for verification
pub type Engine<C> = Secp256k1<C>;

/// Secp256k1 engine that can only be used for signing.
pub type SignEngine = Secp256k1<SignOnly>;

/// Secp256k1 engine that can only be used for verifying.
pub type VerifyEngine = Secp256k1<VerifyOnly>;

/// Secp256k1 engine that can be used for signing and for verifying.
pub type CryptoEngine = Secp256k1<secp256k1::All>;

/// Extended Key is just a Key with a Chain Code
#[derive(Clone, PartialEq, Eq, Debug)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct ExtendedSK {
    /// Secret key
    pub secret_key: SK,
    /// Chain code
    chain_code: Protected,
}

impl ExtendedSK {
    /// Create a new extended secret key which is the combination of a secret key and a chain code.
    pub fn new(secret_key: SK, chain_code: Protected) -> Self {
        Self {
            secret_key,
            chain_code,
        }
    }

    /// Create a new extended secret key from the given slip32-encoded string.
    pub fn from_slip32(slip32: &str) -> Result<(Self, KeyPath), KeyError> {
        let (hrp, data) = bech32::decode(slip32).map_err(KeyError::deserialization_err)?;

        if hrp.as_str() != "xprv" {
            return Err(KeyError::Deserialization(failure::format_err!(
                "prefix is not \"xprv\""
            )));
        }

        let bytes: Vec<u8> =
            FromBase32::from_base32(&data).map_err(KeyError::deserialization_err)?;
        let actual_len = bytes.len();
        let mut cursor = io::Cursor::new(bytes);
        let depth = cursor.read_u8()? as usize;
        let len = depth * 4;
        let expected_len = len + 66; // 66 = 1 (depth) 32 (chain code) + 33 (private key)

        if expected_len != actual_len {
            return Err(KeyError::Deserialization(failure::format_err!(
                "invalid data length, expected: {}, got: {}",
                expected_len,
                actual_len
            )));
        }

        let mut path = vec![0; depth];
        cursor.read_u32_into::<BigEndian>(path.as_mut())?;

        let mut chain_code = Protected::new(vec![0; 32]);
        cursor.read_exact(chain_code.as_mut())?;

        let secret_prefix = cursor.read_u8()?;
        debug_assert!(secret_prefix == 0);

        let mut secret = Protected::new(vec![0; 32]);
        cursor.read_exact(secret.as_mut())?;

        let sk = SK::from_slice(secret.as_ref())?;
        let extended_sk = Self::new(sk, chain_code);

        Ok((extended_sk, path.into()))
    }

    /// Serialize the key following the SLIP32 spec.
    ///
    /// See https://github.com/satoshilabs/slips/blob/master/slip-0032.md#serialization-format
    pub fn to_slip32(&self, path: &KeyPath) -> Result<String, KeyError> {
        let depth = path.depth();
        let depth = u8::try_from(depth).map_err(|_| {
            KeyError::Serialization(failure::format_err!(
                "path depth '{}' is greater than 255",
                depth,
            ))
        })?;

        let capacity = 1     // 1 byte for depth
            + 4 * depth      // 4 * depth bytes for path
            + 32             // 32 bytes for chain code
            + 33             // 33 bytes for 0x00 || private key
            ;
        let mut bytes = Protected::new(vec![0; usize::from(capacity)]);
        let mut slice = bytes.as_mut();

        slice.write_all(&[depth])?;
        for index in path.iter() {
            slice.write_all(&index.as_ref().to_be_bytes())?;
        }
        slice.write_all(self.chain_code.as_ref())?;
        slice.write_all(&[0])?;
        slice.write_all(self.secret().as_ref())?;

        let encoded = bech32::encode("xprv", bytes.as_ref().to_base32())
            .map_err(KeyError::serialization_err)?;

        Ok(encoded)
    }

    /// Get the secret and chain code concatenated
    pub fn concat(&self) -> Protected {
        let mut bytes = Vec::from(&self.secret_key[..]);
        bytes.extend_from_slice(&self.chain_code);

        Protected::from(bytes)
    }

    /// Get the secret key part.
    pub fn secret(&self) -> Protected {
        Protected::from(&self.secret_key[..])
    }

    /// Get the chain code part.
    pub fn chain_code(&self) -> Protected {
        self.chain_code.clone()
    }

    /// Try to derive an extended private key from a given path
    pub fn derive<C: Signing>(
        &self,
        engine: &Engine<C>,
        path: &KeyPath,
    ) -> Result<ExtendedSK, KeyDerivationError> {
        let mut extended_sk = self.clone();
        for index in path.iter() {
            extended_sk = extended_sk.child(engine, index)?
        }

        Ok(extended_sk)
    }

    /// Try to get a private child key from parent
    pub fn child<C: Signing>(
        &self,
        engine: &Engine<C>,
        index: &KeyPathIndex,
    ) -> Result<ExtendedSK, KeyDerivationError> {
        let mut hmac512: Hmac<sha2::Sha512> =
            Hmac::new_varkey(&self.chain_code).map_err(|_| KeyDerivationError::InvalidKeyLength)?;
        let index_bytes = index.as_ref().to_be_bytes();

        if index.is_hardened() {
            hmac512.input(&[0]); // BIP-32 padding that makes key 33 bytes long
            hmac512.input(&self.secret_key[..]);
        } else {
            hmac512.input(&PublicKey::from_secret_key(engine, &self.secret_key).serialize());
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

impl Into<SK> for ExtendedSK {
    fn into(self) -> SK {
        self.secret_key
    }
}

/// Extended Public Key.
///
/// It can be used to derive other HD-Wallets public keys.
#[derive(Clone, PartialEq, Eq, Debug)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct ExtendedPK {
    /// Public key
    pub key: PK,
    /// Chain code
    pub chain_code: Protected,
}

impl ExtendedPK {
    /// Derive the public key from a private key.
    pub fn from_secret_key<C: Signing>(engine: &Engine<C>, key: &ExtendedSK) -> Self {
        let ExtendedSK {
            secret_key,
            chain_code,
        } = key;
        let key = PublicKey::from_secret_key(engine, secret_key);
        Self {
            key,
            chain_code: chain_code.clone(),
        }
    }
}

impl Into<PK> for ExtendedPK {
    fn into(self) -> PK {
        self.key
    }
}

/// Represents an index inside a key derivation path.
/// See BIP-32 spec for more information.
#[derive(Debug, PartialEq, Eq, Clone)]
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
#[derive(Debug, Default, PartialEq, Eq, Clone)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct KeyPath {
    path: Vec<KeyPathIndex>,
}

impl KeyPath {
    const HARDENED_KEY_INDEX: u32 = 0x8000_0000;

    /// Add a hardened-index index to the current path.
    ///
    /// Example
    /// ```
    /// # use witnet_crypto::key::KeyPath;
    /// let path = KeyPath::default().hardened(3).hardened(4);
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
    /// let path = KeyPath::default().index(3).index(4);
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

    /// Return the number of levels (depth) this path has.
    pub fn depth(&self) -> usize {
        self.path.len()
    }

    /// Returns true if this key path corresponds to a master key.
    pub fn is_master(&self) -> bool {
        self.depth() == 0
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

impl From<Vec<u32>> for KeyPath {
    fn from(path: Vec<u32>) -> Self {
        Self {
            path: path.into_iter().map(KeyPathIndex).collect(),
        }
    }
}

#[inline]
fn get_chain_code_and_secret(
    seed: &[u8],
    mut hmac512: Hmac<sha2::Sha512>,
) -> Result<(Protected, SecretKey), KeyDerivationError> {
    hmac512.input(seed);
    let i = hmac512.result().code();
    let (il, ir) = i.split_at(32);
    let chain_code = Protected::from(ir);
    let secret_key = SecretKey::from_slice(&il).map_err(KeyDerivationError::Secp256k1Error)?;

    Ok((chain_code, secret_key))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mnemonic as bip39;
    use crate::test_vectors::slip32_vectors;

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
        let path = KeyPath::default()
            .hardened(44) // purpose: BIP-44
            .hardened(0) // coin_type: Bitcoin
            .hardened(0) // account: hardened 0
            .index(0) // change: 0
            .index(0); // address: 0
        let engine = SignEngine::signing_only();
        let account = extended_sk.derive(&engine, &path).unwrap();

        let expected_account = [
            137, 174, 230, 121, 4, 190, 53, 238, 47, 181, 52, 226, 109, 68, 153, 170, 112, 150, 84,
            84, 26, 177, 194, 157, 76, 80, 136, 25, 6, 79, 247, 43,
        ];

        assert_eq!(
            &expected_account,
            account.secret().as_ref(),
            "Secret key is invalid"
        );
    }

    #[test]
    fn test_slip32() {
        let phrase = "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon about";
        let mnemonic = bip39::Mnemonic::from_phrase(phrase.into()).unwrap();
        let seed = mnemonic.seed(&"".into());
        let master_key = MasterKeyGen::new(&seed).generate().unwrap();
        let engine = Secp256k1::signing_only();

        for (expected, keypath) in slip32_vectors() {
            let key = master_key.derive(&engine, &keypath).unwrap();
            let xprv = key.to_slip32(&keypath).unwrap();

            assert_eq!(expected, xprv);

            let (recovered_key, path) = ExtendedSK::from_slip32(&xprv).unwrap();

            assert_eq!(keypath, path);
            assert_eq!(key, recovered_key);
        }
    }
}
