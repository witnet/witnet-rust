//! Various hash functions

use digest::Digest;
#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};
use sha2;

/// Enumeration of hash-function names
#[derive(Clone, PartialEq, Eq, Debug)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub enum HashFunction {
    /// SHA-256 hash function
    Sha256,
}

/// Secure hashing algorithm v2
#[derive(Copy, Clone, Debug, PartialEq, Hash)]
pub struct Sha256(pub [u8; 32]);

/// Value of an empty hash
pub static EMPTY_SHA256: Sha256 = Sha256([
    227, 176, 196, 66, 152, 252, 28, 20, 154, 251, 244, 200, 153, 111, 185, 36, 39, 174, 65, 228,
    100, 155, 147, 76, 164, 149, 153, 27, 120, 82, 184, 85,
]);

impl AsRef<[u8]> for Sha256 {
    fn as_ref(&self) -> &[u8] {
        &self.0
    }
}

/// Calculate the SHA256 hash
pub fn calculate_sha256(bytes: &[u8]) -> Sha256 {
    let mut hasher = sha2::Sha256::new();
    hasher.input(&bytes);
    let mut hash = [0; 32];
    hash.copy_from_slice(&hasher.result());
    Sha256(hash)
}
