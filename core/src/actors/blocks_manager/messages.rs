use actix::Message;
use std::ops::RangeInclusive;

use crate::actors::blocks_manager::BlocksManagerError;
use witnet_data_structures::{
    chain::{Block, CheckpointBeacon, Epoch, Hash, InvVector},
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

/// Ask for a block identified by its hash
pub struct GetBlock {
    /// Block hash
    pub hash: Hash,
}

impl Message for GetBlock {
    type Result = Result<Block, BlocksManagerError>;
}

/// Message to obtain a vector of block hashes using a range of epochs
pub struct GetBlocksEpochRange {
    /// Range of Epochs
    pub range: RangeInclusive<Epoch>,
}

impl Message for GetBlocksEpochRange {
    type Result = Result<Vec<InvVector>, BlocksManagerError>;
}
