//! Various hash functions

use crypto::digest::Digest;
use crypto::sha2;

/// Secure hashing algorithm v2
#[derive(Copy, Clone, Debug, PartialEq, Hash)]
pub struct Sha256(pub [u8; 32]);

/// Calculate the SHA256 hash
pub fn calculate_sha256(bytes: &[u8]) -> Sha256 {
    let mut hasher = sha2::Sha256::new();
    hasher.input(&bytes);
    let mut hash = [0; 32];
    hasher.result(&mut hash);
    Sha256(hash)
}
