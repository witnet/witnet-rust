use std::{collections::HashSet, net::SocketAddr, sync::Arc, time::Duration};

use actix::{
    ActorFutureExt, Addr, AsyncContext, Context, ContextFutureSpawner, MailboxError, SystemService,
    WrapFuture,
};

use ansi_term::Color::Cyan;

use witnet_p2p::sessions::{SessionType, Sessions};

use self::beacons::Beacons;
use crate::actors::{
    chain_manager::ChainManager,
    connections_manager::ConnectionsManager,
    epoch_manager::EpochManager,
    messages::{
        Anycast, CloseSession, GetEpochConstants, GetRandomPeers, OutboundTcpConnect, PeersBeacons,
        PeersSocketAddrsResult, SendGetPeers, Subscribe,
    },
    peers_manager::PeersManager,
    session::Session,
};
use failure::Fail;
use witnet_config::config::Config;
use witnet_data_structures::{
    chain::{CheckpointBeacon, Epoch, EpochConstants},
    types::LastBeacon,
};

mod actor;
mod beacons;
mod handlers;

/// SessionsManager actor
#[derive(Default)]
pub struct SessionsManager {
    /// Registered Sessions
    sessions: Sessions<Addr<Session>>,
    /// List of beacons of outbound sessions
    beacons: Beacons,
    /// Constants used to calculate instants in time
    epoch_constants: Option<EpochConstants>,
    /// Current epoch
    current_epoch: Epoch,
    /// Current tip of the chain, used to check if outbound peers are in consensus when connecting
    /// Note that the sessions manager will not be able to create any sessions if this field is None
    last_beacon: Option<LastBeacon>,
    /// Logging message hashset
    logging_messages: HashSet<String>,
    /// Reference to config
    config: Option<Arc<Config>>,
    /// SuperBlock target.
    /// Last SuperBlock consensus achieved in the previous SuperEpoch by more than 2/3 of the signing
    /// committee when our node reached another superblock
    superblock_beacon_target: Option<CheckpointBeacon>,
}

#[derive(Debug, Fail)]
enum NotSendingPeersBeaconsBecause {
    #[fail(
        display = "Not sending PeersBeacons message because it was already sent during this epoch"
    )]
    AlreadySent,
    #[fail(
        display = "Not sending PeersBeacons message because of lack of peers (still bootstrapping)"
    )]
    BootstrapNeeded,
    #[fail(
        display = "Not sending PeersBeacons message because not enough peers sent their beacons"
    )]
    NotEnoughBeacons,
}

impl SessionsManager {
    /// Method to periodically bootstrap outbound Sessions
    fn bootstrap_peers(&self, ctx: &mut Context<Self>, bootstrap_peers_period: Duration) {
        // Schedule the bootstrap with a given period
        ctx.run_later(bootstrap_peers_period, move |act, ctx| {
            // Check if bootstrap is needed
            if act.last_beacon.is_some() && act.sessions.is_outbound_bootstrap_needed() {
                log::info!(
                    "{} Inbound: {} | Outbound: {}",
                    Cyan.bold().paint("[Sessions]"),
                    Cyan.bold()
                        .paint(act.sessions.get_num_inbound_sessions().to_string()),
                    Cyan.bold()
                        .paint(act.sessions.get_num_outbound_sessions().to_string())
                );
                log::trace!("{:#?}", act.sessions.show_ips());

                // Get peers manager address
                let peers_manager_addr = PeersManager::from_registry();

                // Start chain of actions
                peers_manager_addr
                    // Send GetPeer message to peers manager actor
                    // This returns a Request Future, representing an asynchronous message sending process
                    .send(GetRandomPeers {
                        n: usize::from(act.sessions.outbound_consolidated.limit.unwrap_or(1)),
                    })
                    // Convert a normal future into an ActorFuture
                    .into_actor(act)
                    // Process the response from the peers manager
                    // This returns a FutureResult containing the socket address if present
                    .then(|res, act, _ctx| {
                        // Process the response from peers manager
                        let addresses = act.process_get_peer_response(res);
                        log::debug!(
                            "Trying to create a new outbound connection to {:?}",
                            addresses
                        );

                        for address in addresses {
                            // Get ConnectionsManager from registry and send an OutboundTcpConnect message to it
                            let connections_manager_addr = ConnectionsManager::from_registry();
                            connections_manager_addr.do_send(OutboundTcpConnect {
                                address,
                                session_type: SessionType::Outbound,
                            });
                        }

                        actix::fut::ready(())
                    })
                    .wait(ctx);
            }
            // Reschedule the bootstrap peers task
            act.bootstrap_peers(ctx, bootstrap_peers_period);
        });
    }

    /// Method to periodically discover peers
    fn discovery_peers(&self, ctx: &mut Context<Self>, discovery_peers_period: Duration) {
        // Schedule the discovery_peers with a given period
        ctx.run_later(discovery_peers_period, move |act, ctx| {
            // Send Anycast(GetPeers) message
            ctx.notify(Anycast {
                command: SendGetPeers {},
                safu: false,
            });

            act.discovery_peers(ctx, discovery_peers_period);
        });
    }

