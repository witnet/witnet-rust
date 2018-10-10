use std::io::Error;

use actix::{Actor, Context, StreamHandler, Running, AsyncContext,
            WrapFuture, ActorFuture, ActorContext, ContextFutureSpawner, System};
use actix::io::{FramedWrite, WriteHandler};
use log::info;
use tokio::io::WriteHalf;
use tokio::net::TcpStream;

use crate::actors::codec::{P2PCodec, Request};
use crate::actors::session_manager::{SessionManager, Connect, Disconnect};

#[derive(Copy, Clone)]
/// Session type
pub enum SessionType {
    /// Server session
    Server,

    /// Client session
    Client,
}

/// Session representing a TCP connection
pub struct Session {
    /// Unique session id
    id: usize,

    /// Session type
    session_type: SessionType,

    /// Framed wrapper to send messages through the TCP connection
    _framed: FramedWrite<WriteHalf<TcpStream>, P2PCodec>,
}

/// Session helper methods
impl Session {
    /// Method to create a new session
    pub fn new(session_type: SessionType,
               _framed: FramedWrite<WriteHalf<TcpStream>, P2PCodec>) -> Session {
        Session { id: 0, session_type, _framed }
    }
}

/// Implement actor trait for Session
impl Actor for Session {
    /// Every actor has to provide execution `Context` in which it can run.
    type Context = Context<Self>;

    /// Method to be executed when the actor is started
    fn started(&mut self, ctx: &mut Self::Context) {
        // Get session manager address
        let session_manager_addr = System::current().registry().get::<SessionManager>();

        // Register self in session manager. `AsyncContext::wait` register
        // future within context, but context waits until this future resolves
        // before processing any other events.
        session_manager_addr
            .send(Connect::new(ctx.address(), self.session_type))
            .into_actor(self)
            .then(|res, act, ctx| {
                match res {
                    Ok(res) => act.id = res,
                    // Something is wrong with session manager
                    _ => ctx.stop(),
                }
                actix::fut::ok(())
            })
            .wait(ctx);
    }

    /// Method to be executed when the actor is stopping
    fn stopping(&mut self, _: &mut Self::Context) -> Running {
        // Get session manager address
        let session_manager_addr = System::current().registry().get::<SessionManager>();

        // Deregister session from session manager
        session_manager_addr.do_send(Disconnect::new(self.id, self.session_type));

        Running::Stop
    }
}

/// Implement write handler for Session
impl WriteHandler<Error> for Session {}

/// Implement `StreamHandler`trait in order to use `Framed` with an actor
impl StreamHandler<Request, Error> for Session {
    /// This is main event loop for client requests
    fn handle(&mut self, msg: Request, _ctx: &mut Self::Context) {
        // Handler different types of requests
        match msg {
            Request::Message(message) => {
                info!("Session {} received message: {}", self.id, message);
            }
        }
    }
}
