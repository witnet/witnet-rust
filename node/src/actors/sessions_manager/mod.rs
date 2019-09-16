use log::{debug, error, info, trace, warn};
use std::{net::SocketAddr, time::Duration};

use actix::{
    fut::FutureResult, ActorFuture, Addr, AsyncContext, Context, ContextFutureSpawner, Handler,
    MailboxError, Message, System, SystemService, WrapFuture,
};

use ansi_term::Color::Cyan;

use witnet_p2p::sessions::Sessions;

use crate::actors::{
    chain_manager::ChainManager,
    connections_manager::ConnectionsManager,
    epoch_manager::EpochManager,
    messages::{
        Anycast, CloseSession, GetRandomPeer, OutboundTcpConnect, PeersBeacons,
        PeersSocketAddrResult, SendGetPeers, Subscribe,
    },
    peers_manager::PeersManager,
    session::Session,
};
use std::collections::{HashMap, HashSet};
use witnet_data_structures::chain::CheckpointBeacon;

mod actor;
mod handlers;

/// SessionsManager actor
#[derive(Default)]
pub struct SessionsManager {
    // Registered Sessions
    sessions: Sessions<Addr<Session>>,
    // List of beacons of outbound sessions
    beacons: HashMap<SocketAddr, Option<CheckpointBeacon>>,
}

impl SessionsManager {
    /// Method to periodically bootstrap outbound Sessions
    fn bootstrap_peers(&self, ctx: &mut Context<Self>, bootstrap_peers_period: Duration) {
        // Schedule the bootstrap with a given period
        ctx.run_later(bootstrap_peers_period, move |act, ctx| {
            info!(
                "{} Inbound: {} | Outbound: {}",
                Cyan.bold().paint("[Sessions]"),
                Cyan.bold()
                    .paint(act.sessions.get_num_inbound_sessions().to_string()),
                Cyan.bold()
                    .paint(act.sessions.get_num_outbound_sessions().to_string())
            );
            trace!("{:#?}", act.sessions.show_ips());

            // Check if bootstrap is needed
            if act.sessions.is_outbound_bootstrap_needed() {
                // Get peers manager address
                let peers_manager_addr = System::current().registry().get::<PeersManager>();

                // Start chain of actions
                peers_manager_addr
                    // Send GetPeer message to peers manager actor
                    // This returns a Request Future, representing an asynchronous message sending process
                    .send(GetRandomPeer)
                    // Convert a normal future into an ActorFuture
                    .into_actor(act)
                    // Process the response from the peers manager
                    // This returns a FutureResult containing the socket address if present
                    .then(|res, act, _ctx| {
                        // Process the response from peers manager
                        act.process_get_peer_response(res)
                    })
                    // Process the socket address received
                    // This returns a FutureResult containing a success or error
                    .and_then(|address, _act, _ctx| {
                        debug!("Trying to create a new outbound connection to {}", address);

                        // Get ConnectionsManager from registry and send an OutboundTcpConnect message to it
                        let connections_manager_addr =
                            System::current().registry().get::<ConnectionsManager>();
                        connections_manager_addr.do_send(OutboundTcpConnect {
                            address,
                            feeler: false,
                        });

                        actix::fut::ok(())
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
        response: Result<PeersSocketAddrResult, MailboxError>,
    ) -> FutureResult<SocketAddr, (), Self> {
        response
            // Unwrap the Result<PeersSocketAddrResult, MailboxError>
            .unwrap_or_else(|_| {
                error!("Failed to communicate with PeersManager");
                Ok(None)
            })
            // Unwrap the PeersSocketAddrResult
            .unwrap_or_else(|_| {
                error!("Error when trying to get a peer address from PeersManager");
                None
            })
            // Check if PeersSocketAddrResult returned `None`
            .or_else(|| {
                warn!("Did not obtain any peer addresses from PeersManager");
                None
            })
            // Filter the result checking if outbound address is eligible as new peer
            .filter(|address: &SocketAddr| {
                self.sessions.is_outbound_address_eligible(address.clone())
            })
            // Check if there is a peer after filter
            .or_else(|| {
                debug!(
                    "The peer address obtained from PeersManager is not eligible for a new session"
                );
                None
            })
            // Convert Some(SocketAddr) or None to FutureResult<SocketAddr, (), Self>
            .map(actix::fut::ok)
            .unwrap_or_else(|| actix::fut::err(()))
    }

    /// Method to process Session SendMessage response
    fn process_command_response<T>(
        &mut self,
        response: &Result<T::Result, MailboxError>,
    ) -> FutureResult<(), (), Self>
    where
        T: Message,
        Session: Handler<T>,
    {
        match response {
            Ok(_) => actix::fut::ok(()),
            Err(_) => actix::fut::err(()),
        }
    }

    /// Subscribe to all future epochs
    fn subscribe_to_epoch_manager(&mut self, ctx: &mut Context<Self>) {
        // Get EpochManager address from registry
        let epoch_manager_addr = System::current().registry().get::<EpochManager>();

        // Subscribe to all epochs with an empty payload
        epoch_manager_addr.do_send(Subscribe::to_all(ctx.address(), ()));
    }

    fn send_peers_beacons(&mut self, ctx: &mut Context<Self>) {
        if self.sessions.is_outbound_bootstrap_needed() {
            // Do not send PeersBeacons until we get to the outbound limit
            debug!("PeersBeacons message delayed because of lack of peers");
            return;
        }

        debug!("Sending PeersBeacons message");
        // Send message to peers manager
        // Peers which did not send a beacon will be ignored: neither unregistered nor promoted to safu
        let pb: Vec<_> = self
            .beacons
            .iter()
            .filter_map(|(k, v)| v.map(|v| (*k, v)))
            .collect();

        if self
            .sessions
            .outbound_consolidated
            .limit
            .map(|limit| pb.len() < limit as usize)
            .unwrap_or(true)
        {
            debug!("PeersBeacons message delayed because not enough peers sent their beacons");
            return;
        }

        let mut peers_to_keep: HashSet<_> = pb.iter().map(|(p, _b)| *p).collect();
        ChainManager::from_registry()
            .send(PeersBeacons { pb })
            .into_actor(self)
            .then(|res, act, _ctx| {
                match res {
                    Err(_e) => {
                        // Actix error, ignore
                    }
                    Ok(Err(())) => {
                        // Nothing to do, carry on
                    }
                    Ok(Ok(peers_to_unregister)) => {
                        // Unregister peers out of consensus
                        for peer in peers_to_unregister {
                            if let Some(a) =
                                act.sessions.outbound_consolidated.collection.get(&peer)
                            {
                                a.reference.do_send(CloseSession);
                            }
                            peers_to_keep.remove(&peer);
                        }
                        // Mark remaining peers as safu
                        for peer in peers_to_keep {
                            match act.sessions.consensus_session(peer) {
                                _ => {}
                            }
                        }
                    }
                }

                actix::fut::ok(())
            })
            .wait(ctx);
    }

    fn clear_beacons(&mut self) {
        self.beacons.clear();
        for socket_addr in self.sessions.outbound_consolidated.collection.keys() {
            self.beacons.insert(*socket_addr, None);
        }
    }
}

/// Required traits for being able to retrieve SessionsManager address from registry
impl actix::Supervised for SessionsManager {}

impl SystemService for SessionsManager {}
