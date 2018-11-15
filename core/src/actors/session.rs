use std::io::Error;
use std::net::SocketAddr;
use std::time::Duration;

use actix::io::{FramedWrite, WriteHandler};
use actix::{
    Actor, ActorContext, ActorFuture, AsyncContext, Context, ContextFutureSpawner, Handler,
    Message, Running, StreamHandler, System, WrapFuture,
};
use log::{debug, info, warn};
use tokio::io::WriteHalf;
use tokio::net::TcpStream;

use crate::actors::codec::{BytesMut, P2PCodec};
use crate::actors::peers_manager;
use crate::actors::sessions_manager::{Consolidate, Register, SessionsManager, Unregister};

use witnet_data_structures::{
    builders::from_address,
    serializers::TryFrom,
    types::{Address, Command, Message as WitnetMessage},
};
use witnet_p2p::sessions::{SessionStatus, SessionType};

////////////////////////////////////////////////////////////////////////////////////////
// ACTOR BASIC STRUCTURE
////////////////////////////////////////////////////////////////////////////////////////
/// Handshake flags
#[derive(Default)]
struct HandshakeFlags {
    /// Flag to indicate that a version message was sent
    version_tx: bool,
    /// Flag to indicate that a version message was received
    version_rx: bool,
    /// Flag to indicate that a verack message was sent
    verack_tx: bool,
    /// Flag to indicate that a verack message was received
    verack_rx: bool,
}

/// HandshakeFlags helper methods
impl HandshakeFlags {
    // Auxiliary function to check if all flags are set to true
    fn all_true(&self) -> bool {
        self.verack_tx && self.verack_rx && self.version_tx && self.version_rx
    }
}

/// Session representing a TCP connection
pub struct Session {
    /// Server socket address (local peer)
    server_addr: SocketAddr,

    /// Remote socket address (remote server address only if outbound session)
    remote_addr: SocketAddr,

    /// Session type
    session_type: SessionType,

    /// Framed wrapper to send messages through the TCP connection
    framed: FramedWrite<WriteHalf<TcpStream>, P2PCodec>,

    /// Handshake timeout
    handshake_timeout: Duration,

    /// Session status
    status: SessionStatus,

    /// Handshake flags
    handshake_flags: HandshakeFlags,

    /// Remote sender address
    remote_sender_addr: Option<SocketAddr>,
}

/// Session helper methods
impl Session {
    /// Method to create a new session
    pub fn new(
        server_addr: SocketAddr,
        remote_addr: SocketAddr,
        session_type: SessionType,
        framed: FramedWrite<WriteHalf<TcpStream>, P2PCodec>,
        handshake_timeout: Duration,
    ) -> Session {
        Session {
            server_addr,
            remote_addr,
            session_type,
            framed,
            handshake_timeout,
            status: SessionStatus::Unconsolidated,
            handshake_flags: HandshakeFlags::default(),
            remote_sender_addr: None,
        }
    }
    /// Method to send a Witnet message to the remote peer
    fn send_message(&mut self, msg: WitnetMessage) {
        debug!(
            "-----> Session ({:?}) sending message: {:?}",
            self.remote_addr, msg
        );
        // Convert WitnetMessage into a vector of bytes
        let bytes: Vec<u8> = msg.into();
        // Convert bytes into BytestMut and send them
        self.framed.write(bytes.into());
    }
}

////////////////////////////////////////////////////////////////////////////////////////
// ACTOR IMPL
////////////////////////////////////////////////////////////////////////////////////////
/// Implement actor trait for Session
impl Actor for Session {
    /// Every actor has to provide execution `Context` in which it can run.
    type Context = Context<Self>;

