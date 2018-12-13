use actix::Message;
use std::ops::RangeInclusive;

use crate::actors::chain_manager::ChainManagerError;
use witnet_data_structures::{
    chain::{Block, CheckpointBeacon, Epoch, Hash, InventoryEntry},
    error::ChainInfoResult,
};

/// Message result of unit
pub type SessionUnitResult = ();

/// Message to obtain the highest block checkpoint managed by the `ChainManager`
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
    type Result = SessionUnitResult;
}

/// Ask for a block identified by its hash
pub struct GetBlock {
    /// Block hash
    pub hash: Hash,
}

impl Message for GetBlock {
    type Result = Result<Block, ChainManagerError>;
}

/// Message to obtain a vector of block hashes using a range of epochs
pub struct GetBlocksEpochRange {
    /// Range of Epochs
    pub range: RangeInclusive<Epoch>,
}

impl Message for GetBlocksEpochRange {
    type Result = Result<Vec<InventoryEntry>, ChainManagerError>;
}

/// Discard inventory entries that exist in the BlocksManager
pub struct DiscardExistingInventoryEntries {
    /// Vector of inventory entries
    pub inv_entries: Vec<InventoryEntry>,
}

/// Result of the DiscardExistingInventoryEntries message handling
pub type InventoryEntriesResult = Result<Vec<InventoryEntry>, ChainManagerError>;

impl Message for DiscardExistingInventoryEntries {
    type Result = InventoryEntriesResult;
}
