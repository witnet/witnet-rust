//! PBKDF2 Derivation Function

use crypto::{hmac::Hmac, pbkdf2::pbkdf2, sha2};

use witnet_protected::Protected;

/// Derive a key with PBKDF2
pub fn pbkdf2_sha256(password: &[u8], salt: &[u8], c: u32) -> Protected {
    let mut mac = Hmac::new(sha2::Sha256::new(), password);
    let mut secret = Protected::new(vec![0; 32]);

    pbkdf2(&mut mac, salt, c, secret.as_mut());

    secret
}
