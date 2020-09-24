use std::{
    fmt::{Debug, Display},
    marker::Send,
};

use actix::{
    io::FramedWrite, Actor, AsyncContext, Context, Handler, Message, ResponseFuture, StreamHandler,
    SystemService,
};
use ansi_term::Color::Cyan;
use futures::future::Future;
use tokio::{codec::FramedRead, io::AsyncRead};

use super::{NotSendingPeersBeaconsBecause, SessionsManager};
use crate::actors::{
    chain_manager::ChainManager,
    codec::P2PCodec,
    messages::{
        AddConsolidatedPeer, AddPeers, Anycast, Broadcast, Consolidate, Create, EpochNotification,
        GetConsolidatedPeers, LogMessage, NumSessions, NumSessionsResult, PeerBeacon, Register,
        RemoveAddressesFromTried, SessionsUnitResult, SetLastBeacon, TryMineBlock, Unregister,
    },
    peers_manager::PeersManager,
    session::Session,
};
use witnet_p2p::{
    error::SessionsError,
    sessions::{ip_range_string, SessionType},
};
use witnet_util::timestamp::{duration_until_timestamp, get_timestamp};

/// Handler for Create message.
impl Handler<Create> for SessionsManager {
    type Result = ();

    fn handle(&mut self, msg: Create, _ctx: &mut Context<Self>) {
        // Get server address
        let public_address = self.sessions.public_address;

        // Get magic number
        let magic_number = self.sessions.magic_number;

        // Get current epoch
        let current_epoch = self.current_epoch;

        let config = self.config.as_ref().expect("Config should be set");

        // Get last beacon
        let last_beacon = match self.last_beacon.as_ref() {
            Some(x) => x.clone(),
            None => {
                log::debug!("Cannot create session because last beacon is not set");
                return;
            }
        };

        // Get remote peer address
        let remote_addr = match msg.stream.peer_addr() {
            Ok(x) => x,
            Err(e) => {
                log::debug!(
                    "Cannot create session of type {:?}: {}",
                    msg.session_type,
                    e
                );
                return;
            }
        };

        // Refuse creating multiple inbound sessions for similar IP ranges
        // This is guarded once here and again when consolidating, just to mitigate a possible race
        // condition
        if config.connections.reject_sybil_inbounds && msg.session_type == SessionType::Inbound {
            if let Some(range) = self.sessions.is_similar_to_inbound_session(&remote_addr) {
                log::trace!("Refusing to accept {} as inbound peer because there is already an inbound session with another peer in IP range {}", remote_addr, ip_range_string(range));
                return;
            }
        };

        // Clone the reference to config
        let config = config.clone();

        // Create a Session actor
        Session::create(move |ctx| {
            // Get server address (if not present, send local address instead)
            let public_addr = public_address;

            // Split TCP stream into read and write parts
            let (r, w) = msg.stream.split();

            // Add stream in session actor from the read part of the tcp stream
            Session::add_stream(FramedRead::new(r, P2PCodec), ctx);

            // Create the session actor and store in its state the write part of the tcp stream
            Session::new(
                public_addr,
                remote_addr,
                msg.session_type,
                FramedWrite::new(w, P2PCodec, ctx),
                magic_number,
                current_epoch,
                last_beacon,
                config,
            )
        });
    }
}

/// Handler for Register message.
impl Handler<Register> for SessionsManager {
    type Result = SessionsUnitResult;

    fn handle(&mut self, msg: Register, _: &mut Context<Self>) -> Self::Result {
        // Call method register session from sessions library
        let result = self
            .sessions
            .register_session(msg.session_type, msg.address, msg.actor);

        match &result {
            Ok(_) => log::debug!(
                "Session (type {:?}) registered for peer {}",
                msg.session_type,
                msg.address
            ),
            Err(error @ SessionsError::AddressAlreadyRegistered)
            | Err(error @ SessionsError::MaxPeersReached) => log::debug!(
                "Error while registering peer {} (session type {:?}): {}",
                msg.address,
                msg.session_type,
                error
            ),
            Err(error) => log::error!(
                "Error while registering peer {} (session type {:?}): {}",
                msg.address,
                msg.session_type,
                error
            ),
        }

        result
    }
}

/// Handler for Unregister message.
impl Handler<Unregister> for SessionsManager {
    type Result = SessionsUnitResult;

