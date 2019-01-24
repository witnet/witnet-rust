//! Messages for `RadManager`
use actix::Message;
use witnet_data_structures::chain::{RADConsensus, RADRequest};
use witnet_rad::error::RadResult;

/// Message for resolving the request-aggregate step of a data
/// request.
#[derive(Debug)]
pub struct ResolveRA {
    /// RAD request to be executed
    pub rad_request: RADRequest,
}

/// Message for running the consensus step of a data request.
#[derive(Debug)]
pub struct RunConsensus {
    /// RAD consensus to be executed
    pub script: RADConsensus,
    /// Reveals vector for consensus
    pub reveals: Vec<Vec<u8>>,
}

impl Message for ResolveRA {
    type Result = RadResult<Vec<u8>>;
}

impl Message for RunConsensus {
    type Result = RadResult<Vec<u8>>;
}
