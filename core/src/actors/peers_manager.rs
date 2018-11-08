use std::net::SocketAddr;

use actix::{Actor, Context, Handler, Message, Supervised, SystemService};
use log::{debug, error};

use crate::actors::config_manager::send_get_config_request;

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

    fn started(&mut self, ctx: &mut Self::Context) {
        debug!("Peers Manager actor has been started!");

        // Send message to config manager and process response
        send_get_config_request(self, ctx, |s, _ctx, config| {
            // Get known peers
            let known_peers = config.connections.known_peers.iter().cloned().collect();

            // Add all peers
            match s.peers.add(known_peers) {
                Ok(peers) => debug!("Added the following peer addresses: {:?}", peers),
                Err(e) => error!("Error when adding peer addresses: {}", e),
            }
        });
    }
}

/// Required traits for being able to retrieve session manager address from registry
impl Supervised for PeersManager {}
impl SystemService for PeersManager {}

/// Messages for peer management:
/// * Add peers
/// * Remove peers
/// * Get random peer
/// * Get all peers

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

/// Handlers to manage the previous messages using the `peers` library:
/// * Add peers
/// * Remove peers
/// * Get random peer
/// * Get all peers

/// Handler for AddPeers message
impl Handler<AddPeers> for PeersManager {
    type Result = PeersSocketAddrsResult;

    fn handle(&mut self, msg: AddPeers, _: &mut Context<Self>) -> Self::Result {
        // Insert address
        debug!("Add peer handle for addresses: {:?}", msg.addresses);
        self.peers.add(msg.addresses)
    }
}

/// Handler for RemovePeers message
impl Handler<RemovePeers> for PeersManager {
    type Result = PeersSocketAddrsResult;

    fn handle(&mut self, msg: RemovePeers, _: &mut Context<Self>) -> Self::Result {
        // // Find index of element with address
        debug!("Remove peer handle for addresses: {:?}", msg.addresses);
        self.peers.remove(&msg.addresses)
    }
}

/// Handler for GetRandomPeer message
impl Handler<GetRandomPeer> for PeersManager {
    type Result = PeersSocketAddrResult;

    fn handle(&mut self, _msg: GetRandomPeer, _: &mut Context<Self>) -> Self::Result {
        debug!("Get random peer");
        self.peers.get_random()
    }
}

/// Handler for GetPeers message
impl Handler<GetPeers> for PeersManager {
    type Result = PeersSocketAddrsResult;

    fn handle(&mut self, _msg: GetPeers, _: &mut Context<Self>) -> Self::Result {
        debug!("Get all peers");
        self.peers.get_all()
    }
}