    /// Method to process peers manager RequestPeer response
    fn process_get_peer_response(
        &mut self,
        response: Result<PeersSocketAddrsResult, MailboxError>,
    ) -> Vec<SocketAddr> {
        response
            // Unwrap the Result<PeersSocketAddrResult, MailboxError>
            .unwrap_or_else(|e| {
                log::error!("Failed to communicate with PeersManager: {}", e);
                Ok(vec![])
            })
            // Unwrap the PeersSocketAddrResult
            .unwrap_or_else(|e| {
                log::error!(
                    "Error when trying to get a peer address from PeersManager: {}",
                    e
                );
                vec![]
            })
            // Filter the result checking if outbound address is eligible as new peer
            .into_iter()
            .filter(|address| self.sessions.is_outbound_address_eligible(*address))
            // Take at most as many peers as missing outbounds
            .take(self.sessions.num_missing_outbound())
            .collect()
    }

    /// Subscribe to all future epochs
    fn subscribe_to_epoch_manager(&mut self, ctx: &mut Context<Self>) {
        // Get EpochManager address from registry
        let epoch_manager_addr = EpochManager::from_registry();

        // Subscribe to all epochs with an empty payload
        epoch_manager_addr.do_send(Subscribe::to_all(ctx.address(), ()));

        // Get epoch constants
        epoch_manager_addr
            .send(GetEpochConstants)
            .into_actor(self)
            .map(move |res, act, _ctx| match res {
                Ok(Some(f)) => act.epoch_constants = Some(f),
                Ok(None) => log::error!("Failed to get epoch constants"),
                Err(err) => log::error!("Failed to get epoch constants: {:?}", err),
            })
            .wait(ctx);
    }

    /// Check if we can send a PeersBeacons message, and if we can, send it
    fn try_send_peers_beacons(
        &mut self,
        ctx: &mut Context<Self>,
    ) -> Result<(), NotSendingPeersBeaconsBecause> {
        if self.beacons.already_sent() {
            return Err(NotSendingPeersBeaconsBecause::AlreadySent);
        }

        if self.sessions.is_outbound_bootstrap_needed() {
            // Do not send PeersBeacons until we get to the outbound limit
            return Err(NotSendingPeersBeaconsBecause::BootstrapNeeded);
        }

        // We may have 0 beacons out of 0
        // We actually want to check it against the outbound limit
        let expected_peers = self
            .sessions
            .outbound_consolidated
            .limit
            .map(|x| x as usize);
        if Some(self.beacons.total_count()) < expected_peers {
            return Err(NotSendingPeersBeaconsBecause::NotEnoughBeacons);
        }

        self.send_peers_beacons(ctx);

        Ok(())
    }

    /// Send PeersBeacons message to peers manager
    fn send_peers_beacons(&mut self, ctx: &mut Context<Self>) {
        let (pb, pnb) = match self.beacons.send() {
            Some(x) => x,
            None => {
                log::debug!("{}", NotSendingPeersBeaconsBecause::AlreadySent);
                return;
            }
        };

        log::debug!("Sending PeersBeacons message");
        let pb: Vec<_> = pb
            .iter()
            .map(|(k, v)| (*k, Some(v.clone())))
            .chain(pnb.iter().map(|k| (*k, None)))
            .collect();
        let mut peers_to_keep: HashSet<_> = pb.iter().map(|(k, _v)| *k).collect();
        let outbound_limit = self.sessions.outbound_consolidated.limit;

        ChainManager::from_registry()
            .send(PeersBeacons { pb, outbound_limit })
            .into_actor(self)
            .map(|res, act, _ctx| {
                match res {
                    Err(_e) => {
                        // Actix error, ignore
                    }
                    Ok(Err(())) => {
                        // Nothing to do, carry on
                    }
                    Ok(Ok(peers_to_unregister)) => {
                        if !peers_to_unregister.is_empty() {
                            log::debug!(
                                "Dropping {} peers that were out of consensus",
                                peers_to_unregister.len()
                            );
                        }
                        // Unregister peers out of consensus
                        act.drop_outbound_peers(peers_to_unregister.as_ref());
                        for peer in peers_to_unregister {
                            peers_to_keep.remove(&peer);
                        }
                        // Mark remaining peers as safu
                        for peer in peers_to_keep {
                            let _ = act.sessions.consensus_session(peer);
                        }
                    }
                }
            })
            .wait(ctx);
    }

    /// Clear the received beacons, and in the next epoch wait for beacons
    /// from all our outbound consolidated peers
    fn clear_beacons(&mut self) {
        self.beacons.clear(
            self.sessions
                .outbound_consolidated
                .collection
                .keys()
                .cloned(),
        );
    }

    /// Drop outbound peers
    fn drop_outbound_peers(&mut self, peers_to_unregister: &[SocketAddr]) {
        // Unregister peers out of consensus
        for peer in peers_to_unregister {
            if let Some(a) = self.sessions.outbound_consolidated.collection.get(&peer) {
                a.reference.do_send(CloseSession);
            }
        }
    }

    /// Drop all peers
    fn drop_all_peers(&mut self) {
        for (_peer, a) in self.sessions.inbound_consolidated.collection.iter() {
            a.reference.do_send(CloseSession);
        }
        for (_peer, a) in self.sessions.outbound_consolidated.collection.iter() {
            a.reference.do_send(CloseSession);
        }
    }
}

/// Required traits for being able to retrieve SessionsManager address from registry
impl actix::Supervised for SessionsManager {}

impl SystemService for SessionsManager {}
