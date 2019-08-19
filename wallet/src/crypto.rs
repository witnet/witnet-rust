pub use witnet_crypto::hash::calculate_sha256;
use witnet_crypto::{hash::HashFunction, key::MasterKeyGen, pbkdf2::pbkdf2_sha256};

use crate::types;

/// Result type for cryptographic operations that can fail.
pub type Result<T> = std::result::Result<T, failure::Error>;

/// Generate an HD-Wallet master extended key from a seed.
///
/// The seed can be treated as either, a mnemonic phrase or an xprv
pub fn gen_master_key(
    seed: &str,
    salt: &[u8],
    source: &types::SeedSource,
) -> Result<types::ExtendedSK> {
    let key = match source {
        types::SeedSource::Mnemonics(mnemonic) => {
            let seed = mnemonic.seed_ref(seed);

            MasterKeyGen::new(seed).with_key(salt).generate()?
        }
        types::SeedSource::Xprv => {
            // TODO: Implement key generation from xprv
            unimplemented!("xprv not implemented yet")
        }
    };

    Ok(key)
}

/// Generate an encryption key using pbkdf2.
pub fn key_from_password(password: &[u8], salt: &[u8], iterations: u32) -> types::Secret {
    pbkdf2_sha256(password, salt, iterations)
}

/// Generate a cryptographic wallet id.
pub fn gen_wallet_id(
    hash: &HashFunction,
    key: &types::ExtendedSK,
    salt: &[u8],
    iterations: u32,
) -> String {
    match hash {
        HashFunction::Sha256 => {
            let password = key.concat();
            let id_bytes = pbkdf2_sha256(password.as_ref(), salt, iterations);

            hex::encode(id_bytes)
        }
    }
}

/// Generate a cryptographic session id.
pub fn gen_session_id<Rng>(
    rng: &mut Rng,
    hash: &HashFunction,
    key: &[u8],
    salt: &[u8],
    iterations: u32,
) -> String
where
    Rng: rand::Rng + rand::CryptoRng,
{
    match hash {
        HashFunction::Sha256 => {
            let rand_bytes: [u8; 32] = rng.gen();
            let password = [key, salt, rand_bytes.as_ref()].concat();
            let id_bytes = pbkdf2_sha256(&password, &salt, iterations);

            hex::encode(id_bytes)
        }
    }
}

/// Generate a cryptographic salt.
pub fn salt<Rand>(rng: &mut Rand, len: usize) -> Vec<u8>
where
    Rand: rand::Rng + rand::CryptoRng,
{
    let mut bytes = vec![0u8; len];

    rng.fill_bytes(&mut bytes);

    bytes
}
