use std::io::Error;
use std::net::SocketAddr;

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

use witnet_data_structures::types::Message as ProtocolMessage;
use witnet_p2p::sessions::{SessionStatus, SessionType};

////////////////////////////////////////////////////////////////////////////////////////
// ACTOR BASIC STRUCTURE
////////////////////////////////////////////////////////////////////////////////////////

/// Session representing a TCP connection
pub struct Session {
    /// Session socket address
    address: SocketAddr,

    /// Session type
    session_type: SessionType,

    /// Session status
    status: SessionStatus,

    /// Framed wrapper to send messages through the TCP connection
    _framed: FramedWrite<WriteHalf<TcpStream>, P2PCodec>,
}

/// Session helper methods
impl Session {
    /// Method to create a new session
    pub fn new(
        address: SocketAddr,
        session_type: SessionType,
        status: SessionStatus,
        _framed: FramedWrite<WriteHalf<TcpStream>, P2PCodec>,
    ) -> Session {
        Session {
            address,
            session_type,
            status,
            _framed,
        }
    }
}

/// Implement actor trait for Session
impl Actor for Session {
    /// Every actor has to provide execution `Context` in which it can run.
    type Context = Context<Self>;

    /// Method to be executed when the actor is started
    fn started(&mut self, ctx: &mut Self::Context) {
        // Get session manager address
        let session_manager_addr = System::current().registry().get::<SessionsManager>();

        // Register self in session manager. `AsyncContext::wait` register
        // future within context, but context waits until this future resolves
        // before processing any other events.
        session_manager_addr
            .send(Register {
                address: self.address,
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

        // Deregister session from session manager
        session_manager_addr.do_send(Unregister {
            address: self.address,
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
pub type SessionSendMessageResult = ();

/// Message to indicate that the session needs to send a message through the network
pub struct SendMessage {
    /// Protocol message to be sent through the network
    pub message: ProtocolMessage,
}

impl Message for SendMessage {
    type Result = SessionSendMessageResult;
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
                debug!("Session {} received message: {:?}", self.address, message);
            }
        }
    }
}

/// Handler for SendMessage message.
impl Handler<SendMessage> for Session {
    type Result = SessionSendMessageResult;

    fn handle(&mut self, msg: SendMessage, _: &mut Context<Self>) -> Self::Result {
        debug!("Received message to send: {:?}", msg.message);
    }
}