    fn handle(&mut self, msg: Unregister, _: &mut Context<Self>) -> Self::Result {
        // First evaluate Feeler case
        if msg.session_type == SessionType::Feeler {
            // Feeler sessions should not be managed by `SessionsManager`
            Ok(())
        } else {
            // Call method register session from sessions library
            let result =
                self.sessions
                    .unregister_session(msg.session_type, msg.status, msg.address);

            match &result {
                Ok(_) => {
                    log::debug!(
                        "Session (type {:?}) unregistered for peer {}",
                        msg.session_type,
                        msg.address
                    );

                    if msg.session_type == SessionType::Outbound {
                        self.beacons.remove(&msg.address);

                        let peers_manager_addr = PeersManager::from_registry();
                        // Ice the peer that was unregistered
                        peers_manager_addr.do_send(RemoveAddressesFromTried {
                            // Use the address to which we connected to, not the public address reported by the peer
                            addresses: vec![msg.address],
                            ice: true,
                        });
                    }
                }
                Err(error @ SessionsError::AddressNotFound) => log::debug!(
                    "Error while unregistering peer {} (session type {:?}): {}",
                    msg.address,
                    msg.session_type,
                    error
                ),
                Err(error) => log::error!(
                    "Error while unregistering peer {} (session type {:?}): {}",
                    msg.address,
                    msg.session_type,
                    error
                ),
            }

            result
        }
    }
}

/// Handler for Consolidate message.
impl Handler<Consolidate> for SessionsManager {
    type Result = SessionsUnitResult;

    fn handle(&mut self, msg: Consolidate, _: &mut Context<Self>) -> Self::Result {
        // Call method register session from sessions library
        let result = self
            .sessions
            .consolidate_session(msg.session_type, msg.address);

        // Get peers manager address
        let peers_manager_addr = PeersManager::from_registry();

        if msg.session_type == SessionType::Outbound {
            // Send AddConsolidatedPeer message to the peers manager
            // Try to add this potential peer in the tried addresses bucket
            peers_manager_addr.do_send(AddConsolidatedPeer {
                address: msg.address,
            });
        }

        // Send AddPeers message to the peers manager
        // Try to add this potential peer in the new addresses bucket
        if let Some(potential_new_peer) = msg.potential_new_peer {
            peers_manager_addr.do_send(AddPeers {
                addresses: vec![potential_new_peer],
                src_address: Some(msg.address),
            });
        }

        match &result {
            Ok(_) => {
                log::debug!(
                    "Established a consolidated {:?} session with the peer at {}",
                    msg.session_type,
                    msg.address
                );
                if msg.session_type == SessionType::Outbound {
                    // Add outbound peer to the list of peers that should send us a beacon
                    self.beacons.also_wait_for(msg.address);
                }
            }
            Err(error @ SessionsError::AddressAlreadyRegistered)
            | Err(error @ SessionsError::AddressInSameRangeAlreadyRegistered { .. })
            | Err(error @ SessionsError::MaxPeersReached) => log::debug!(
                "Error while consolidating {:?} session with the peer at {}: {:?}",
                msg.session_type,
                msg.address,
                error
            ),
            Err(error) => log::error!(
                "Error while consolidating {:?} session with the peer at {}: {:?}",
                msg.session_type,
                msg.address,
                error
            ),
        }

        result
    }
}

/// Handler for Anycast message
impl<T: 'static> Handler<Anycast<T>> for SessionsManager
where
    T: Message + Send + Debug + Display,
    T::Result: Send,
    Session: Handler<T>,
{
    type Result = ResponseFuture<T::Result, ()>;

    fn handle(&mut self, msg: Anycast<T>, _ctx: &mut Context<Self>) -> Self::Result {
        log::debug!(
            "A {} message is now being forwarded to a random session",
            msg.command
        );

        // Request a random consolidated outbound session
        self.sessions
            .get_random_anycast_session(msg.safu)
            .map(|session_addr| {
                // Send message to session and await for response
                session_addr
                    // Send SendMessage message to session actor
                    // This returns a Request Future, representing an asynchronous message sending process
                    .send(msg.command)
                    .map_err(|e| {
                        log::error!("Anycast error: {}", e);
                    })
            })
            .map(|fut| {
                let b: Box<dyn Future<Item = T::Result, Error = ()>> = Box::new(fut);
                b
            })
            .unwrap_or_else(|| {
                log::warn!("No consolidated outbound session was found");
                Box::new(futures::future::err(()))
            })
    }
}

/// Handler for Broadcast message
impl<T: 'static> Handler<Broadcast<T>> for SessionsManager
where
    T: Clone + Message + Send + Display,
    T::Result: Send,
    Session: Handler<T>,
{
    type Result = ();

    fn handle(&mut self, msg: Broadcast<T>, _ctx: &mut Context<Self>) {
        log::trace!(
            "A Broadcast<{}> message is now being forwarded to all sessions",
            msg.command
        );

        if msg.only_inbound {
            self.sessions
                .get_consolidated_inbound_sessions()
                .for_each(|session_addr| {
                    // Send message to session and ignore errors
                    session_addr.do_send(msg.command.clone());
                });
        } else {
            self.sessions
                .get_all_consolidated_sessions()
                .for_each(|session_addr| {
                    // Send message to session and ignore errors
                    session_addr.do_send(msg.command.clone());
                });
        }
    }
}

impl Handler<EpochNotification<()>> for SessionsManager {
    type Result = ();

