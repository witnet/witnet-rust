//! Signature module

use failure::Fail;
use secp256k1::{Message, PublicKey, Secp256k1, SecretKey};

/// Signature
pub type Signature = secp256k1::Signature;

/// The error type for operations with signatures
#[derive(Debug, PartialEq, Fail)]
pub enum SignatureError {
    #[fail(display = "Fail in verify process")]
    /// Fail in verify process
    VerifyError,
}

/// Sign data with provided secret key
pub fn sign(secret_key: SecretKey, data: &[u8]) -> Signature {
    let msg = Message::from_slice(data).unwrap();
    let secp = Secp256k1::new();
    secp.sign(&msg, &secret_key)
}
/// Verify signature with a provided public key
pub fn verify(public_key: &PublicKey, data: &[u8], sig: &Signature) -> Result<(), failure::Error> {
    let msg = Message::from_slice(data).unwrap();
    let secp = Secp256k1::new();

    secp.verify(&msg, sig, public_key)
        .map_err(|_| SignatureError::VerifyError.into())
}

#[cfg(test)]
mod tests {
    use crate::hash::{calculate_sha256, Sha256};
    use crate::signature::{sign, verify};
    use secp256k1::{PublicKey, Secp256k1, SecretKey, Signature};

    #[test]
    fn test_sign_and_verify() {
        let data = [0xab; 32];
        let secp = Secp256k1::new();
        let secret_key = SecretKey::from_slice(&[0xcd; 32]).expect("32 bytes, within curve order");
        let public_key = PublicKey::from_secret_key(&secp, &secret_key);

        let signature = sign(secret_key, &data);
        let signature_expected = "3044\
                                  0220\
                                  3dc4fa74655c21b7ffc0740e29bfd88647e8dfe2b68c507cf96264e4e7439c1f\
                                  0220\
                                  7aa61261b18eebdfdb704ca7bab4c7bcf7961ae0ade5309f6f1398e21aec0f9f\
                                  0000";
        assert_eq!(signature_expected.to_string(), signature.to_string());

        assert!(verify(&public_key, &data, &signature).is_ok());
    }

    #[test]
    fn test_der_and_compact() {
        let der1 = "3044\
                    0220\
                    3dc4fa74655c21b7ffc0740e29bfd88647e8dfe2b68c507cf96264e4e7439c1f\
                    0220\
                    7aa61261b18eebdfdb704ca7bab4c7bcf7961ae0ade5309f6f1398e21aec0f9f";
        let signature1 = Signature::from_der(hex::decode(der1).unwrap().as_slice()).unwrap();

        let r_s = signature1.serialize_compact();
        let (r, s) = r_s.split_at(32);

        let r_expected =
            hex::decode("3dc4fa74655c21b7ffc0740e29bfd88647e8dfe2b68c507cf96264e4e7439c1f")
                .unwrap();
        let s_expected =
            hex::decode("7aa61261b18eebdfdb704ca7bab4c7bcf7961ae0ade5309f6f1398e21aec0f9f")
                .unwrap();

        assert_eq!(r.to_vec(), r_expected);
        assert_eq!(s.to_vec(), s_expected);

        let der2 = "3045\
                    0220\
                    397116930c282d1fcb71166a2d06728120cf2ee5cf6ccd4e2d822e8e0ae24a30\
                    0221\
                    009e997d4718a7603942834fbdd22a4b856fc4083704ede62033cf1a77cb9822a9";

        let signature2 = Signature::from_der(hex::decode(der2).unwrap().as_slice()).unwrap();

        let r_s = signature2.serialize_compact();
        let (r, s) = r_s.split_at(32);

        let r_expected =
            hex::decode("397116930c282d1fcb71166a2d06728120cf2ee5cf6ccd4e2d822e8e0ae24a30")
                .unwrap();
        let s_expected =
            hex::decode("9e997d4718a7603942834fbdd22a4b856fc4083704ede62033cf1a77cb9822a9")
                .unwrap();

        assert_eq!(r.to_vec(), r_expected);
        assert_eq!(s.to_vec(), s_expected);
    }

    #[test]
    fn test_sign_and_verify_before_hash() {
        let secret_key = SecretKey::from_slice(&[0xcd; 32]).expect("32 bytes, within curve order");

        let i = 9;

        let mut message = "Message ".to_string();
        let i_str = i.to_string();
        message.push_str(&i_str);

        let Sha256(hashed_data) = calculate_sha256(message.as_bytes());

        let signature = sign(secret_key, &hashed_data);

        let r_s = signature.serialize_compact();
        let (r, s) = r_s.split_at(32);

        let r_expected =
            hex::decode("87d0a0e4e8af2b911f5e8834a6335307ed226fcd1fabe97cffedd37240fdca33")
                .unwrap();
        let s_expected =
            hex::decode("7d1cd708ea12c2701e47633745907f6d20f29c621313b8eabb1c2f24b34ebd90")
                .unwrap();

        assert_eq!(r.to_vec(), r_expected);
        assert_eq!(s.to_vec(), s_expected);
    }
}
