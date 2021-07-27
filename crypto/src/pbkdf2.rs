//! PBKDF2 Derivation Function

use ring::pbkdf2;
use std::num::NonZeroU32;

use witnet_protected::Protected;

/// Derive a key with PBKDF2
pub fn pbkdf2_sha256(password: &[u8], salt: &[u8], c: u32) -> Protected {
    let mut secret = Protected::new(vec![0; 32]);
    let n_iter = NonZeroU32::new(c).unwrap();

    pbkdf2::derive(
        pbkdf2::PBKDF2_HMAC_SHA256,
        n_iter,
        salt,
        password,
        &mut secret,
    );

    secret
}
