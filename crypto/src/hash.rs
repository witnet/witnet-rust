//! Various hash functions

use witnet_data_structures::chain::Hash;

use crypto::digest::Digest;
use crypto::sha2::Sha256;

/// Calculate the SHA256 hash
pub fn calculate_sha256(bytes: &[u8]) -> Hash {
    let mut hasher = Sha256::new();
    hasher.input(&bytes);
    let mut hash = [0; 32];
    hasher.result(&mut hash);
    Hash::SHA256(hash)
}
