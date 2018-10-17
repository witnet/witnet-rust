use std::net::SocketAddr;

use actix::{Actor, Context, Handler, Message, Supervised, SystemService};
use log::debug;

use witnet_p2p::peers::{error::PeersResult, Peers};

/// Peers manager actor: manages a list of available peers to connect
///
/// During the execuion of the node, there are at least 2 ways in which peers can be discovered:
///   + PEERS message as response to GET_PEERS -> []addr
///   + Incoming connections to the node -> []addr
///
/// In the future, there might be other additional means to retrieve peers, e.g. from trusted servers.
#[derive(Default)]
pub struct PeersManager {
    /// Known peers
    peers: Peers,
}

/// Make actor from `PeersManager`
impl Actor for PeersManager {
    /// Every actor has to provide execution `Context` in which it can run.
    type Context = Context<Self>;
    fn started(&mut self, _ctx: &mut Self::Context) {
        debug!("Peers Manager actor has been started!")
    }
}

/// Required traits for being able to retrieve session manager address from registry
impl Supervised for PeersManager {}
impl SystemService for PeersManager {}

/// Messages for peer management:
///  * add peer
///  * remove peer
///  * get peer

// Message result of an option of Socket Address
type PeersSocketAddrResult = PeersResult<Option<SocketAddr>>;

/// Message to add a peer to list
pub struct AddPeer {
    /// Address of the peer
    pub address: SocketAddr,
}

impl Message for AddPeer {
    type Result = PeersSocketAddrResult;
}

/// Message to remove peer from list
pub struct RemovePeer {
    /// Address of the peer
    pub address: SocketAddr,
}

impl Message for RemovePeer {
    type Result = PeersSocketAddrResult;
}

/// Message to get a (random) peer from the list
pub struct GetPeer;

impl Message for GetPeer {
    type Result = PeersSocketAddrResult;
}

/// Handlers to manage the previous messages using the `peers` library:
/// * Add peer
/// * Remove peer
/// * Get peer

/// Handler for AddPeer message
impl Handler<AddPeer> for PeersManager {
    type Result = PeersSocketAddrResult;

    fn handle(&mut self, msg: AddPeer, _: &mut Context<Self>) -> Self::Result {
        // Insert address
        debug!("Add peer handle for address {}", msg.address);
        self.peers.add(msg.address)
    }
}

/// Handler for RemovePeer message
impl Handler<RemovePeer> for PeersManager {
    type Result = PeersSocketAddrResult;

    fn handle(&mut self, msg: RemovePeer, _: &mut Context<Self>) -> Self::Result {
        // // Find index of element with address
        debug!("Remove peer handle for address {}", msg.address);
        self.peers.remove(msg.address)
    }
}

/// Handler for AddPeer message
impl Handler<GetPeer> for PeersManager {
    type Result = PeersSocketAddrResult;

    fn handle(&mut self, _msg: GetPeer, _: &mut Context<Self>) -> Self::Result {
        debug!("Get peer handle for address");
        self.peers.get_random()
    }
}
