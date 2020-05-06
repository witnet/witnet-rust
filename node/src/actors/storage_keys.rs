/// Database key that stores the master secret key
pub const MASTER_KEY: &[u8] = b"master_key";

/// Database key that stores the BN256 secret key
pub const BN256_SECRET_KEY: &[u8] = b"bn256_secret_key";

/// Function to create a chain state key for the storage
#[inline]
pub fn chain_state_key(magic: u16) -> String {
    format!("chain-{}-key", magic)
}

/// Function to create a peers key for the storage
#[inline]
pub fn peers_key(magic: u16) -> String {
    format!("peers-{}-key", magic)
}
