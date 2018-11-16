use actix::{Context, Handler};
use log::{debug, info};

use super::messages::{
    AddPeers, GetPeers, GetRandomPeer, PeersSocketAddrResult, PeersSocketAddrsResult, RemovePeers,
};

use super::PeersManager;

/// Handler for AddPeers message
impl Handler<AddPeers> for PeersManager {
    type Result = PeersSocketAddrsResult;

    fn handle(&mut self, msg: AddPeers, _: &mut Context<Self>) -> Self::Result {
        // Insert address
        info!("Add peer handle for addresses: {:?}", msg.addresses);
        self.peers.add(msg.addresses)
    }
}

/// Handler for RemovePeers message
impl Handler<RemovePeers> for PeersManager {
    type Result = PeersSocketAddrsResult;

    fn handle(&mut self, msg: RemovePeers, _: &mut Context<Self>) -> Self::Result {
        // // Find index of element with address
        info!("Remove peer handle for addresses: {:?}", msg.addresses);
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
