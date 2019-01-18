//! Messages for `RadManager`
use actix::Message;
use witnet_data_structures::chain::{RADConsensus, RADRequest};

/// Message for resolving the request-aggregate step of a data
/// request.
#[derive(Debug)]
pub struct ResolveRA {
    /// RAD request to be executed
    pub script: RADRequest,
}

/// Message for running the consensus step of a data request.
#[derive(Debug)]
pub struct RunConsensus {
    /// RAD consensus to be executed
    pub script: RADConsensus,
    /// Reveals vector for consensus
    pub reveals: Vec<Vec<u8>>,
}

/// Message result of unit
pub type SessionUnitResult = ();

impl Message for ResolveRA {
    // TODO: Use RAD error
    type Result = Result<Vec<u8>, String>;
}

impl Message for RunConsensus {
    // TODO: Use RAD error
    type Result = Result<Vec<u8>, String>;
}