    fn handle(&mut self, msg: EpochNotification<()>, ctx: &mut Context<Self>) {
        log::debug!("Periodic epoch notification received {:?}", msg.checkpoint);
        let current_timestamp = get_timestamp();
        log::debug!(
            "Timestamp diff: {}, Epoch timestamp: {}. Current timestamp: {}",
            current_timestamp as i64 - msg.timestamp as i64,
            msg.timestamp,
            current_timestamp
        );

        log::info!(
            "{} Inbound: {} | Outbound: {}",
            Cyan.bold().paint("[Sessions]"),
            Cyan.bold()
                .paint(self.sessions.get_num_inbound_sessions().to_string()),
            Cyan.bold()
                .paint(self.sessions.get_num_outbound_sessions().to_string())
        );
        log::trace!("{:#?}", self.sessions.show_ips());

        // Clear the logging hashset
        self.logging_messages.clear();

        self.current_epoch = msg.checkpoint;

        self.beacons.new_epoch();
        // If for some reason we already have all the beacons, send message to ChainManager
        match self.try_send_peers_beacons(ctx) {
            Ok(()) => {}
            Err(NotSendingPeersBeaconsBecause::NotEnoughBeacons) => {}
            Err(e) => log::debug!("{}", e),
        }

        // Set timeout for receiving beacons
        // This timeout is also used to trigger block mining
        let timestamp_mining = self
            .epoch_constants
            .unwrap()
            .block_mining_timestamp(msg.checkpoint)
            .unwrap();
        let duration_until_mining = if let Some(d) = duration_until_timestamp(timestamp_mining, 0) {
            d
        } else {
            let timestamp_now = get_timestamp();
            let delay = timestamp_now - timestamp_mining;
            if delay < 0 {
                log::error!("Time went backwards");
            } else if msg.checkpoint > 0 {
                log::warn!(
                    "Epoch notification received too late, not sending beacons to ChainManager and not mining until next epoch"
                );
            }

            return;
        };

        ctx.run_later(duration_until_mining, move |act, ctx| {
            // If some peers sent us beacons, but not all of them, the peers beacons message will be sent now
            match act.try_send_peers_beacons(ctx) {
                Ok(_) => {}
                Err(NotSendingPeersBeaconsBecause::AlreadySent) => {}
                Err(NotSendingPeersBeaconsBecause::BootstrapNeeded) => {
                    // If the number of peers is less than the outbound limit, send the message
                    // and try to calculate the consensus by counting missing peers as "NO BEACON"
                    act.send_peers_beacons(ctx);
                }
                Err(NotSendingPeersBeaconsBecause::NotEnoughBeacons) => {
                    // Send it even if it is incomplete, and unregister the peers which have not sent a beacon
                    act.send_peers_beacons(ctx);
                }
            }

            // From this moment, all the received beacons are assumed to be for the next epoch
            // This fixes a race condition where sometimes we receive a beacon just before the epoch checkpoint
            act.clear_beacons();
            if msg.checkpoint > 0 {
                ChainManager::from_registry().do_send(TryMineBlock);
            }
        });
    }
}

impl Handler<PeerBeacon> for SessionsManager {
    type Result = ();

    fn handle(&mut self, msg: PeerBeacon, ctx: &mut Context<Self>) {
        self.beacons.insert(msg.address, msg.beacon);

        // Check if we have all the beacons, and sent PeersBeacons message to ChainManager
        match self.try_send_peers_beacons(ctx) {
            Ok(()) => {}
            Err(NotSendingPeersBeaconsBecause::NotEnoughBeacons) => {}
            Err(e) => log::debug!("{}", e),
        }
    }
}

impl Handler<NumSessions> for SessionsManager {
    type Result = <NumSessions as Message>::Result;

    fn handle(&mut self, _msg: NumSessions, _ctx: &mut Context<Self>) -> Self::Result {
        Ok(NumSessionsResult {
            inbound: self.sessions.get_num_inbound_sessions(),
            outbound: self.sessions.get_num_outbound_sessions(),
        })
    }
}

impl Handler<GetConsolidatedPeers> for SessionsManager {
    type Result = <GetConsolidatedPeers as Message>::Result;

    fn handle(&mut self, _msg: GetConsolidatedPeers, _ctx: &mut Context<Self>) -> Self::Result {
        Ok(self.sessions.get_consolidated_sessions_addr())
    }
}

impl Handler<LogMessage> for SessionsManager {
    type Result = SessionsUnitResult;

    fn handle(&mut self, msg: LogMessage, _ctx: &mut Context<Self>) -> Self::Result {
        if !self.logging_messages.contains(&msg.log_data) {
            log::debug!("{}", msg.log_data);
            self.logging_messages.insert(msg.log_data);
        }

        Ok(())
    }
}

impl Handler<SetLastBeacon> for SessionsManager {
    type Result = ();

    fn handle(&mut self, msg: SetLastBeacon, _ctx: &mut Context<Self>) -> Self::Result {
        self.last_beacon = Some(msg.beacon);
    }
}
