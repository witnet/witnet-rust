use actix::{Context, Handler};

use super::PeersManager;
use crate::actors::messages::{
    AddConsolidatedPeer, AddPeers, ClearPeers, EpochNotification, GetKnownPeers, GetRandomPeers,
    GetUpdatedPeersPercentage, InitializePeers, PeersNewTried, PeersSocketAddrResult,
    PeersSocketAddrsResult, RemoveAddressesFromTried, RequestPeers,
};
use witnet_util::timestamp::get_timestamp;

/// Handler for AddPeers message
impl Handler<AddPeers> for PeersManager {
    type Result = PeersSocketAddrsResult;

    fn handle(&mut self, msg: AddPeers, _: &mut Context<Self>) -> Self::Result {
        // Insert address
        log::trace!("Adding the following peer addresses: {:?}", msg.addresses);
        // Remove peers added manually from the ice bucket to ensure that they are always added
        // The easiest way to check if a peer was added manually is by using msg.src_address,
        // which is set to None when using the addPeers JSON-RPC method and also when reading peers from config
        if msg.src_address.is_none() {
            self.peers.remove_many_from_ice(&msg.addresses);
        }
        self.peers.add_to_new(msg.addresses, msg.src_address)
    }
}

/// Handler for ClearPeers message
impl Handler<ClearPeers> for PeersManager {
    type Result = Result<(), failure::Error>;

    fn handle(&mut self, _msg: ClearPeers, _: &mut Context<Self>) -> Self::Result {
        // Clear addresses address
        log::trace!("Clear all peer addresses from tried bucket");
        self.peers.clear_tried_bucket();
        log::trace!("Clear all peer addresses from new bucket");
        self.peers.clear_new_bucket();
        log::trace!("Clear all peer addresses from ice bucket");
        self.peers.clear_ice_bucket();

        Ok(())
    }
}

/// Handler for InitializePeers message
impl Handler<InitializePeers> for PeersManager {
    type Result = Result<(), failure::Error>;

    fn handle(&mut self, msg: InitializePeers, _: &mut Context<Self>) -> Self::Result {
        // Clear addresses address
        log::trace!("Clear all peer addresses from tried bucket");
        self.peers.clear_tried_bucket();
        log::trace!("Clear all peer addresses from new bucket");
        self.peers.clear_new_bucket();
        log::trace!("Clear all peer addresses from ice bucket");
        self.peers.clear_ice_bucket();

        // Initialize with config peers
        self.peers.add_to_new(msg.known_peers, None)?;

        Ok(())
    }
}

/// Handler for AddConsolidatedPeer message
impl Handler<AddConsolidatedPeer> for PeersManager {
    type Result = PeersSocketAddrResult;

    fn handle(&mut self, msg: AddConsolidatedPeer, _: &mut Context<Self>) -> Self::Result {
        // Insert address
        log::debug!(
            "Adding the following consolidated peer address: {:?}",
            msg.address
        );
        let current_ts = get_timestamp();

        let index = self.peers.tried_bucket_index(&msg.address);
        match self.peers.tried_bucket_get_timestamp(index) {
            Some(ts) if current_ts - ts < self.bucketing_update_period => {
                // It is recently updated
                Ok(None)
            }
            _ => self.peers.add_to_tried(msg.address, msg.version),
        }
    }
}

/// Handler for RemovePeers message
impl Handler<RemoveAddressesFromTried> for PeersManager {
    type Result = PeersSocketAddrsResult;

    fn handle(&mut self, msg: RemoveAddressesFromTried, _: &mut Context<Self>) -> Self::Result {
        log::debug!(
            "Removing the following addresses from `tried` buckets (if present): {:?}",
            msg.addresses
        );
        Ok(self.peers.remove_from_tried(&msg.addresses, msg.ice))
    }
}

/// Handler for GetRandomPeer message
impl Handler<GetRandomPeers> for PeersManager {
    type Result = PeersSocketAddrsResult;

    fn handle(&mut self, msg: GetRandomPeers, _: &mut Context<Self>) -> Self::Result {
        let result = self.peers.get_random_peers(msg.n);

        match result {
            Ok(peers) => {
                log::debug!("Selected random peer addresses: {:?}", peers);
                Ok(peers)
            }
            error => {
                log::error!("Error selecting a random peer address: {:?}", error);
                error
            }
        }
    }
}

/// Handler for RequestPeers message
impl Handler<RequestPeers> for PeersManager {
    type Result = PeersSocketAddrsResult;

    fn handle(&mut self, _msg: RequestPeers, _: &mut Context<Self>) -> Self::Result {
        log::debug!("Get all peers");
        self.peers.get_all_from_tried()
    }
}

/// Handler for GetKnownPeers message
impl Handler<GetKnownPeers> for PeersManager {
    type Result = Result<PeersNewTried, failure::Error>;

    fn handle(&mut self, _msg: GetKnownPeers, _: &mut Context<Self>) -> Self::Result {
        Ok(PeersNewTried {
            new: self.peers.get_all_from_new()?,
            tried: self.peers.get_all_from_tried_with_version()?,
        })
    }
}

/// Handler for GetUpdatedPeersPercentage
impl Handler<GetUpdatedPeersPercentage> for PeersManager {
    type Result = Result<u32, failure::Error>;

    fn handle(&mut self, msg: GetUpdatedPeersPercentage, _: &mut Context<Self>) -> Self::Result {
        Ok(self.peers.get_updated_percentage(msg.version))
    }
}

/// Handler for EpochNotification message
impl Handler<EpochNotification<()>> for PeersManager {
    type Result = ();

    fn handle(&mut self, _msg: EpochNotification<()>, _: &mut Context<Self>) -> Self::Result {
        // Simply set the `bootstrapped` flag to `true`, because epoch notifications are not sent
        // anyway before the network is bootstrapped
        self.peers.bootstrapped = true
    }
}
