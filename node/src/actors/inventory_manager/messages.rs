use actix::Message;

use witnet_data_structures::chain::{Hash, InventoryItem};

use super::InventoryManagerError;

/// Add a new item
pub struct AddItem {
    /// Item
    pub item: InventoryItem,
}

impl Message for AddItem {
    type Result = Result<(), InventoryManagerError>;
}

/// Ask for an item identified by its hash
pub struct GetItem {
    /// item hash
    pub hash: Hash,
}

impl Message for GetItem {
    type Result = Result<InventoryItem, InventoryManagerError>;
}
