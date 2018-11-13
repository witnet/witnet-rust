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

use crate::actors::codec::{P2PCodec, Request};
use crate::actors::sessions_manager::{Register, SessionsManager, Unregister};

use witnet_p2p::sessions::{SessionStatus, SessionType};

////////////////////////////////////////////////////////////////////////////////////////
// ACTOR BASIC STRUCTURE
////////////////////////////////////////////////////////////////////////////////////////

/// Session representing a TCP connection
pub struct Session {
    /// Local socket address
    _local_addr: SocketAddr,

    /// Remote socket address
    remote_addr: SocketAddr,

    /// Session type
    session_type: SessionType,

    /// Session status
    status: SessionStatus,

    /// Framed wrapper to send messages through the TCP connection
    _framed: FramedWrite<WriteHalf<TcpStream>, P2PCodec>,

    /// Handshake timeout
    _handshake_timeout: Duration,
}

/// Session helper methods
impl Session {
    /// Method to create a new session
    pub fn new(
        _local_addr: SocketAddr,
        remote_addr: SocketAddr,
        session_type: SessionType,
        _framed: FramedWrite<WriteHalf<TcpStream>, P2PCodec>,
        _handshake_timeout: Duration,
    ) -> Session {
        Session {
            _local_addr,
            remote_addr,
            session_type,
            status: SessionStatus::Unconsolidated,
            _framed,
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
    fn handle(&mut self, msg: Request, _ctx: &mut Self::Context) {
        // Handler different types of requests
        match msg {
            Request::Message(message) => {
                debug!(
                    "Session against {} received message: {:?}",
                    self.remote_addr, message
                );
            }
        }
    }
}

/// Handler for GetPeers message.
impl Handler<GetPeers> for Session {
    type Result = SessionGetPeersResult;

    fn handle(&mut self, _msg: GetPeers, _: &mut Context<Self>) {
        debug!("GetPeers message should be sent through the network");
    }
}
