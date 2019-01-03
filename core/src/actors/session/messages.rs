use std::fmt;

use actix::Message;
use witnet_data_structures::chain::{Block, InventoryEntry};

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

/// Message to send blocks through the network
#[derive(Clone, Debug, Message)]
pub struct SendBlock {
    /// Block
    pub block: Block,
}

impl fmt::Display for SendBlock {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "SendBlock")
    }
}
