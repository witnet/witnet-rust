use std::{net::SocketAddr, time::Duration};

use actix::{
    ActorFuture, AsyncContext, Context, ContextFutureSpawner, Supervised, SystemService, WrapFuture,
};

use witnet_p2p::{peers::Peers, sessions::SessionType};
use witnet_util::timestamp::get_timestamp;

use crate::{
    actors::{
        connections_manager::ConnectionsManager,
        messages::{OutboundTcpConnect, RemoveAddressesFromTried},
        storage_keys,
    },
    storage_mngr,
};
use witnet_config::config::Config;

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
    /// Period in seconds for checking melted peers in the "ice" bucket
    pub check_melted_peers_period: Duration,
    /// Magic number from ConsensusConstants
    magic: u16,
}

impl PeersManager {
    /// Initialize `PeersManager` taking the configuration from a `Config` structure
    pub fn from_config(config: &Config) -> Self {
        PeersManager {
            peers: Peers::from_config(config),
            bucketing_update_period: config.connections.bucketing_update_period,
            check_melted_peers_period: config.connections.check_melted_peers_period,
            magic: config.consensus_constants.get_magic(),
        }
    }

    /// Method to periodically persist peers into storage
    fn persist_peers(&self, ctx: &mut Context<Self>, storage_peers_period: Duration) {
        // Schedule the discovery_peers with a given period
        ctx.run_later(storage_peers_period, move |act, ctx| {
            storage_mngr::put(&storage_keys::peers_key(act.get_magic()), &act.peers)
                .into_actor(act)
                .map(|res, _act, _ctx| match res {
                    Ok(_) => log::trace!("PeersManager successfully persisted peers to storage"),
                    Err(err) => {
                        log::error!("Peers manager persist peers to storage failed: {}", err)
                    }
                })
                .spawn(ctx);

            act.persist_peers(ctx, storage_peers_period);
        });
    }

    fn import_peers(&mut self, peers: Peers, known_peers: Vec<SocketAddr>) {
        self.peers = peers;

        match self.peers.add_to_new(known_peers, None) {
            Ok(_duplicated_peers) => {}
            Err(e) => log::error!("Error when adding peer addresses from config: {}", e),
        }
    }

    /// Method to try a peer before to insert in the tried addresses bucket
    pub fn try_peer(&mut self, _ctx: &mut Context<Self>, address: SocketAddr) {
        let connections_manager_addr = ConnectionsManager::from_registry();
        let current_ts = get_timestamp();

        let index = self.peers.tried_bucket_index(&address);
        match self.peers.tried_bucket_get_timestamp(index) {
            None => {
                // Empty slot, try new peer
                log::debug!("Trying new address {} ", address);
                connections_manager_addr.do_send(OutboundTcpConnect {
                    address,
                    session_type: SessionType::Feeler,
                });
            }
            Some(ts) if current_ts - ts > self.bucketing_update_period => {
                // No empty slot, first try the old one
                let old_address = self.peers.tried_bucket_get_address(index).unwrap();

                // Try a connection with the old address
                log::debug!("Trying old address {} ", address);
                connections_manager_addr.do_send(OutboundTcpConnect {
                    address: old_address,
                    session_type: SessionType::Feeler,
                });

                // Remove from tried bucket (in case of old address is ok, it will be
                // added again, in the other case the slot will be free to accept the new one)
                self.peers.remove_from_tried(&[old_address], false);
            }
            // Case peer updated recently ( do nothing )
            _ => {}
        }
    }

    /// Remove a peer address from the `tried` buckets if present, and optionally ice the removed
    /// addresses
    pub fn remove_address_from_tried(address: &SocketAddr, ice: bool) {
        let peers_manager_addr = PeersManager::from_registry();

        peers_manager_addr.do_send(RemoveAddressesFromTried {
            addresses: vec![*address],
            ice,
        });
    }

    /// Method to try peers periodically to move peers from new to tried
    pub fn feeler(&mut self, ctx: &mut Context<Self>, feeler_peers_period: Duration) {
        // Schedule the discovery_peers with a given period
        ctx.run_later(feeler_peers_period, move |act, ctx| {
            if let Some((key, peer)) = act.peers.get_new_random_peer() {
                act.peers.remove_from_new_with_index(&[key]);
                act.try_peer(ctx, peer);
            }
            act.feeler(ctx, feeler_peers_period);
        });
    }

    /// Retrieve current check_melted_peers period
    pub fn current_check_melted_peers_period(&self) -> Duration {
        if self.peers.bootstrapped {
            self.check_melted_peers_period
        } else {
            Duration::from_secs(60)
        }
    }

    /// Method to periodically melt peers
    fn melt_peers(&self, ctx: &mut Context<Self>) {
        // Schedule the melt_peers with a given period
        let check_melted_peers_period = self.current_check_melted_peers_period();
        ctx.run_later(check_melted_peers_period, move |act, ctx| {
            // Remove peers from `ice` bucket that have "melted", i.e. they have been in that bucket
            // for as long as specified by the `connections.bucketing_ice_period_seconds` setting in
            // the configuration. These peers will be added to `new` again, so as to ensure that
            // they never get stuck in the `ice` bucket
            let addresses = act.peers.extract_melted_peers_from_ice_bucket();
            if !addresses.is_empty() {
                log::debug!("Melting these addresses: {:?}", addresses);
                let _res = act.peers.add_to_new(addresses, None);
            }

            act.melt_peers(ctx);
        });
    }

    /// Set Magic number
    pub fn set_magic(&mut self, new_magic: u16) {
        self.magic = new_magic;
    }

    /// Get Magic number
    pub fn get_magic(&self) -> u16 {
        self.magic
    }
}

/// Required traits for being able to retrieve SessionsManager address from registry
impl Supervised for PeersManager {}
impl SystemService for PeersManager {}
