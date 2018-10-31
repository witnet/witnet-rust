use futures::Stream;
use log::{debug, info};
use std::net::SocketAddr;

use actix::{
    actors::resolver::{ConnectAddr, Resolver, ResolverError},
    fut::FutureResult,
    io::FramedWrite,
    Actor, ActorFuture, AsyncContext, Context, ContextFutureSpawner, Handler, MailboxError,
    Message, StreamHandler, System, SystemService, WrapFuture,
};
use tokio::{
    codec::FramedRead,
    io::AsyncRead,
    net::{TcpListener, TcpStream},
};

use crate::actors::config_manager::{process_get_config_response, ConfigManager, GetConfig};
use crate::actors::{codec::P2PCodec, session::Session};

use witnet_p2p::sessions::SessionType;

////////////////////////////////////////////////////////////////////////////////////////
// ACTOR MESSAGES
////////////////////////////////////////////////////////////////////////////////////////
/// Actor message that holds the TCP stream from an inbound TCP connection
#[derive(Message)]
struct InboundTcpConnect {
    stream: TcpStream,
}

impl InboundTcpConnect {
    /// Method to create a new InboundTcpConnect message from a TCP stream
    fn new(stream: TcpStream) -> InboundTcpConnect {
        InboundTcpConnect { stream }
    }
}

/// Actor message to request the creation of an outbound TCP connection to a peer.
#[derive(Message)]
pub struct OutboundTcpConnect {
    /// Address of the outbound connection
    pub address: SocketAddr,
}

/// Returned type by the Resolver actor for the ConnectAddr message
type ResolverResult = Result<TcpStream, ResolverError>;

////////////////////////////////////////////////////////////////////////////////////////
// ACTOR BASIC STRUCTURE
////////////////////////////////////////////////////////////////////////////////////////
/// Connections manager actor
#[derive(Default)]
pub struct ConnectionsManager;

/// Make actor from `ConnectionsManager`
impl Actor for ConnectionsManager {
    /// Every actor has to provide execution `Context` in which it can run.
    type Context = Context<Self>;

    /// Method to be executed when the actor is started
    fn started(&mut self, ctx: &mut Self::Context) {
        debug!("Connections Manager actor has been started!");

        // Start server
        // FIXME(#72): decide what to do with actor when server cannot be started
        self.start_server(ctx);
    }
}

/// Required trait for being able to retrieve connections manager address from system registry
impl actix::Supervised for ConnectionsManager {}

/// Required trait for being able to retrieve connections manager address from system registry
impl SystemService for ConnectionsManager {}

/// Auxiliary methods for `ConnectionsManager` actor
impl ConnectionsManager {
    /// Method to start a server
    fn start_server(&mut self, ctx: &mut <Self as Actor>::Context) {
        debug!("Trying to start P2P server...");

        // Get address to launch the server
        let config_manager_addr = System::current().registry().get::<ConfigManager>();

        // Start chain of actions
        config_manager_addr
            // Send GetConfig message to config manager actor
            // This returns a Request Future, representing an asynchronous message sending process
            .send(GetConfig)
            // Convert a normal future into an ActorFuture
            .into_actor(self)
            // Process the response from the config manager
            // This returns a FutureResult containing the socket address if present
            .then(|res, _act, _ctx| {
                // Process the response from config manager
                process_get_config_response(res)
            })
            // Process the received config
            // This returns a FutureResult containing a success or error
            .and_then(|config, _act, ctx| {
                // Get server address from config
                let server_address = config.connections.server_addr;

                // Bind TCP listener to this address
                // FIXME(#72): decide what to do with actor when server cannot be started
                let listener = TcpListener::bind(&server_address).unwrap();

                // Add message stream which will return a InboundTcpConnect for each incoming TCP connection
                ctx.add_message_stream(
                    listener
                        .incoming()
                        .map_err(|_| ())
                        .map(InboundTcpConnect::new),
                );

                info!("P2P server has been started at {:?}", server_address);

                actix::fut::ok(())
            })
            .wait(ctx);
    }

    /// Method to create a session actor from a TCP stream
    fn create_session(stream: TcpStream, session_type: SessionType) {
        // Create a session actor
        Session::create(move |ctx| {
            // Get peer address
            let address = stream.peer_addr().unwrap();

            // Split TCP stream into read and write parts
            let (r, w) = stream.split();

            // Add stream in session actor from the read part of the tcp stream
            Session::add_stream(FramedRead::new(r, P2PCodec), ctx);

            // Create the session actor and store in its state the write part of the tcp stream
            Session::new(address, session_type, FramedWrite::new(w, P2PCodec, ctx))
        });
    }

    /// Method to process resolver ConnectAddr response
    fn process_connect_addr_response(
        response: Result<ResolverResult, MailboxError>,
    ) -> FutureResult<(), (), Self> {
        response
            // Process the Result<ResolverResult, MailboxError>
            .map_or_else(
                |e| {
                    debug!("Unsuccessful communication with resolver: {}", e);
                    actix::fut::err(())
                },
                |res| {
                    // Process the ResolverResult
                    res.map_or_else(
                        |e| {
                            debug!("Error while trying to connect to the peer: {}", e);
                            actix::fut::err(())
                        },
                        |stream| {
                            debug!("Connected to peer {:?}", stream.peer_addr());

                            // Create a session actor from connection
                            ConnectionsManager::create_session(stream, SessionType::Outbound);

                            actix::fut::ok(())
                        },
                    )
                },
            )
    }
}

////////////////////////////////////////////////////////////////////////////////////////
// ACTOR MESSAGE HANDLERS
////////////////////////////////////////////////////////////////////////////////////////
/// Handler for InboundTcpConnect messages (built from inbound connections)
impl Handler<InboundTcpConnect> for ConnectionsManager {
    /// Response for message, which is defined by `ResponseType` trait
    type Result = ();

    /// Method to handle the InboundTcpConnect message
    fn handle(&mut self, msg: InboundTcpConnect, _ctx: &mut Self::Context) {
        // Create a session actor from connection
        ConnectionsManager::create_session(msg.stream, SessionType::Inbound);
    }
}

/// Handler for OutboundTcpConnect messages (requested for creating outgoing connections)
impl Handler<OutboundTcpConnect> for ConnectionsManager {
    /// Response for message, which is defined by `ResponseType` trait
    type Result = ();

    /// Method to handle the OutboundTcpConnect message
    fn handle(&mut self, msg: OutboundTcpConnect, ctx: &mut Self::Context) {
        // Get resolver from registry and send a ConnectAddr message to it
        Resolver::from_registry()
            .send(ConnectAddr(msg.address))
            .into_actor(self)
            .then(|res, _act, _ctx| ConnectionsManager::process_connect_addr_response(res))
            .wait(ctx);
    }
}
