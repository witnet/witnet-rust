use std::fmt;

use actix::Message;
use witnet_data_structures::chain::{InventoryEntry, InventoryItem};

/// Message result of unit
pub type SessionUnitResult = ();

/// Message to indicate that the session needs to send a GetPeers message through the network
#[derive(Debug)]
pub struct GetPeers;

impl Message for GetPeers {
    type Result = SessionUnitResult;
}

impl fmt::Display for GetPeers {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "GetPeers")
    }
}

/// Message to announce new inventory entries through the network
#[derive(Clone, Debug, Message)]
pub struct AnnounceItems {
    /// Inventory entries
    pub items: Vec<InventoryEntry>,
}

impl fmt::Display for AnnounceItems {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "AnnounceItems")
    }
}

/// Message to send inventory items through the network
#[derive(Clone, Debug, Message)]
pub struct SendInventoryItem {
    /// InventoryItem
    pub item: InventoryItem,
}

impl fmt::Display for SendInventoryItem {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "SendInventoryItem")
    }
}

/// Message to request blocks through the network
#[derive(Clone, Debug, Message)]
pub struct RequestBlock {
    /// Block
    pub block_entry: InventoryEntry,
}

impl fmt::Display for RequestBlock {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "RequestBlock")
    }
}
