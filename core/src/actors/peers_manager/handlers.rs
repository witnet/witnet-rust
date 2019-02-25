use actix::{Context, Handler};
use log::{debug, error, warn};

use super::PeersManager;
use crate::actors::messages::{
    AddPeers, GetPeers, GetRandomPeer, PeersSocketAddrResult, PeersSocketAddrsResult, RemovePeers,
};

/// Handler for AddPeers message
impl Handler<AddPeers> for PeersManager {
    type Result = PeersSocketAddrsResult;

    fn handle(&mut self, msg: AddPeers, _: &mut Context<Self>) -> Self::Result {
        // Insert address
        debug!("Adding the following peer addresses: {:?}", msg.addresses);
        self.peers.add(msg.addresses)
    }
}

/// Handler for RemovePeers message
impl Handler<RemovePeers> for PeersManager {
    type Result = PeersSocketAddrsResult;

    fn handle(&mut self, msg: RemovePeers, _: &mut Context<Self>) -> Self::Result {
        // // Find index of element with address
        debug!("Removing the following addresses: {:?}", msg.addresses);
        self.peers.remove(&msg.addresses)
    }
}

/// Handler for GetRandomPeer message
impl Handler<GetRandomPeer> for PeersManager {
    type Result = PeersSocketAddrResult;

    fn handle(&mut self, _msg: GetRandomPeer, _: &mut Context<Self>) -> Self::Result {
        let result = self.peers.get_random();

        match result {
            Ok(Some(address)) => {
                debug!("Selected a random peer address: {:?}", address);
                result
            }
            Ok(None) => {
                warn!("Could not select a random peer address because there were none");
                result
            }
            error => {
                error!("Error selecting a random peer address: {:?}", error);
                error
            }
        }
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
