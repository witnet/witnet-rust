//! Verifiable Random Functions
//!
//! This module integrates the `vrf` crate with the data structures used in witnet.
use serde::{Deserialize, Serialize};
use vrf::{
    openssl::{CipherSuite, ECVRF},
    VRF,
};

use crate::{
    chain::{CheckpointVRF, Hash, HashParseError, PublicKey, PublicKeyHash, SecretKey},
    proto::{schema::witnet, ProtobufConvert},
};

/// VRF context using SECP256K1 curve
#[derive(Debug)]
pub struct VrfCtx(ECVRF);

impl VrfCtx {
    /// Initialize a VRF context for the SECP256K1 curve
    pub fn secp256k1() -> Result<Self, failure::Error> {
        let vrf = ECVRF::from_suite(CipherSuite::SECP256K1_SHA256_TAI)?;

        Ok(Self(vrf))
    }
}

/// A VRF Proof is a unique, deterministic way to sign a message with a public key.
/// It is used to prevent one identity from creating multiple different proofs of eligibility.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, ProtobufConvert, Hash)]
#[protobuf_convert(pb = "witnet::VrfProof")]
pub struct VrfProof {
    proof: Vec<u8>,
    public_key: PublicKey,
}

impl VrfProof {
    /// Create a VRF proof for a given message
    pub fn create(
        vrf: &mut VrfCtx,
        secret_key: &SecretKey,
        message: &VrfMessage,
    ) -> Result<(VrfProof, Hash), failure::Error> {
        // The public key is derived from the secret key
        let public_key_bytes = vrf.0.derive_public_key(&secret_key.bytes)?;
        let public_key = PublicKey::try_from_slice(&public_key_bytes)?;
        let proof = vrf.0.prove(&secret_key.bytes, &message.0)?;
        let proof_hash = vrf.0.proof_to_hash(&proof)?;
        let vrf_proof = VrfProof { proof, public_key };

        if proof_hash.len() != 32 {
            Err(HashParseError::InvalidLength(proof_hash.len()).into())
        } else {
            let mut x = [0; 32];
            x.copy_from_slice(&proof_hash);
            let proof_hash = Hash::SHA256(x);

            Ok((vrf_proof, proof_hash))
        }
    }

    /// Verify the proof. The message must be exactly the same as the one used to create the proof.
    pub fn verify(
        &self,
        vrf: &mut VrfCtx,
        message: &VrfMessage,
    ) -> Result<Vec<u8>, failure::Error> {
        Ok(vrf
            .0
            .verify(&self.public_key.to_bytes(), &self.proof, &message.0)?)
    }

    /// Return the pkh of the identity that signed the proof
    pub fn pkh(&self) -> PublicKeyHash {
        PublicKeyHash::from_public_key(&self.public_key)
    }

    /// Getter for the VRF proof
    pub fn get_proof(&self) -> Vec<u8> {
        self.proof.clone()
    }

    /// Create a VRF proof for a given message
    pub fn proof_to_hash(&self, vrf: &mut VrfCtx) -> Result<Hash, failure::Error> {
        let proof_hash = vrf.0.proof_to_hash(&self.proof)?;
        if proof_hash.len() != 32 {
            Err(HashParseError::InvalidLength(proof_hash.len()).into())
        } else {
            let mut x = [0; 32];
            x.copy_from_slice(&proof_hash);
            let proof_hash = Hash::SHA256(x);

            Ok(proof_hash)
        }
    }
}

/// Wrapper type to prevent creating VRF proofs of arbitrary data
#[derive(Debug, Hash, Serialize, Deserialize)]
pub struct VrfMessage(Vec<u8>);

// Functions to easily construct the vrf messages
impl VrfMessage {
    /// Create a VRF message used for block eligibility
    pub fn block_mining(vrf_input: CheckpointVRF) -> VrfMessage {
        VrfMessage(vrf_input.to_pb_bytes().unwrap())
    }

    /// Create a VRF message used for data request commitment eligibility
    pub fn data_request(vrf_input: CheckpointVRF, dr_hash: Hash) -> VrfMessage {
        VrfMessage(
            DataRequestVrfMessage { vrf_input, dr_hash }
                .to_pb_bytes()
                .unwrap(),
        )
    }
    /// Create a VRF message
    pub fn set_data(data: Vec<u8>) -> VrfMessage {
        VrfMessage(data)
    }
}

/// Block mining eligibility claim
#[derive(Debug, Eq, PartialEq, Clone, Serialize, Deserialize, ProtobufConvert, Default, Hash)]
#[protobuf_convert(pb = "witnet::Block_BlockEligibilityClaim")]
pub struct BlockEligibilityClaim {
    /// A Verifiable Random Function proof of the eligibility for a given epoch and public key
    pub proof: VrfProof,
}

impl BlockEligibilityClaim {
    /// Create a block eligibility claim for a given vrf message
    pub fn create(
        vrf: &mut VrfCtx,
        secret_key: &SecretKey,
        vrf_input: CheckpointVRF,
    ) -> Result<Self, failure::Error> {
        let message = VrfMessage::block_mining(vrf_input);
        Ok(Self {
            proof: VrfProof::create(vrf, secret_key, &message)?.0,
        })
    }

    /// Verify a block eligibility claim for a given vrf message
    pub fn verify(
        &self,
        vrf: &mut VrfCtx,
        vrf_input: CheckpointVRF,
    ) -> Result<Hash, failure::Error> {
        self.proof
            .verify(vrf, &VrfMessage::block_mining(vrf_input))
            .map(|v| {
                let mut sha256 = [0; 32];
                sha256.copy_from_slice(&v);

                Hash::SHA256(sha256)
            })
    }