    /// Method to be executed when the actor is started
    fn started(&mut self, ctx: &mut Self::Context) {
        // Set Handshake timeout for stopping actor if session is still unconsolidated after given period of time
        ctx.run_later(self.handshake_timeout, |act, ctx| {
            if act.status != SessionStatus::Consolidated {
                info!(
                    "Handshake timeout expired, disconnecting session with peer {:?}",
                    act.remote_addr
                );
                if let SessionStatus::Unconsolidated = act.status {
                    ctx.stop();
                }
            }
        });

        // Get sessions manager address
        let sessions_manager_addr = System::current().registry().get::<SessionsManager>();

        // Register self in session manager. `AsyncContext::wait` register
        // future within context, but context waits until this future resolves
        // before processing any other events.
        sessions_manager_addr
            .send(Register {
                address: self.remote_addr,
                actor: ctx.address(),
                session_type: self.session_type,
            })
            .into_actor(self)
            .then(|res, _act, ctx| {
                match res {
                    Ok(Ok(_)) => {
                        debug!("Session successfully registered into the Session Manager");

                        actix::fut::ok(())
                    }
                    _ => {
                        debug!("Session register into Session Manager failed");
                        // FIXME(#72): a full stop of the session is not correct (unregister should
                        // be skipped)
                        ctx.stop();

                        actix::fut::err(())
                    }
                }
            })
            .and_then(|_, act, _ctx| {
                // Send version if outbound session
                if let SessionType::Outbound = act.session_type {
                    // FIXME(#142): include the checkpoint of the current tip of the local blockchain
                    let version_msg =
                        WitnetMessage::build_version(act.server_addr, act.remote_addr, 0);
                    act.send_message(version_msg);
                    // Set handshake flag of sent version message
                    act.handshake_flags.version_tx = true;
                }

                actix::fut::ok(())
            })
            .wait(ctx);
    }

    /// Method to be executed when the actor is stopping
    fn stopping(&mut self, _: &mut Self::Context) -> Running {
        // Get session manager address
        let session_manager_addr = System::current().registry().get::<SessionsManager>();

        // Unregister session from session manager
        session_manager_addr.do_send(Unregister {
            address: self.remote_addr,
            session_type: self.session_type,
            status: self.status,
        });

        Running::Stop
    }
}

////////////////////////////////////////////////////////////////////////////////////////
// ACTOR MESSAGES
////////////////////////////////////////////////////////////////////////////////////////
/// Message result of unit
pub type SessionUnitResult = ();

/// Message to indicate that the session needs to send a GetPeers message through the network
pub struct GetPeers;

impl Message for GetPeers {
    type Result = SessionUnitResult;
}

////////////////////////////////////////////////////////////////////////////////////////
// ACTOR MESSAGE HANDLERS
////////////////////////////////////////////////////////////////////////////////////////
/// Implement `WriteHandler` for Session
impl WriteHandler<Error> for Session {}

/// Implement `StreamHandler` trait in order to use `Framed` with an actor
impl StreamHandler<BytesMut, Error> for Session {
    /// This is main event loop for client requests
    fn handle(&mut self, bytes: BytesMut, ctx: &mut Self::Context) {
        let result = WitnetMessage::try_from(bytes.to_vec());
        match result {
            Err(err) => warn!("Error decoding message: {:?}", err),
            Ok(msg) => {
                debug!(
                    "<----- Session ({}) received message: {:?}",
                    self.remote_addr, msg.kind
                );
                match (self.status, msg.kind) {
                    ////////////////////qq
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
                        debug!("Not implemented message command received!");
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

////////////////////////////////////////////////////////////////////////////////////////
// ACTOR HELPER FUNCTIONS
////////////////////////////////////////////////////////////////////////////////////////
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

/// Function called when Verack message is received
fn handshake_verack(session: &mut Session) {
    let flags = &mut session.handshake_flags;

    if flags.verack_rx {
        debug!("Verack message already received");
    }

    // Set verack_rx flag
    flags.verack_rx = true;
}

/// Function to try to consolidate session if handshake conditions are met
fn try_consolidate_session(session: &mut Session, ctx: &mut Context<Session>) {
    // Check if handshake flags are all set to true
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
                    debug!("Session consolidate in Session Manager failed");
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
                    debug!(
                        "Received ({:?}) peer addresses from PeersManager",
                        addresses.len()
                    );
                    let peers_msg = WitnetMessage::build_peers(&addresses);
                    act.send_message(peers_msg);
                }
                _ => {
                    debug!("Failed to receive peers from PeersManager");
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
    peers_manager_addr.do_send(peers_manager::AddPeers {
        // TODO: convert Vec<Address> to Vec<SocketAddr>
        addresses,
    });
}
