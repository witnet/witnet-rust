use crate::types;

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
