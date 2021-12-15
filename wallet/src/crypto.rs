use failure::Fail;

use witnet_crypto::{
    cipher,
    hash::HashFunction,
    key::{ExtendedSK, KeyError, MasterKeyGen, MasterKeyGenError},
    pbkdf2::pbkdf2_sha256,
};

use crate::types;

const IV_LENGTH: usize = 16;
const SALT_LENGTH: usize = 32;
const HASH_ITER_COUNT: u32 = 10_000;

/// Generation of master key errors
#[derive(Debug, Fail)]
pub enum Error {
    /// The generation of the master key failed.
    #[fail(display = "Generation of key failed: {}", _0)]
    Generation(#[cause] MasterKeyGenError),
    /// The deserialization of the master key failed.
    #[fail(display = "Deserialization of key failed: {}", _0)]
    Deserialization(#[cause] KeyError),
    /// The key path of the slip32-serialized key is not of a master key.
    #[fail(
        display = "Imported key is not a master key according to its path: {}",
        _0
    )]
    InvalidKeyPath(String),
    #[fail(display = "The AES encryption/decryption failed: {}", _0)]
    Aes(#[cause] cipher::Error),
}

/// Result type for cryptographic operations that can fail.
pub type Result<T> = std::result::Result<T, Error>;

/// Generate an HD-Wallet master extended key from a seed.
///
/// The seed can be treated as either, a mnemonic phrase or an xprv
pub fn gen_master_key(seed: &str, salt: &[u8], source: &types::SeedSource) -> Result<ExtendedSK> {
    let key = match source {
        types::SeedSource::Mnemonics(mnemonic) => {
            let seed = mnemonic.seed_ref(seed);

            MasterKeyGen::new(seed)
                .with_key(salt)
                .generate()
                .map_err(Error::Generation)?
        }
        types::SeedSource::Xprv(slip32) => {
            let (key, path) =
                ExtendedSK::from_slip32(slip32.as_ref()).map_err(Error::Deserialization)?;
            if !path.is_master() {
                return Err(Error::InvalidKeyPath(format!("{}", path)));
            }

            key
        }
        _ => return Err(Error::Generation(MasterKeyGenError::InvalidKeyLength)),
    };

    Ok(key)
}

/// Generate an encryption key using pbkdf2.
pub fn key_from_password(password: &[u8], salt: &[u8], iterations: u32) -> types::Secret {
    pbkdf2_sha256(password, salt, iterations)
}

/// Generate a cryptographic wallet id.
pub fn gen_wallet_id(
    hash: &HashFunction,
    key: &ExtendedSK,
    salt: &[u8],
    iterations: u32,
) -> String {
    match hash {
        HashFunction::Sha256 => {
            let password = key.concat();
            let id_bytes = pbkdf2_sha256(password.as_ref(), salt, iterations);

            hex::encode(id_bytes)
        }
    }
}

/// Generate a cryptographic session id.
pub fn gen_session_id<Rng>(
    rng: &mut Rng,
    hash: &HashFunction,
    key: &[u8],
    salt: &[u8],
    iterations: u32,
) -> String
where
    Rng: rand::Rng + rand::CryptoRng,
{
    match hash {
        HashFunction::Sha256 => {
            let rand_bytes: [u8; 32] = rng.gen();
            let password = [key, salt, rand_bytes.as_ref()].concat();
            let id_bytes = pbkdf2_sha256(&password, salt, iterations);

            hex::encode(id_bytes)
        }
    }
}

/// Generate a cryptographic salt.
pub fn salt<Rand>(rng: &mut Rand, len: usize) -> Vec<u8>
where
    Rand: rand::Rng + rand::CryptoRng,
{
    let mut bytes = vec![0u8; len];

    rng.fill_bytes(&mut bytes);

    bytes
}

/// AES-CBC encryption of a given u8 slice with the provided password. Returns IV|SALT|CIPHERTEXT
pub fn encrypt_cbc(value: &[u8], password: &[u8]) -> Result<Vec<u8>> {
    let iv = cipher::generate_random(IV_LENGTH).map_err(Error::Aes)?;
    let salt = cipher::generate_random(SALT_LENGTH).map_err(Error::Aes)?;
    let secret = pbkdf2_sha256(password, &salt, HASH_ITER_COUNT);
    let ciphertext = cipher::encrypt_aes_cbc(&secret, value, iv.as_ref()).map_err(Error::Aes)?;
    let mut final_value = iv;
    final_value.extend(salt);
    final_value.extend(ciphertext);

    Ok(final_value)
}

/// AES-CBC decryption of a given u8 given as IV|SALT|CIPHERTEXT slice with the provided password.
pub fn decrypt_cbc(ciphertext: &[u8], password: &[u8]) -> Result<Vec<u8>> {
    let mut iv = ciphertext.to_vec();
    let mut salt = iv.split_off(IV_LENGTH);
    let true_ciphertext = salt.split_off(SALT_LENGTH);
    let secret = pbkdf2_sha256(password, &salt, HASH_ITER_COUNT);
    let plaintext = cipher::decrypt_aes_cbc(&secret, &true_ciphertext, &iv).map_err(Error::Aes)?;

    Ok(plaintext)
}
