use std::net::SocketAddr;
use std::time::Duration;

use actix::{
    Actor, ActorFuture, AsyncContext, Context, ContextFutureSpawner, Handler, Message, Supervised,
    System, SystemService, WrapFuture,
};
use log::{debug, error};

use crate::actors::config_manager::send_get_config_request;
use crate::actors::storage_keys::PEERS_KEY;
use crate::actors::storage_manager::{Get, Put, StorageManager};

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

impl PeersManager {
    /// Method to periodically persist peers into storage
    fn persist_peers(&self, ctx: &mut Context<Self>, storage_peers_period: Duration) {
        // Schedule the discovery_peers with a given period
        ctx.run_later(storage_peers_period, move |act, ctx| {
            // Get storage manager address
            let storage_manager_addr = System::current().registry().get::<StorageManager>();

            // Persist peers into storage. `AsyncContext::wait` registers
            // future within context, but context waits until this future resolves
            // before processing any other events.
            storage_manager_addr
                .send(Put::from_value(PEERS_KEY, &act.peers).unwrap())
                .into_actor(act)
                .then(|res, _act, _ctx| {
                    match res {
                        Ok(Ok(_)) => debug!("PeersManager successfully persist peers to storage"),
                        _ => {
                            debug!("Peers manager persist peers to storage failed");
                            // FIXME(#72): handle errors
                        }
                    }
                    actix::fut::ok(())
                })
                .wait(ctx);

            act.persist_peers(ctx, storage_peers_period);
        });
    }
}

/// Make actor from `PeersManager`
impl Actor for PeersManager {
    /// Every actor has to provide execution `Context` in which it can run.
    type Context = Context<Self>;

    fn started(&mut self, ctx: &mut Self::Context) {
        debug!("Peers Manager actor has been started!");

        // Send message to config manager and process response
        send_get_config_request(self, ctx, |act, ctx, config| {
            // Get known peers
            let known_peers: Vec<_> = config.connections.known_peers.iter().cloned().collect();

            // Get storage peers period
            let storage_peers_period = config.connections.storage_peers_period;

            // Add all peers
            debug!(
                "Adding the following peer addresses from config: {:?}",
                known_peers
            );
            match act.peers.add(known_peers) {
                Ok(_duplicated_peers) => {}
                Err(e) => error!("Error when adding peer addresses from config: {}", e),
            }

            // Add peers from storage:
            // Get storage manager actor address
            let storage_manager_addr = System::current().registry().get::<StorageManager>();
            storage_manager_addr
                // Send a message to read the peers from the storage
                .send(Get::<Peers>::new(PEERS_KEY))
                .into_actor(act)
                // Process the response
                .then(|res, _act, _ctx| match res {
                    Err(e) => {
                        // Error when sending message
                        debug!("Unsuccessful communication with config manager: {}", e);
                        actix::fut::err(())
                    }
                    Ok(res) => match res {
                        Err(e) => {
                            // Storage error
                            error!("Error while getting peers from storage: {}", e);
                            actix::fut::err(())
                        }
                        Ok(res) => actix::fut::ok(res),
                    },
                })
                .and_then(|peers_from_storage, act, _ctx| {
                    // peers_from_storage can be None if the storage does not contain that key
                    if let Some(peers_from_storage) = peers_from_storage {
                        // Add all the peers from storage
                        // The add method handles duplicates by overwriting the old values
                        let peers = peers_from_storage.get_all().unwrap();
                        debug!(
                            "Adding the following peer addresses from storage: {:?}",
                            peers
                        );
                        match act.peers.add(peers) {
                            Ok(_duplicated_peers) => {}
                            Err(e) => {
                                error!("Error when adding peer addresses from storage: {}", e);
                            }
                        }
                    }

                    actix::fut::ok(())
                })
                .wait(ctx);

            // Start the storage peers process on sessions manager start
            act.persist_peers(ctx, storage_peers_period);
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