    /// Output the hash of the VRF proof.
    /// This hash will become the input to future VRF create functions that compute eligibilities.
    pub fn proof_to_hash(&self, vrf: &mut VrfCtx) -> Result<Hash, failure::Error> {
        self.proof.proof_to_hash(vrf)
    }
}

/// Structure used to serialize the parameters needed for the `DataRequestEligibilityClaim`
#[derive(Debug, ProtobufConvert)]
#[protobuf_convert(pb = "witnet::DataRequestVrfMessage")]
struct DataRequestVrfMessage {
    vrf_input: CheckpointVRF,
    dr_hash: Hash,
}

/// Data request eligibility claim
#[derive(Debug, Eq, PartialEq, Clone, Serialize, Deserialize, ProtobufConvert, Default, Hash)]
#[protobuf_convert(pb = "witnet::DataRequestEligibilityClaim")]
pub struct DataRequestEligibilityClaim {
    /// A Verifiable Random Function proof of the eligibility for a given epoch, public key and data request
    pub proof: VrfProof,
}

impl DataRequestEligibilityClaim {
    /// Create a data request eligibility claim for a given vrf message and data request hash
    pub fn create(
        vrf: &mut VrfCtx,
        secret_key: &SecretKey,
        vrf_input: CheckpointVRF,
        dr_hash: Hash,
    ) -> Result<Self, failure::Error> {
        let message = VrfMessage::data_request(vrf_input, dr_hash);
        Ok(Self {
            proof: VrfProof::create(vrf, secret_key, &message)?.0,
        })
    }

    /// Verify a data request eligibility claim for a given vrf message and data request hash
    pub fn verify(
        &self,
        vrf: &mut VrfCtx,
        vrf_input: CheckpointVRF,
        dr_hash: Hash,
    ) -> Result<Hash, failure::Error> {
        self.proof
            .verify(vrf, &VrfMessage::data_request(vrf_input, dr_hash))
            .map(|v| {
                let mut sha256 = [0; 32];
                sha256.copy_from_slice(&v);

                Hash::SHA256(sha256)
            })
    }
}

#[cfg(test)]
mod tests {
    use witnet_protected::Protected;

    use super::*;

    #[test]
    fn vrf_derived_public_key() {
        // Test that the public key derived by the VRF crate is the same as the
        // public key derived by the secp256k1 crate
        use crate::chain::PublicKey;
        use witnet_crypto::secp256k1::{
            PublicKey as Secp256k1_PublicKey, Secp256k1, SecretKey as Secp256k1_SecretKey,
        };

        let secp = Secp256k1::new();
        let secret_key =
            Secp256k1_SecretKey::from_slice(&[0xcd; 32]).expect("32 bytes, within curve order");
        let public_key = Secp256k1_PublicKey::from_secret_key(&secp, &secret_key);
        let witnet_pk = PublicKey::from(public_key);
        let witnet_sk = SecretKey::from(secret_key);

        let vrf = &mut VrfCtx::secp256k1().unwrap();
        let vrf_proof = VrfProof::create(vrf, &witnet_sk, &VrfMessage(b"test".to_vec())).unwrap();
        let vrf_pk = vrf_proof.0.public_key;

        assert_eq!(witnet_pk, vrf_pk);
    }

    #[test]
    fn block_proof_validity() {
        let vrf = &mut VrfCtx::secp256k1().unwrap();
        let secret_key = SecretKey {
            bytes: Protected::from(vec![0x44; 32]),
        };
        let vrf_input = CheckpointVRF {
            checkpoint: 0,
            hash_prev_vrf: Default::default(),
        };

        let proof = BlockEligibilityClaim::create(vrf, &secret_key, vrf_input).unwrap();
        assert!(proof.verify(vrf, vrf_input).is_ok());

        // Changing the beacon should invalidate the vrf proof
        for checkpoint in 1..=2 {
            let vrf_input2 = CheckpointVRF {
                checkpoint,
                ..vrf_input
            };
            assert!(proof.verify(vrf, vrf_input2).is_err());
        }
    }

    #[test]
    fn data_request_proof_validity() {
        let vrf = &mut VrfCtx::secp256k1().unwrap();
        let secret_key = SecretKey {
            bytes: Protected::from(vec![0x44; 32]),
        };
        let vrf_input = CheckpointVRF {
            checkpoint: 0,
            hash_prev_vrf: Default::default(),
        };
        let dr_pointer = "2222222222222222222222222222222222222222222222222222222222222222"
            .parse()
            .unwrap();
        let proof =
            DataRequestEligibilityClaim::create(vrf, &secret_key, vrf_input, dr_pointer).unwrap();
        assert!(proof.verify(vrf, vrf_input, dr_pointer).is_ok());

        // Changing the beacon should invalidate the vrf proof
        let vrf_input2 = CheckpointVRF {
            checkpoint: 1,
            ..vrf_input
        };
        assert!(proof.verify(vrf, vrf_input2, dr_pointer).is_err());

        // Changing the dr_hash should invalidate the vrf proof
        let dr_pointer2 = "2222222222222222222222222222222222222222222222222222222222222223"
            .parse()
            .unwrap();
        assert!(proof.verify(vrf, vrf_input, dr_pointer2).is_err());
    }
}
