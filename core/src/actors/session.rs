use log::warn;
use std::io::Error;
use std::net::SocketAddr;
use std::time::Duration;

use actix::io::{FramedWrite, WriteHandler};
use actix::{
    Actor, ActorContext, ActorFuture, AsyncContext, Context, ContextFutureSpawner, Handler,
    Message, Running, StreamHandler, System, WrapFuture,
};
use log::debug;
use tokio::io::WriteHalf;
use tokio::net::TcpStream;

use crate::actors::codec::{P2PCodec, Request, Response};
use crate::actors::peers_manager;
use crate::actors::sessions_manager::{Register, SessionsManager, Unregister};

use witnet_data_structures::{
    serializers::TryFrom,
    types::{Command, Message as WitnetMessage},
};
use witnet_p2p::sessions::{SessionStatus, SessionType};

////////////////////////////////////////////////////////////////////////////////////////
// ACTOR BASIC STRUCTURE
////////////////////////////////////////////////////////////////////////////////////////

/// Session representing a TCP connection
pub struct Session {
    /// Server socket address
    _server_addr: SocketAddr,

    /// Remote socket address
    remote_addr: SocketAddr,

    /// Session type
    session_type: SessionType,

    /// Session status
    status: SessionStatus,

    /// Framed wrapper to send messages through the TCP connection
    framed: FramedWrite<WriteHalf<TcpStream>, P2PCodec>,

    /// Handshake timeout
    _handshake_timeout: Duration,
}

/// Session helper methods
impl Session {
    /// Method to create a new session
    pub fn new(
        _server_addr: SocketAddr,
        remote_addr: SocketAddr,
        session_type: SessionType,
        framed: FramedWrite<WriteHalf<TcpStream>, P2PCodec>,
        _handshake_timeout: Duration,
    ) -> Session {
        Session {
            _server_addr,
            remote_addr,
            session_type,
            status: SessionStatus::Unconsolidated,
            framed,
            _handshake_timeout,
        }
    }
}

/// Implement actor trait for Session
impl Actor for Session {
    /// Every actor has to provide execution `Context` in which it can run.
    type Context = Context<Self>;

    /// Method to be executed when the actor is started
    fn started(&mut self, ctx: &mut Self::Context) {
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
                    Ok(Ok(_)) => debug!("Session successfully registered into the Session Manager"),
                    _ => {
                        debug!("Session register into Session Manager failed");
                        // FIXME(#72): a full stop of the session is not correct (unregister should
                        // be skipped)
                        ctx.stop()
                    }
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
pub type SessionGetPeersResult = ();

/// Message to indicate that the session needs to send a GetPeers message through the network
pub struct GetPeers;

impl Message for GetPeers {
    type Result = SessionGetPeersResult;
}

////////////////////////////////////////////////////////////////////////////////////////
// ACTOR MESSAGE HANDLERS
////////////////////////////////////////////////////////////////////////////////////////
/// Implement `WriteHandler` for Session
impl WriteHandler<Error> for Session {}

/// Implement `StreamHandler` trait in order to use `Framed` with an actor
impl StreamHandler<Request, Error> for Session {
    /// This is main event loop for client requests
    fn handle(&mut self, msg: Request, ctx: &mut Self::Context) {
        // Handle different types of requests
        match msg {
            Request::Message(message) => {
                debug!(
                    "Session against {} received message: {:?}",
                    self.remote_addr, message
                );
                let decoded_msg = WitnetMessage::try_from(message.to_vec());
                match decoded_msg {
                    Err(err) => debug!("Error decoding message: {:?}", err),
                    Ok(msg) => match msg.kind {
                        Command::GetPeers => {
                            handle_get_peers_message(self, ctx);
                        }
                        _ => warn!(
                            "Received a message of kind \"{:?}\", which is not implemented yet",
                            msg.kind
                        ),
                    },
                }
            }
        }
    }
}

/// Handler for GetPeers message.
impl Handler<GetPeers> for Session {
    type Result = SessionGetPeersResult;

    fn handle(&mut self, _msg: GetPeers, _: &mut Context<Self>) {
        debug!("GetPeers message should be sent through the network");
        // Create get peers message
        let get_peers_msg: Vec<u8> = WitnetMessage::build_get_peers().into();
        // Write get peers message in session
        self.framed.write(Response::Message(get_peers_msg.into()));
    }
}

fn handle_get_peers_message(session: &mut Session, ctx: &mut Context<Session>) {
    // Get the address of PeersManager actor
    let peers_manager_addr = System::current()
        .registry()
        .get::<peers_manager::PeersManager>();

    // Start chain of actions
    peers_manager_addr
        // Send GetPeer message to PeersManager actor
        // This returns a Request Future, representing an asynchronous message sending process
        .send(peers_manager::GetPeers)
        // Convert a normal future into an ActorFuture
        .into_actor(session)
        // Process the response from PeersManager
        // This returns a FutureResult containing the socket address if present
        .then(|res, act, ctx| {
            match res {
                Ok(Ok(addresses)) => {
                    debug!("Get peers successfully registered into the Peers Manager");
                    let peers_msg: Vec<u8> = WitnetMessage::build_peers(&addresses).into();
                    act.framed.write(Response::Message(peers_msg.into()));
                }
                _ => {
                    debug!("Get peers register into Peers Manager failed");
                    // FIXME(#72): a full stop of the session is not correct (unregister should
                    // be skipped)
                    ctx.stop();
                }
            }
            actix::fut::ok(())
        })
        .wait(ctx);
}
