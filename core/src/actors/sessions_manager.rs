use std::marker::Send;
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
use crate::actors::session::{GetPeers, Session};

use crate::actors::peers_manager::{GetRandomPeer, PeersManager, PeersSocketAddrResult};

use witnet_p2p::sessions::{error::SessionsResult, SessionStatus, SessionType, Sessions};

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
    fn bootstrap_peers(&self, ctx: &mut Context<Self>, bootstrap_peers_period: Duration) {
        // Schedule the bootstrap with a given period
        ctx.run_later(bootstrap_peers_period, move |act, ctx| {
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
            act.bootstrap_peers(ctx, bootstrap_peers_period);
        });
    }

    /// Method to periodically discover peers
    fn discovery_peers(&self, ctx: &mut Context<Self>, discovery_peers_period: Duration) {
        // Schedule the discovery_peers with a given period
        ctx.run_later(discovery_peers_period, move |act, ctx| {
            // Send Anycast(GetPeers) message
            ctx.notify(Anycast {
                command: GetPeers {},
            });
            act.discovery_peers(ctx, discovery_peers_period);
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

    /// Method to process session SendMessage response
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
}

/// Make actor from `SessionsManager`
impl Actor for SessionsManager {
    /// Every actor has to provide execution `Context` in which it can run
    type Context = Context<Self>;

    /// Method to be executed when the actor is started
    fn started(&mut self, ctx: &mut Self::Context) {
        debug!("Sessions Manager actor has been started!");

        // Send message to config manager and process its response
        send_get_config_request(self, ctx, |act, ctx, config| {
            // Get periods for peers bootstrapping and discovery tasks
            let bootstrap_peers_period = config.connections.bootstrap_peers_period;
            let discovery_peers_period = config.connections.discovery_peers_period;

            // Set server address and connections limits
            act.sessions
                .set_server_address(config.connections.server_addr);
            act.sessions.set_limits(
                config.connections.inbound_limit,
                config.connections.outbound_limit,
            );

            // We'll start the peers bootstrapping process upon SessionsManager's start
            act.bootstrap_peers(ctx, bootstrap_peers_period);

            // We'll start the peers discovery process upon SessionsManager's start
            act.discovery_peers(ctx, discovery_peers_period);
        });
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

/// Message to indicate that a message is to be forwarded to a random consolidated outbound session
pub struct Anycast<T> {
    /// Command to be sent to the session
    pub command: T,
}

impl<T> Message for Anycast<T>
where
    T: Message + Send,
    T::Result: Send,
    Session: Handler<T>,
{
    type Result = ();
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

/// Handler for Anycast message
impl<T: 'static> Handler<Anycast<T>> for SessionsManager
where
    T: Message + Send,
    T::Result: Send,
    Session: Handler<T>,
{
    type Result = ();

    fn handle(&mut self, msg: Anycast<T>, ctx: &mut Context<Self>) {
        debug!("Received a message to send to a random session");

        // Request a random consolidated outbound session
        self.sessions
            .get_random_anycast_session()
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
                debug!("No consolidated outbound session was found");
            });
    }
}
