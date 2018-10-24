use futures::Stream;
use log::info;
use std::net::SocketAddr;

use actix::actors::resolver::{ConnectAddr, Resolver, ResolverError};
use actix::fut::FutureResult;
use actix::io::FramedWrite;
use actix::{
    Actor, ActorFuture, AsyncContext, Context, ContextFutureSpawner, Handler, MailboxError,
    Message, StreamHandler, System, SystemService, WrapFuture,
};
use tokio::codec::FramedRead;
use tokio::io::AsyncRead;
use tokio::net::{TcpListener, TcpStream};

use crate::actors::codec::P2PCodec;
use crate::actors::peers_manager::{GetPeer, PeersManager, PeersSocketAddrResult};
use crate::actors::session::{Session, SessionType};

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
/// The address of the peer is not specified as it will be determined by the peers manager actor.
#[derive(Default, Message)]
pub struct OutboundTcpConnect;

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
        // Start server
        // TODO[23-10-2018]: handle errors when starting server appropiately
        ConnectionsManager::start_server(ctx);
    }
}

/// Required trait for being able to retrieve connections manager address from system registry
impl actix::Supervised for ConnectionsManager {}

/// Required trait for being able to retrieve connections manager address from system registry
impl SystemService for ConnectionsManager {
    fn service_started(&mut self, _ctx: &mut Context<Self>) {}
}

/// Auxiliary methods for `ConnectionsManager` actor
impl ConnectionsManager {
    /// Method to start a server
    fn start_server(ctx: &mut <Self as Actor>::Context) {
        info!("Trying to start P2P server...");

        // Get address to launch the server
        // TODO[23-10-2018]: query server address from config manager
        let server_address = "127.0.0.1:50000".parse().unwrap();

        // Bind TCP listener to this address
        // TODO[23-10-2018]: handle errors
        let listener = TcpListener::bind(&server_address).unwrap();

        // Add message stream which will return a InboundTcpConnect for each incoming TCP connection
        ctx.add_message_stream(
            listener
                .incoming()
                .map_err(|_| ())
                .map(InboundTcpConnect::new),
        );

        info!("P2P server has been started at {:?}", server_address);
    }

    /// Method to create a session actor from a TCP stream
    fn create_session(stream: TcpStream, kind: SessionType) {
        // Create a session actor
        Session::create(move |ctx| {
            // Split TCP stream into read and write parts
            let (r, w) = stream.split();

            // Add stream in session actor from the read part of the tcp stream
            Session::add_stream(FramedRead::new(r, P2PCodec), ctx);

            // Create the session actor and store in its state the write part of the tcp stream
            Session::new(kind, FramedWrite::new(w, P2PCodec, ctx))
        });
    }

    /// Method to process peers manager GetPeer response
    fn process_get_peer_response(
        response: Result<PeersSocketAddrResult, MailboxError>,
    ) -> FutureResult<SocketAddr, (), Self> {
        match response {
            Ok(result) => match result {
                Ok(opt_sock_addr) => match opt_sock_addr {
                    Some(sock_addr) => {
                        info!("Trying to connect to peer {}", sock_addr);
                        actix::fut::ok(sock_addr)
                    }

                    None => {
                        info!("No peer obtained from peers manager");
                        actix::fut::err(())
                    }
                },
                Err(_) => {
                    info!("An error happened in peers manager when getting a peer");
                    actix::fut::err(())
                }
            },
            Err(_) => {
                info!("Unsuccessful communication with peers manager");
                actix::fut::err(())
            }
        }
    }

    /// Method to process resolver ConnectAddr response
    fn process_connect_addr_response(
        response: Result<Result<TcpStream, ResolverError>, MailboxError>,
    ) -> FutureResult<(), (), Self> {
        match response {
            Ok(result) => {
                match result {
                    Ok(stream) => {
                        info!("Connected to peer {:?}", stream.peer_addr());

                        // Create a session actor from connection
                        ConnectionsManager::create_session(stream, SessionType::Client);

                        actix::fut::ok(())
                    }
                    Err(_) => {
                        info!("An error happened in resolver when trying to connect to the peer");
                        actix::fut::err(())
                    }
                }
            }
            Err(_) => {
                info!("Unsuccessful communication with resolver");
                actix::fut::err(())
            }
        }
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
        ConnectionsManager::create_session(msg.stream, SessionType::Server);
    }
}

/// Handler for OutboundTcpConnect messages (requested for creating outgoing connections)
impl Handler<OutboundTcpConnect> for ConnectionsManager {
    /// Response for message, which is defined by `ResponseType` trait
    type Result = ();

    /// Method to handle the OutboundTcpConnect message
    fn handle(&mut self, _msg: OutboundTcpConnect, ctx: &mut Self::Context) {
        // Get peers manager address
        let peers_manager_addr = System::current().registry().get::<PeersManager>();

        // Start chain of actions
        peers_manager_addr
            // Send GetPeer message to peers manager actor
            // This returns a Request Future, representing an asynchronous message sending process
            .send(GetPeer)
            // Convert a normal future into an ActorFuture
            .into_actor(self)
            // Process the response from the peers manager
            // This returns a FutureResult containing the socket address if present
            .then(|res, _act, _ctx| ConnectionsManager::process_get_peer_response(res))
            //// Process the socket address received
            // This returns a FutureResult containing a success or error
            .and_then(|res, act, _ctx| {
                // Get resolver from registry and send a ConnectAddr message to it
                Resolver::from_registry()
                    .send(ConnectAddr(res))
                    .into_actor(act)
                    .then(|res, _act, _ctx| ConnectionsManager::process_connect_addr_response(res))
            })
            .wait(ctx);
    }
}
