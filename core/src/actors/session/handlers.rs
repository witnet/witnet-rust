use std::io::Error;

use actix::io::WriteHandler;
use actix::{
    ActorContext, ActorFuture, Context, ContextFutureSpawner, Handler, StreamHandler, System,
    WrapFuture,
};

use log::{debug, error, info, warn};

use crate::actors::{
    codec::BytesMut,
    peers_manager,
    sessions_manager::{messages::Consolidate, SessionsManager},
};

use witnet_data_structures::{
    builders::from_address,
    serializers::TryFrom,
    types::{Address, Command, Message as WitnetMessage},
};
use witnet_p2p::sessions::SessionStatus;

use super::{
    messages::{GetPeers, SessionUnitResult},
    Session,
};

/// Implement WriteHandler for Session
impl WriteHandler<Error> for Session {}

/// Implement `StreamHandler` trait in order to use `Framed` with an actor
impl StreamHandler<BytesMut, Error> for Session {
    /// This is main event loop for client requests
    fn handle(&mut self, bytes: BytesMut, ctx: &mut Self::Context) {
        let result = WitnetMessage::try_from(bytes.to_vec());
        match result {
            Err(err) => error!("Error decoding message: {:?}", err),
            Ok(msg) => {
                info!(
                    "<----- Session ({}) received message: {}",
                    self.remote_addr, msg.kind
                );
                match (self.status, msg.kind) {
                    ////////////////////
                    //   HANDSHAKE    //
                    ////////////////////
                    (SessionStatus::Unconsolidated, Command::Version { sender_address, .. }) => {
                        let msgs = handshake_version(self, &sender_address);
                        for msg in msgs {
                            self.send_message(msg);
                        }
                        try_consolidate_session(self, ctx);
                    }
                    (SessionStatus::Unconsolidated, Command::Verack) => {
                        handshake_verack(self);
                        try_consolidate_session(self, ctx);
                    }
                    ////////////////////
                    // PEER DISCOVERY //
                    ////////////////////
                    (SessionStatus::Consolidated, Command::GetPeers) => {
                        peer_discovery_get_peers(self, ctx);
                    }
                    (SessionStatus::Consolidated, Command::Peers { peers }) => {
                        peer_discovery_peers(&peers);
                    }
                    /////////////////////
                    // NOT IMPLEMENTED //
                    /////////////////////
                    (SessionStatus::Consolidated, _) => {
                        warn!("Not implemented message command received!");
                    }
                    (_, kind) => {
                        warn!(
                            "Received a message of kind \"{:?}\", which is not implemented yet",
                            kind
                        );
                    }
                };
            }
        }
    }
}

/// Handler for GetPeers message (sent by other actors)
impl Handler<GetPeers> for Session {
    type Result = SessionUnitResult;

    fn handle(&mut self, _msg: GetPeers, _: &mut Context<Self>) {
        debug!("GetPeers message should be sent through the network");
        // Create get peers message
        let get_peers_msg = WitnetMessage::build_get_peers();
        // Write get peers message in session
        self.send_message(get_peers_msg);
    }
}

/// Function to try to consolidate session if handshake conditions are met
fn try_consolidate_session(session: &mut Session, ctx: &mut Context<Session>) {
    // Check if HandshakeFlags are all set to true
    if session.handshake_flags.all_true() && session.remote_sender_addr.is_some() {
        // Update session to consolidate status
        update_consolidate(session, ctx);
    }
}

// Function to notify the SessionsManager that the session has been consolidated
fn update_consolidate(session: &Session, ctx: &mut Context<Session>) {
    // Get session manager address
    let session_manager_addr = System::current().registry().get::<SessionsManager>();

    // Register self in session manager. `AsyncContext::wait` register
    // future within context, but context waits until this future resolves
    // before processing any other events.
    session_manager_addr
        .send(Consolidate {
            address: session.remote_addr,
            potential_new_peer: session.remote_sender_addr.unwrap(),
            session_type: session.session_type,
        })
        .into_actor(session)
        .then(|res, act, ctx| {
            match res {
                Ok(Ok(_)) => {
                    debug!("Session successfully consolidated in the Session Manager");
                    // Set status to consolidate
                    act.status = SessionStatus::Consolidated;

                    actix::fut::ok(())
                }
                _ => {
                    warn!("Session consolidate in Session Manager failed");
                    // FIXME(#72): a full stop of the session is not correct (unregister should
                    // be skipped)
                    ctx.stop();

                    actix::fut::err(())
                }
            }
        })
        .wait(ctx);
}

/// Function called when GetPeers message is received
fn peer_discovery_get_peers(session: &mut Session, ctx: &mut Context<Session>) {
    // Get the address of PeersManager actor
    let peers_manager_addr = System::current()
        .registry()
        .get::<peers_manager::PeersManager>();

    // Start chain of actions
    peers_manager_addr
        // Send GetPeer message to PeersManager actor
        // This returns a Request Future, representing an asynchronous message sending process
        .send(peers_manager::messages::GetPeers)
        // Convert a normal future into an ActorFuture
        .into_actor(session)
        // Process the response from PeersManager
        // This returns a FutureResult containing the socket address if present
        .then(|res, act, ctx| {
            match res {
                Ok(Ok(addresses)) => {
                    info!(
                        "Received ({:?}) peer addresses from PeersManager",
                        addresses.len()
                    );
                    let peers_msg = WitnetMessage::build_peers(&addresses);
                    act.send_message(peers_msg);
                }
                _ => {
                    warn!("Failed to receive peers from PeersManager");
                    // FIXME(#72): a full stop of the session is not correct (unregister should
                    // be skipped)
                    ctx.stop();
                }
            }
            actix::fut::ok(())
        })
        .wait(ctx);
}

/// Function called when Peers message is received
fn peer_discovery_peers(peers: &[Address]) {
    // Get peers manager address
    let peers_manager_addr = System::current()
        .registry()
        .get::<peers_manager::PeersManager>();

    // Convert array of address to vector of socket addresses
    let addresses = peers.iter().map(from_address).collect();

    // Send AddPeers message to the peers manager
    peers_manager_addr.do_send(peers_manager::messages::AddPeers {
        // TODO: convert Vec<Address> to Vec<SocketAddr>
        addresses,
    });
}

/// Function called when Verack message is received
fn handshake_verack(session: &mut Session) {
    let flags = &mut session.handshake_flags;

    if flags.verack_rx {
        debug!("Verack message already received");
    }

    // Set verack_rx flag
    flags.verack_rx = true;
}

/// Function called when Version message is received
fn handshake_version(session: &mut Session, sender_address: &Address) -> Vec<WitnetMessage> {
    let flags = &mut session.handshake_flags;

    if flags.version_rx {
        debug!("Version message already received");
    }

    // Placeholder for version fields verification
    session.remote_sender_addr = Some(from_address(sender_address));

    // Set version_rx flag, indicating reception of a version message from the peer
    flags.version_rx = true;

    let mut responses: Vec<WitnetMessage> = vec![];
    if !flags.verack_tx {
        flags.verack_tx = true;
        let verack = WitnetMessage::build_verack();
        responses.push(verack);
    }
    if !flags.version_tx {
        flags.version_tx = true;
        let version = WitnetMessage::build_version(session.server_addr, session.remote_addr, 0);
        responses.push(version);
    }

    responses
}
