//! Verifiable Random Functions
//!
//! This module integrates the `vrf` crate with the data structures used in witnet.
use serde::{Deserialize, Serialize};
use vrf::{
    openssl::{CipherSuite, ECVRF},
    VRF,
};

use crate::{
    chain::{CheckpointBeacon, Hash, PublicKey, SecretKey},
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
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, ProtobufConvert)]
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
    ) -> Result<VrfProof, failure::Error> {
        // The public key is derived from the secret key
        let public_key_bytes = vrf.0.derive_public_key(&secret_key.bytes)?;
        let public_key = PublicKey::try_from_slice(&public_key_bytes)?;
        let proof = vrf.0.prove(&secret_key.bytes, &message.0)?;

        Ok(VrfProof { proof, public_key })
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

    // TODO: remove unwraps
    pub fn hash(&self, vrf: &mut VrfCtx) -> Hash {
        let h = vrf.0.proof_to_hash(&self.proof).unwrap();
        let mut x = [0; 32];
        x.copy_from_slice(&h);
        Hash::SHA256(x)
    }
}

/// Wrapper type to prevent creating VRF proofs of arbitrary data
#[derive(Debug)]
pub struct VrfMessage(Vec<u8>);

// Functions to easily construct the vrf messages
impl VrfMessage {
    /// Create a VRF proof used for block eligibility
    pub fn block_mining(beacon: CheckpointBeacon) -> VrfMessage {
        VrfMessage(beacon.to_pb_bytes().unwrap())
    }

    /// Create a VRF proof used for data request commitment eligibility
    pub fn data_request(beacon: CheckpointBeacon, dr_hash: Hash) -> VrfMessage {
        VrfMessage(
            DataRequestVrfMessage { beacon, dr_hash }
                .to_pb_bytes()
                .unwrap(),
        )
    }
}

/// Block mining eligibility claim
#[derive(Debug, Eq, PartialEq, Clone, Serialize, Deserialize, ProtobufConvert, Default)]
#[protobuf_convert(pb = "witnet::Block_BlockEligibilityClaim")]
pub struct BlockEligibilityClaim {
    /// A Verifiable Random Function proof of the eligibility for a given epoch and public key
    pub proof: VrfProof,
}

impl BlockEligibilityClaim {
    /// Create a block eligibility claim for a given beacon
    pub fn create(
        vrf: &mut VrfCtx,
        secret_key: &SecretKey,
        beacon: CheckpointBeacon,
    ) -> Result<Self, failure::Error> {
        let message = VrfMessage::block_mining(beacon);
        Ok(Self {
            proof: VrfProof::create(vrf, secret_key, &message)?,
        })
    }

    /// Verify a block eligibility claim for a given beacon
    pub fn verify(
        &self,
        vrf: &mut VrfCtx,
        beacon: CheckpointBeacon,
    ) -> Result<Hash, failure::Error> {
        self.proof
            .verify(vrf, &VrfMessage::block_mining(beacon))
            .map(|v| {
                let mut sha256 = [0; 32];
                sha256.copy_from_slice(&v);

                Hash::SHA256(sha256)
            })
    }
}

/// Structure used to serialize the parameters needed for the `DataRequestEligibilityClaim`
#[derive(Debug, ProtobufConvert)]
#[protobuf_convert(pb = "witnet::DataRequestVrfMessage")]
struct DataRequestVrfMessage {
    beacon: CheckpointBeacon,
    dr_hash: Hash,
}

/// Data request eligibility claim
#[derive(Debug, Eq, PartialEq, Clone, Serialize, Deserialize, ProtobufConvert, Default)]
#[protobuf_convert(pb = "witnet::DataRequestEligibilityClaim")]
pub struct DataRequestEligibilityClaim {
    /// A Verifiable Random Function proof of the eligibility for a given epoch, public key and data request
    pub proof: VrfProof,
}

impl DataRequestEligibilityClaim {
    /// Create a data request eligibility claim for a given beacon and data request hash
    pub fn create(
        vrf: &mut VrfCtx,
        secret_key: &SecretKey,
        beacon: CheckpointBeacon,
        dr_hash: Hash,
    ) -> Result<Self, failure::Error> {
        let message = VrfMessage::data_request(beacon, dr_hash);
        Ok(Self {
            proof: VrfProof::create(vrf, secret_key, &message)?,
        })
    }

    /// Verify a data request eligibility claim for a given beacon and data request hash
    pub fn verify(
        &self,
        vrf: &mut VrfCtx,
        beacon: CheckpointBeacon,
        dr_hash: Hash,
    ) -> Result<Hash, failure::Error> {
        self.proof
            .verify(vrf, &VrfMessage::data_request(beacon, dr_hash))
            .map(|v| {
                let mut sha256 = [0; 32];
                sha256.copy_from_slice(&v);

                Hash::SHA256(sha256)
            })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use vrf::openssl::CipherSuite;

    #[test]
    fn vrf_derived_public_key() {
        // Test that the public key derived by the VRF crate is the same as the
        // public key derived by the secp256k1 crate
        use crate::chain::PublicKey;
        use secp256k1::{
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
        let vrf_pk = vrf_proof.public_key;

        assert_eq!(witnet_pk, vrf_pk);
    }
}
