use crate::types;

/// Cryptographic params that can be changed for each wallet.
#[derive(Clone)]
pub struct Params {
    pub testnet: bool,
    pub seed_password: types::Password,
    pub master_key_salt: Vec<u8>,
    pub id_hash_iterations: u32,
    pub id_hash_function: types::HashFunction,
    pub db_hash_iterations: u32,
    pub db_iv_length: usize,
    pub db_salt_length: usize,
}

impl Default for Params {
    fn default() -> Self {
        Self {
            testnet: true,
            seed_password: "".into(),
            master_key_salt: b"Bitcoin seed".to_vec(),
            id_hash_iterations: 4096,
            id_hash_function: types::HashFunction::Sha256,
            db_hash_iterations: 10_000,
            db_iv_length: 16,
            db_salt_length: 32,
        }
    }
}
