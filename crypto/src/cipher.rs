//! Cipher
use aes::Aes256;
//use aes_soft as aes;
use block_modes::{block_padding::Pkcs7, BlockMode, Cbc};
use failure::Fail;
use rand::{rngs::OsRng, RngCore};

// create an alias for convenience
type Aes256Cbc = Cbc<Aes256, Pkcs7>;

/// Error that can be raised when encrypting/decrypting
#[derive(Debug, Fail)]
pub enum Error {
    /// Block mode error
    #[fail(display = "{}", _0)]
    BlockModeError(block_modes::BlockModeError),
    /// Invalid key IV length
    #[fail(display = "{}", _0)]
    InvalidKeyIvLength(block_modes::InvalidKeyIvLength),
    /// Wrapper for random generation errors
    #[fail(display = "Random generation error")]
    Rng(rand::Error),
}

/// Encrypt data with AES CBC using the supplied secret
pub fn encrypt_aes_cbc(secret: &[u8], plaintext: &[u8], iv: &[u8]) -> Result<Vec<u8>, Error> {
    let cipher = Aes256Cbc::new_var(&secret, &iv).map_err(Error::InvalidKeyIvLength)?;
    let ciphertext = cipher.encrypt_vec(plaintext);

    Ok(ciphertext)
}

/// Decrypt data with AES CBC using the supplied secret
pub fn decrypt_aes_cbc(secret: &[u8], ciphertext: &[u8], iv: &[u8]) -> Result<Vec<u8>, Error> {
    let cipher = Aes256Cbc::new_var(&secret, &iv).map_err(Error::InvalidKeyIvLength)?;
    let plaintext = cipher
        .decrypt_vec(ciphertext)
        .map_err(Error::BlockModeError)?;

    Ok(plaintext)
}

/// Generate a random initialization vector of the given size in bytes
pub fn generate_random(size: usize) -> Result<Vec<u8>, Error> {
    let mut iv = vec![0u8; size];
    OsRng.fill_bytes(&mut iv);

    Ok(iv)
}
