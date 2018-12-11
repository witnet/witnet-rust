use actix::Message;
use witnet_data_structures::chain::InventoryEntry;

/// Message result of unit
pub type SessionUnitResult = ();

/// Message to indicate that the session needs to send a GetPeers message through the network
pub struct GetPeers;

impl Message for GetPeers {
    type Result = SessionUnitResult;
}

/// Message to announce new inventory entries through the network
#[derive(Clone, Message)]
pub struct AnnounceItems {
    /// Inventory entries
    pub items: Vec<InventoryEntry>,
}
