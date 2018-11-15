use std::net::SocketAddr;

use actix::Message;

use witnet_p2p::peers::error::PeersResult;

/// One peer
pub type PeersSocketAddrResult = PeersResult<Option<SocketAddr>>;
/// One or more peer addresses
pub type PeersSocketAddrsResult = PeersResult<Vec<SocketAddr>>;

/// Message to add one or more peer addresses to the list
pub struct AddPeers {
    /// Address of the peer
    pub addresses: Vec<SocketAddr>,
}

impl Message for AddPeers {
    type Result = PeersSocketAddrsResult;
}

/// Message to remove one or more peer addresses from the list
pub struct RemovePeers {
    /// Address of the peer
    pub addresses: Vec<SocketAddr>,
}

impl Message for RemovePeers {
    type Result = PeersSocketAddrsResult;
}

/// Message to get a (random) peer address from the list
pub struct GetRandomPeer;

impl Message for GetRandomPeer {
    type Result = PeersSocketAddrResult;
}

/// Message to get all the peer addresses from the list
pub struct GetPeers;

impl Message for GetPeers {
    type Result = PeersSocketAddrsResult;
}
