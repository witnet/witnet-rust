use std::net::SocketAddr;

use actix::actors::resolver::{ConnectAddr, Resolver};
use actix::{Actor, Context, SystemService, WrapFuture,
            ActorFuture, StreamHandler, ActorContext, ContextFutureSpawner};
use actix::io::FramedWrite;

use log::info;

use tokio::codec::FramedRead;
use tokio::io::AsyncRead;

use crate::actors::codec::P2PCodec;
use crate::actors::session::{Session, SessionType};

/// TCP client that will try to connect to a peer
pub struct Client {
    /// Peer (server) address
    peer: SocketAddr,
}

impl Client {
    /// Method to create a new client
    pub fn new(peer: SocketAddr) -> Self {
        Client { peer }
    }
}

/// Make actor from `Client`
impl Actor for Client {
    /// Every actor has to provide execution `Context` in which it can run.
    type Context = Context<Self>;

    /// Method to be executed when the actor is started
    fn started(&mut self, ctx: &mut Self::Context) {
        info!("Trying to connect to peer {}...", self.peer);

        // Get resolver from registry and send a ConnectAddr message to it
        Resolver::from_registry()
            .send(ConnectAddr(self.peer))
            .into_actor(self)
            .map(|res, act, ctx| match res {
                // Successful connection
                Ok(stream) => {
                    info!("Connected to peer {}", act.peer);

                    // Create session actor with the connection
                    Session::create(move |ctx| {
                        // Split tcp stream into read and write parts
                        let (r, w) = stream.split();

                        // Add message stream in session from the read part of the tcp stream
                        Session::add_stream(FramedRead::new(r, P2PCodec), ctx);

                        // Create the session actor and store in it the write part of the tcp stream
                        Session::new(SessionType::Client,
                                     FramedWrite::new(w, P2PCodec, ctx))
                    });
                }

                // Not successful connection
                Err(err) => {
                    info!("Cannot connect to peer: {}", err);
                    ctx.stop();
                }
            })
            // Not successful connection
            .map_err(|err, act, ctx| {
                info!("Cannot connect to server `{}`: {}", act.peer, err);
                ctx.stop();
            })
            .wait(ctx);
    }
}