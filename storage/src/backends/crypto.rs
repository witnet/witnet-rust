//! # Encrypted storage backend
//!
//! High-order storage backend that hashes the key and
//! encrypts/decrypts the value when putting/getting it.
use crate::storage::{Result, Storage};
use witnet_crypto::{cipher, hash::calculate_sha256, pbkdf2::pbkdf2_sha256};
use witnet_protected::Protected;

const IV_LENGTH: usize = 16;
const SALT_LENGTH: usize = 32;
const HASH_ITER_COUNT: u32 = 10_000;

/// Backend that stores values encrypted.
pub struct Backend<T> {
    backend: T,
    password: Protected,
}

impl<T: Storage> Backend<T> {
    /// Create encrypted backend which will use `backend` as the
    /// actual storage backend but will encrypt the data with
    /// `password`
    pub fn new(password: Protected, backend: T) -> Self {
        Backend { password, backend }
    }

    /// Get a reference to the inner storage backend
    pub fn inner(&self) -> &T {
        &self.backend
    }
}

impl<T: Storage> Storage for Backend<T> {
    fn get(&self, key: &[u8]) -> Result<Option<Vec<u8>>> {
        let hash_key = calculate_sha256(key);

        self.backend
            .get(hash_key.as_ref())
            .and_then(|opt| match opt {
                Some(encrypted_bytes) => {
                    let len = encrypted_bytes.len();
                    let iv = &encrypted_bytes[0..IV_LENGTH];
                    let data = &encrypted_bytes[IV_LENGTH..len - SALT_LENGTH];
                    let salt = &encrypted_bytes[len - SALT_LENGTH..];
                    let secret = get_secret(&self.password, salt);

                    cipher::decrypt_aes_cbc(&secret, data, iv)
                        .map(Some)
                        .map_err(Into::into)
                }
                None => Ok(None),
            })
    }

    fn put(&mut self, key: Vec<u8>, value: Vec<u8>) -> Result<()> {
        let hash_key = calculate_sha256(key.as_ref());
        let iv = cipher::generate_random(IV_LENGTH)?;
        let salt = cipher::generate_random(SALT_LENGTH)?;
        let secret = get_secret(&self.password, &salt);
        let encrypted = cipher::encrypt_aes_cbc(&secret, value.as_ref(), iv.as_ref())?;
        let mut final_value = iv;
        final_value.extend(encrypted);
        final_value.extend(salt);

        self.backend.put(hash_key.as_ref().to_vec(), final_value)
    }

    fn delete(&mut self, key: &[u8]) -> Result<()> {
        let hash_key = calculate_sha256(key);
        self.backend.delete(hash_key.as_ref())
    }
}

fn get_secret(password: &[u8], salt: &[u8]) -> Protected {
    pbkdf2_sha256(password, salt, HASH_ITER_COUNT)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backends::hashmap;

    #[test]
    fn test_encrypt_decrypt() {
        let password = "".into();
        let mut backend = Backend::new(password, hashmap::Backend::new());

        assert_eq!(None, backend.get(b"name").unwrap());
        backend.put("name".into(), "johnny".into()).unwrap();
        assert_eq!(Some("johnny".into()), backend.get(b"name").unwrap());
    }

    #[test]
    fn test_read_with_other_password() {
        let password1 = "pass1".into();
        let password2 = "pass2".into();

        let mut backend1 = Backend::new(password1, hashmap::Backend::new());

        backend1.put("name".into(), "johnny".into()).unwrap();

        let backend2 = Backend::new(password2, backend1.inner().clone());

        assert_ne!(
            backend2.get(b"name").unwrap_or(None),
            Some(b"johnny".to_vec())
        );
    }

    #[test]
    fn test_delete() {
        let password = "".into();
        let mut backend = Backend::new(password, hashmap::Backend::new());

        assert_eq!(None, backend.get(b"name").unwrap());
        backend.put("name".into(), "johnny".into()).unwrap();
        assert_eq!(Some("johnny".into()), backend.get(b"name").unwrap());
        backend.delete(b"name").unwrap();
        assert_eq!(None, backend.get(b"name").unwrap());
    }
}
