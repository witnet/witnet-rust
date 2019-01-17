//! Messages for `RadManager`
use actix::Message;
use witnet_data_structures::chain::RADRequest;

/// Message for resolving the request-aggregate step of a data
/// request.
#[derive(Debug)]
pub struct ResolveRA {
    /// RAD request to be executed
    pub script: RADRequest,
}

/// Message for running the consensus step of a data request.
#[derive(Debug, Message)]
pub struct RunConsensus;

/// Message result of unit
pub type SessionUnitResult = ();

impl Message for ResolveRA {
    type Result = SessionUnitResult;
}
