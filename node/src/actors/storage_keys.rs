/// Constant to specify the secret key key for the storage
pub static MASTER_KEY: &[u8] = b"master_key";

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
