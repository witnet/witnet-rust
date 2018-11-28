use actix::Message;

use crate::actors::blocks_manager::BlocksManagerError;
use witnet_data_structures::{
    chain::{Block, Epoch, Hash},
    error::ChainInfoResult,
};

/// Message to obtain the highest block checkpoint managed by the `BlocksManager`
/// actor.
pub struct GetHighestBlockCheckpoint;

impl Message for GetHighestBlockCheckpoint {
    type Result = ChainInfoResult<Epoch>;
}

/// Add a new block
pub struct AddNewBlock {
    /// Block
    pub block: Block,
}

impl Message for AddNewBlock {
    type Result = Result<Hash, BlocksManagerError>;
}
