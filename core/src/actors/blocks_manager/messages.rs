use actix::Message;

use crate::actors::blocks_manager::BlocksManagerError;
use witnet_data_structures::{
    chain::{Block, CheckpointBeacon, Hash},
    error::ChainInfoResult,
};

/// Message to obtain the highest block checkpoint managed by the `BlocksManager`
/// actor.
pub struct GetHighestCheckpointBeacon;

impl Message for GetHighestCheckpointBeacon {
    type Result = ChainInfoResult<CheckpointBeacon>;
}

/// Add a new block
pub struct AddNewBlock {
    /// Block
    pub block: Block,
}

impl Message for AddNewBlock {
    type Result = Result<Hash, BlocksManagerError>;
}
