//! Signature module

use secp256k1::{Error, Message, PublicKey, Secp256k1, SecretKey};

/// Signature
pub type Signature = secp256k1::Signature;

/// Sign data with provided secret key
pub fn sign(secret_key: SecretKey, data: &[u8]) -> Signature {
    let msg = Message::from_slice(data).unwrap();
    let secp = Secp256k1::new();
    secp.sign(&msg, &secret_key)
}
/// Verify signature with a provided public key
pub fn verify(public_key: PublicKey, data: &[u8], sig: Signature) -> Result<(), Error> {
    let msg = Message::from_slice(data).unwrap();
    let secp = Secp256k1::new();
    secp.verify(&msg, &sig, &public_key)
}

#[cfg(test)]
mod tests {
    use crate::signature::{sign, verify};
    use secp256k1::{PublicKey, Secp256k1, SecretKey};

    #[test]
    fn test_sign_and_verify() {
        let data = [0xab; 32];
        let secp = Secp256k1::new();
        let secret_key = SecretKey::from_slice(&[0xcd; 32]).expect("32 bytes, within curve order");
        let public_key = PublicKey::from_secret_key(&secp, &secret_key);

        let signature = sign(secret_key, &data);
        let signature_expected =
            "304402203dc4fa74655c21b7ffc0740e29bfd88647e8dfe2b68c507cf96264e4e7439c1f\
             02207aa61261b18eebdfdb704ca7bab4c7bcf7961ae0ade5309f6f1398e21aec0f9f0000";
        assert_eq!(signature_expected.to_string(), signature.to_string());

        assert!(verify(public_key, &data, signature).is_ok());
    }
}
