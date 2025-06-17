//! Cipher
use aes::Aes256;
use block_modes::{BlockMode, Cbc, block_padding::Pkcs7};
use rand::{Error as RandError, RngCore, rngs::OsRng};
use thiserror::Error;

type Aes256Cbc = Cbc<Aes256, Pkcs7>;

/// Error that can be raised when encrypting/decrypting
#[derive(Debug, Error)]
pub enum Error {
    /// Block mode error
    #[error("{0}")]
    BlockModeError(block_modes::BlockModeError),
    /// Invalid key IV length
    #[error("{0}")]
    InvalidKeyIvLength(block_modes::InvalidKeyIvLength),
    /// Wrapper for random generation errors
    #[error("Randomness generation error: {0}")]
    Rng(RandError),
}

/// Encrypt data with AES CBC using the supplied secret
pub fn encrypt_aes_cbc(secret: &[u8], plaintext: &[u8], iv: &[u8]) -> Result<Vec<u8>, Error> {
    let cipher = Aes256Cbc::new_from_slices(secret, iv).map_err(Error::InvalidKeyIvLength)?;
    let ciphertext = cipher.encrypt_vec(plaintext);

    Ok(ciphertext)
}

/// Decrypt data with AES CBC using the supplied secret
pub fn decrypt_aes_cbc(secret: &[u8], ciphertext: &[u8], iv: &[u8]) -> Result<Vec<u8>, Error> {
    let cipher = Aes256Cbc::new_from_slices(secret, iv).map_err(Error::InvalidKeyIvLength)?;
    let plaintext = cipher
        .decrypt_vec(ciphertext)
        .map_err(Error::BlockModeError)?;

    Ok(plaintext)
}

/// Generate a random initialization vector of the given size in bytes
pub fn generate_random(size: usize) -> Result<Vec<u8>, Error> {
    let mut iv = vec![0u8; size];
    OsRng.try_fill_bytes(&mut iv).map_err(Error::Rng)?;

    Ok(iv)
}
