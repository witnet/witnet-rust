use actix::Message;

use witnet_data_structures::chain::{CheckpointBeacon, LeadershipProof};

/// Message for checking that a proof of eligibility is valid for the
/// known reputation of the issuer of the proof
#[derive(Debug)]
pub struct ValidatePoE {
    /// Block CheckpointBeacon
    pub beacon: CheckpointBeacon,

    /// Block proof
    pub proof: LeadershipProof,
}

impl Message for ValidatePoE {
    type Result = bool;
}
