use actix::Message;

use crate::actors::chain_manager::ChainManagerError;
use std::ops::{Bound, RangeBounds};
use witnet_data_structures::{
    chain::{
        Block, CheckpointBeacon, Epoch, Hash, InventoryEntry, Output, OutputPointer, Transaction,
    },
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

/// Add a new transaction
pub struct AddTransaction {
    /// Transaction
    pub transaction: Transaction,
}

impl Message for AddTransaction {
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
    /// Range of Epochs (prefer using the new method to create a range)
    pub range: (Bound<Epoch>, Bound<Epoch>),
}

impl GetBlocksEpochRange {
    /// Create a GetBlockEpochRange message using range syntax:
    ///
    /// ```rust
    /// # use witnet_core::actors::chain_manager::messages::GetBlocksEpochRange;
    /// GetBlocksEpochRange::new(..); // Unbounded range: all items
    /// GetBlocksEpochRange::new(10..); // All items starting from epoch 10
    /// GetBlocksEpochRange::new(..10); // All items up to epoch 10 (10 excluded)
    /// GetBlocksEpochRange::new(..=9); // All items up to epoch 9 inclusive (same as above)
    /// GetBlocksEpochRange::new(4..=4); // Only epoch 4
    /// ```
    pub fn new<R: RangeBounds<Epoch>>(r: R) -> Self {
        // Manually implement `cloned` method
        let cloned = |b: Bound<&Epoch>| match b {
            Bound::Included(x) => Bound::Included(*x),
            Bound::Excluded(x) => Bound::Excluded(*x),
            Bound::Unbounded => Bound::Unbounded,
        };

        Self {
            range: (cloned(r.start_bound()), cloned(r.end_bound())),
        }
    }
}

impl Message for GetBlocksEpochRange {
    type Result = Result<Vec<(Epoch, InventoryEntry)>, ChainManagerError>;
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

/// Ask for an output
pub struct GetOutput {
    /// Output pointer
    pub output_pointer: OutputPointer,
}

/// Result of the GetOutput message handling
pub type GetOutputResult = Result<Output, ChainManagerError>;

impl Message for GetOutput {
    type Result = GetOutputResult;
}
