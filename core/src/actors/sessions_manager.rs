use std::net::SocketAddr;
use std::time::Duration;

use actix::fut::FutureResult;
use actix::{
    Actor, ActorFuture, Addr, AsyncContext, Context, ContextFutureSpawner, Handler, MailboxError,
    Message, System, SystemService, WrapFuture,
};

use log::debug;

use crate::actors::config_manager::send_get_config_request;
use crate::actors::connections_manager::{ConnectionsManager, OutboundTcpConnect};
use crate::actors::session::Session;

use crate::actors::peers_manager::{GetRandomPeer, PeersManager, PeersSocketAddrResult};
use witnet_p2p::sessions::{error::SessionsResult, SessionStatus, SessionType, Sessions};

/// Period of the bootstrap peers task (in seconds)
const BOOTSTRAP_PEERS_PERIOD: u64 = 5;

////////////////////////////////////////////////////////////////////////////////////////
// ACTOR BASIC STRUCTURE
////////////////////////////////////////////////////////////////////////////////////////

/// Sessions manager actor
#[derive(Default)]
pub struct SessionsManager {
    // Registered sessions
    sessions: Sessions<Addr<Session>>,
}

impl SessionsManager {
    /// Method to periodically bootstrap outbound sessions
    fn bootstrap_peers(&self, ctx: &mut Context<Self>) {
        // Schedule the bootstrap with a given period
        ctx.run_later(Duration::from_secs(BOOTSTRAP_PEERS_PERIOD), |act, ctx| {
            debug!(
                "Number of outbound sessions {}",
                act.sessions.get_num_outbound_sessions()
            );

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

                        // Get connections manager from registry and send an OutboundTcpConnect message to it
                        let connections_manager_addr =
                            System::current().registry().get::<ConnectionsManager>();
                        connections_manager_addr.do_send(OutboundTcpConnect { address });

                        actix::fut::ok(())
                    })
                    .wait(ctx);
            }

            // Reschedule the bootstrap peers task
            act.bootstrap_peers(ctx);
        });
    }

    /// Method to process peers manager GetPeer response
    fn process_get_peer_response(
        &mut self,
        response: Result<PeersSocketAddrResult, MailboxError>,
    ) -> FutureResult<SocketAddr, (), Self> {
        response
            // Unwrap the Result<PeersSocketAddrResult, MailboxError>
            .unwrap_or_else(|_| {
                debug!("Unsuccessful communication with peers manager");
                Ok(None)
            })
            // Unwrap the PeersSocketAddrResult
            .unwrap_or_else(|_| {
                debug!("An error happened in peers manager when getting a peer");
                None
            })
            // Check if PeersSocketAddrResult returned `None`
            .or_else(|| {
                debug!("No peer obtained from peers manager");
                None
            })
            // Filter the result checking if outbound address is eligible as new peer
            .filter(|address: &SocketAddr| {
                self.sessions.is_outbound_address_eligible(address.clone())
            })
            // Check if there is a peer after filter
            .or_else(|| {
                debug!("No eligible peer obtained from peers manager");
                None
            })
            // Convert Some(SocketAddr) or None to FutureResult<SocketAddr, (), Self>
            .map(actix::fut::ok)
            .unwrap_or_else(|| actix::fut::err(()))
    }
}

/// Make actor from `SessionsManager`
impl Actor for SessionsManager {
    /// Every actor has to provide execution `Context` in which it can run
    type Context = Context<Self>;

    /// Method to be executed when the actor is started
    fn started(&mut self, ctx: &mut Self::Context) {
        debug!("Sessions Manager actor has been started!");

        // Send message to config manager and process its response
        send_get_config_request(self, ctx, |s, _ctx, config| {
            // Set server address and connections limits
            s.sessions
                .set_server_address(config.connections.server_addr);
            s.sessions.set_limits(
                config.connections.inbound_limit,
                config.connections.outbound_limit,
            );
        });

        // We'll start the bootstrap peers process on sessions manager start
        self.bootstrap_peers(ctx);
    }
}

/// Required traits for being able to retrieve sessions manager address from registry
impl actix::Supervised for SessionsManager {}
impl SystemService for SessionsManager {}

////////////////////////////////////////////////////////////////////////////////////////
// ACTOR MESSAGES
////////////////////////////////////////////////////////////////////////////////////////
/// Message result of unit
pub type SessionsUnitResult = SessionsResult<()>;

/// Message to indicate that a new session is created
pub struct Register {
    /// Socket address to identify the peer
    pub address: SocketAddr,

    /// Address of the session actor that is to be connected
    pub actor: Addr<Session>,

    /// Session type
    pub session_type: SessionType,
}

impl Message for Register {
    type Result = SessionsUnitResult;
}

/// Message to indicate that a session is disconnected
pub struct Unregister {
    /// Socket address to identify the peer
    pub address: SocketAddr,

    /// Session type
    pub session_type: SessionType,

    /// Session status
    pub status: SessionStatus,
}

impl Message for Unregister {
    type Result = SessionsUnitResult;
}

/// Message to indicate that a session needs to be consolidated
pub struct Consolidate {
    /// Socket address to identify the peer
    pub address: SocketAddr,

    /// Session type
    pub session_type: SessionType,
}

impl Message for Consolidate {
    type Result = SessionsUnitResult;
}

////////////////////////////////////////////////////////////////////////////////////////
// ACTOR MESSAGE HANDLERS
////////////////////////////////////////////////////////////////////////////////////////

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
            Err(error) => debug!(
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
        // Call method register session from sessions library
        let result = self
            .sessions
            .unregister_session(msg.session_type, msg.status, msg.address);

        match &result {
            Ok(_) => debug!(
                "Session (type {:?}) unregistered for peer {}",
                msg.session_type, msg.address
            ),
            Err(error) => debug!(
                "Error while unregistering peer {} (session type {:?}): {}",
                msg.address, msg.session_type, error
            ),
        }

        result
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

        match &result {
            Ok(_) => debug!(
                "Session (type {:?}) status consolidated for peer {}",
                msg.session_type, msg.address
            ),
            Err(error) => debug!(
                "Error while consolidating peer {} (session type {:?}): {}",
                msg.address, msg.session_type, error
            ),
        }

        result
    }
}
