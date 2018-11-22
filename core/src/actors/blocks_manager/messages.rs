use actix::Message;

use witnet_data_structures::{chain::Epoch, error::ChainInfoResult};

/// Message to obtain the highest block checkpoint managed by the `BlocksManager`
/// actor.
pub struct GetHighestBlockCheckpoint;

impl Message for GetHighestBlockCheckpoint {
    type Result = ChainInfoResult<Epoch>;
}
