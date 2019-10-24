use std::{
    fmt::{Debug, Display},
    marker::Send,
};

use actix::{
    io::FramedWrite, Actor, ActorFuture, Context, ContextFutureSpawner, Handler, Message,
    StreamHandler, SystemService, WrapFuture,
};
use ansi_term::Color::Cyan;
use log::{debug, error, info, trace, warn};
use tokio::{codec::FramedRead, io::AsyncRead};

use super::SessionsManager;
use crate::actors::messages::AddPeers;
use crate::actors::{
    codec::P2PCodec,
    messages::{
        AddConsolidatedPeer, Anycast, Broadcast, Consolidate, Create, EpochNotification,
        NumSessions, NumSessionsResult, PeerBeacon, Register, SessionsUnitResult, Unregister,
    },
    peers_manager::PeersManager,
    session::Session,
};
use witnet_p2p::sessions::SessionType;

/// Handler for Create message.
impl Handler<Create> for SessionsManager {
    type Result = ();

    fn handle(&mut self, msg: Create, _ctx: &mut Context<Self>) {
        // Get handshake timeout
        let handshake_timeout = self.sessions.handshake_timeout;

        // Get server address
        let server_addr = self.sessions.server_address;

        // Get magic number
        let magic_number = self.sessions.magic_number;

        // Get blocks timeout
        let blocks_timeout = self.sessions.blocks_timeout;

        // Create a Session actor
        Session::create(move |ctx| {
            // Get server address (if not present, send local address instead)
            let server_addr = server_addr.unwrap_or_else(|| msg.stream.local_addr().unwrap());

            // Get remote peer address
            let remote_addr = msg.stream.peer_addr().unwrap();

            // Split TCP stream into read and write parts
            let (r, w) = msg.stream.split();

            // Add stream in session actor from the read part of the tcp stream
            Session::add_stream(FramedRead::new(r, P2PCodec), ctx);

            // Create the session actor and store in its state the write part of the tcp stream
            Session::new(
                server_addr,
                remote_addr,
                msg.session_type,
                FramedWrite::new(w, P2PCodec, ctx),
                handshake_timeout,
                magic_number,
                blocks_timeout,
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
            Ok(_) => debug!(
                "Session (type {:?}) registered for peer {}",
                msg.session_type, msg.address
            ),
            Err(error) => error!(
                "Error while registering peer {} (session type {:?}): {}",
                msg.address, msg.session_type, error
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
                Ok(_) => debug!(
                    "Session (type {:?}) unregistered for peer {}",
                    msg.session_type, msg.address
                ),
                Err(error) => error!(
                    "Error while unregistering peer {} (session type {:?}): {}",
                    msg.address, msg.session_type, error
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
                address: msg.potential_new_peer,
            });
        } else if msg.session_type == SessionType::Inbound {
            // Send AddPeers message to the peers manager
            // Try to add this potential peer in the new addresses bucket
            peers_manager_addr.do_send(AddPeers {
                addresses: vec![msg.potential_new_peer],
                src_address: msg.address,
            });
        }

        match &result {
            Ok(_) => debug!(
                "Established a consolidated {:?} session with the peer at {}",
                msg.session_type, msg.address
            ),
            Err(error) => error!(
                "Error while consolidating {:?} session with the peer at {}: {:?}",
                msg.session_type, msg.address, error
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
    type Result = ();

    fn handle(&mut self, msg: Anycast<T>, ctx: &mut Context<Self>) {
        debug!(
            "An Anycast<{}> message is now being forwarded to a random session",
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
                    // Convert a normal future into an ActorFuture
                    .into_actor(self)
                    // Process the response from the session
                    // This returns a FutureResult containing the socket address if present
                    .then(|res, act, _ctx| {
                        // Process the response from session
                        act.process_command_response(&res)
                    })
                    .wait(ctx);
            })
            .unwrap_or_else(|| {
                warn!("No consolidated outbound session was found");
            });
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
        debug!(
            "A Broadcast<{}> message is now being forwarded to all sessions",
            msg.command
        );

        if msg.only_inbound {
            self.sessions
                .get_all_consolidated_inbound_sessions()
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

    fn handle(&mut self, _msg: EpochNotification<()>, ctx: &mut Context<Self>) {
        let all_ready_before = self.beacons.iter().all(|(_k, v)| v.is_some());
        if !all_ready_before {
            // Some peers sent us beacons, but not all of them
            self.send_peers_beacons(ctx);
        }
        // New epoch, new beacons
        // There is a race condition here: we must receive the beacons after this handler has
        // been executed. We could avoid this by only clearing beacons from past epochs, and
        // accepting beacons for future epochs, but that would add complexity.
        self.clear_beacons();

        info!(
            "{} Inbound: {} | Outbound: {}",
            Cyan.bold().paint("[Sessions]"),
            Cyan.bold()
                .paint(self.sessions.get_num_inbound_sessions().to_string()),
            Cyan.bold()
                .paint(self.sessions.get_num_outbound_sessions().to_string())
        );
        trace!("{:#?}", self.sessions.show_ips());
    }
}

impl Handler<PeerBeacon> for SessionsManager {
    type Result = ();

    fn handle(&mut self, msg: PeerBeacon, ctx: &mut Context<Self>) {
        let all_ready_before = self.beacons.iter().all(|(_k, v)| v.is_some());
        if all_ready_before {
            // We already got all the beacons for this epoch, do nothing
            return;
        }

        if let Some(x) = self.beacons.get_mut(&msg.address) {
            *x = Some(msg.beacon);
        }

        let all_ready_after = self.beacons.iter().all(|(_k, v)| v.is_some());

        if !all_ready_before && all_ready_after {
            self.send_peers_beacons(ctx);
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
