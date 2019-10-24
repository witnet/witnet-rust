//! Cipher
use crypto::{
    aes,
    blockmodes::PkcsPadding,
    buffer::{self, ReadBuffer, WriteBuffer},
    symmetriccipher,
};
use failure::Fail;
use rand::{rngs::OsRng, RngCore};

/// Error that can be raised when encrypting/decrypting
#[derive(Debug, Fail)]
pub enum Error {
    /// Wrapper for cipher errors
    #[fail(display = "Encryption/decryption error")]
    Cipher(symmetriccipher::SymmetricCipherError),
    /// Wrapper for random generation errors
    #[fail(display = "Random generation error")]
    Rng(rand::Error),
}

/// Encrypt data with AES CBC using the supplied secret
pub fn encrypt_aes_cbc(secret: &[u8], data: &[u8], iv: &[u8]) -> Result<Vec<u8>, Error> {
    let mut encryptor = aes::cbc_encryptor(aes::KeySize::KeySize256, secret, iv, PkcsPadding);
    let mut final_result = Vec::<u8>::new();
    let mut read_buffer = buffer::RefReadBuffer::new(data);
    let mut buffer = [0; 4096];
    let mut write_buffer = buffer::RefWriteBuffer::new(&mut buffer);

    loop {
        let result = encryptor
            .encrypt(&mut read_buffer, &mut write_buffer, true)
            .map_err(Error::Cipher)?;

        final_result.extend(
            write_buffer
                .take_read_buffer()
                .take_remaining()
                .iter()
                .cloned(),
        );

        match result {
            buffer::BufferResult::BufferUnderflow => break,
            buffer::BufferResult::BufferOverflow => (),
        }
    }

    Ok(final_result)
}

/// Decrypt data with AES CBC using the supplied secret
pub fn decrypt_aes_cbc(secret: &[u8], data: &[u8], iv: &[u8]) -> Result<Vec<u8>, Error> {
    let mut decryptor = aes::cbc_decryptor(aes::KeySize::KeySize256, secret, iv, PkcsPadding);
    let mut final_result = Vec::<u8>::new();
    let mut read_buffer = buffer::RefReadBuffer::new(data);
    let mut buffer = [0; 4096];
    let mut write_buffer = buffer::RefWriteBuffer::new(&mut buffer);

    loop {
        let result = decryptor
            .decrypt(&mut read_buffer, &mut write_buffer, true)
            .map_err(Error::Cipher)?;

        final_result.extend(
            write_buffer
                .take_read_buffer()
                .take_remaining()
                .iter()
                .cloned(),
        );

        match result {
            buffer::BufferResult::BufferUnderflow => break,
            buffer::BufferResult::BufferOverflow => (),
        }
    }

    Ok(final_result)
}

/// Generate a random initialization vector of the given size in bytes
pub fn generate_random(size: usize) -> Result<Vec<u8>, Error> {
    let mut iv = vec![0u8; size];
    OsRng.fill_bytes(&mut iv);

    Ok(iv)
}
