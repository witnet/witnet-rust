use std::time::Duration;

use actix::{
    ActorFuture, AsyncContext, Context, ContextFutureSpawner, Supervised, System, SystemService,
    WrapFuture,
};
use crate::actors::{
    storage_keys::PEERS_KEY,
    storage_manager::{messages::Put, StorageManager},
};
use log::{error, info};

use witnet_p2p::peers::Peers;

// Internal Actor implementation for PeersManager
mod actor;

/// Handlers to manage the previous messages using the `peers` library:
/// * Add peers
/// * Remove peers
/// * Get random peer
/// * Get all peers
mod handlers;

/// Messages for peer management:
/// * Add peers
/// * Remove peers
/// * Get random peer
/// * Get all peers
pub mod messages;

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
            // Get StorageManager address
            let storage_manager_addr = System::current().registry().get::<StorageManager>();

            // Persist peers into storage. `AsyncContext::wait` registers
            // future within context, but context waits until this future resolves
            // before processing any other events.
            storage_manager_addr
                .send(Put::from_value(PEERS_KEY, &act.peers).unwrap())
                .into_actor(act)
                .then(|res, _act, _ctx| {
                    match res {
                        Ok(Ok(_)) => info!("PeersManager successfully persist peers to storage"),
                        _ => {
                            error!("Peers manager persist peers to storage failed");
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

/// Required traits for being able to retrieve SessionsManager address from registry
impl Supervised for PeersManager {}
impl SystemService for PeersManager {}
