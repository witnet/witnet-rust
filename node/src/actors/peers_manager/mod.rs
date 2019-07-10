use std::time::Duration;

use actix::prelude::*;
use actix::{
    ActorFuture, AsyncContext, Context, ContextFutureSpawner, Supervised, SystemService, WrapFuture,
};

use log::{debug, error};

use crate::actors::storage_keys::PEERS_KEY;
use crate::storage_mngr;
use witnet_p2p::peers::Peers;

// Internal Actor implementation for PeersManager
mod actor;

/// Handlers to manage the previous messages using the `peers` library:
/// * Add peers
/// * Remove peers
/// * Get random peer
/// * Get all peers
mod handlers;

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
    /// Period to consider if a peer is updated
    pub bucketing_update_period: i64,
    /// Timeout for handshake
    pub handshake_timeout: Duration,
}

impl PeersManager {
    /// Method to periodically persist peers into storage
    fn persist_peers(&self, ctx: &mut Context<Self>, storage_peers_period: Duration) {
        // Schedule the discovery_peers with a given period
        ctx.run_later(storage_peers_period, move |act, ctx| {
            storage_mngr::put(&PEERS_KEY, &act.peers)
                .into_actor(act)
                .and_then(|_, _, _| {
                    debug!("PeersManager successfully persisted peers to storage");
                    fut::ok(())
                })
                .map_err(|err, _, _| {
                    error!("Peers manager persist peers to storage failed: {}", err)
                })
                .spawn(ctx);

            act.persist_peers(ctx, storage_peers_period);
        });
    }

    fn import_peers(&mut self, peers: Peers) {
        self.peers = peers;
    }
}

/// Required traits for being able to retrieve SessionsManager address from registry
impl Supervised for PeersManager {}
impl SystemService for PeersManager {}
