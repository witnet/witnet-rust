//! Messages for `RadManager`
use actix::Message;

/// Message for resolving the request-aggregate step of a data
/// request.
#[derive(Debug, Message)]
pub struct ResolveRA;

/// Message for running the consensus step of a data request.
#[derive(Debug, Message)]
pub struct RunConsensus;
