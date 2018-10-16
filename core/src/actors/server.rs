use std::net::SocketAddr;

use actix::{Actor, AsyncContext, Context, Handler, Message, StreamHandler};
use actix::io::FramedWrite;
use futures::Stream;
use log::info;
use tokio::codec::FramedRead;
use tokio::io::AsyncRead;
use tokio::net::{TcpListener, TcpStream};

use crate::actors::codec::P2PCodec;
use crate::actors::session::{Session, SessionType};

/// Message to hold a TCP stream representing a bidirectional TCP connection
#[derive(Message, Debug)]
struct TcpConnect {
    stream: TcpStream
}

impl TcpConnect {
    /// Method to create a new TcpConnect message
    fn new(stream: TcpStream) -> TcpConnect {
        TcpConnect { stream }
    }
}

/// TCP server that will accept incoming connections and create session actors
pub struct Server {
    /// Server socket address
    address: SocketAddr,
}

impl Server {
    /// Method to create a new server
    pub fn new(address: SocketAddr) -> Self {
        Server { address }
    }
}

/// Handler for TcpConnect messages (built from incoming connections)
impl Handler<TcpConnect> for Server {
    /// Response for message, which is defined by `ResponseType` trait
    type Result = ();

    /// Method to handle the TcpConnect message
    fn handle(&mut self, msg: TcpConnect, _ctx: &mut Self::Context) {
        // Create a session actor
        Session::create(move |ctx| {
            // Split tcp stream into read and write parts
            let (r, w) = msg.stream.split();

            // Add message stream in session from the read part of the tcp stream
            Session::add_stream(FramedRead::new(r, P2PCodec), ctx);

            // Create the session actor and store in it the write part of the tcp stream
            Session::new(SessionType::Server,
                         FramedWrite::new(w, P2PCodec, ctx))
        });
    }
}

/// Make actor from `Server`
impl Actor for Server {
    /// Every actor has to provide execution `Context` in which it can run.
    type Context = Context<Self>;

    /// Method to be executed when the actor is started
    fn started(&mut self, ctx: &mut Self::Context) {
        // Bind TCP listener to this address
        let listener = TcpListener::bind(&self.address).unwrap();

        // Add message stream which will return a TcpConnect for each incoming TCP connection
        ctx.add_message_stream(listener.incoming()
            .map_err(|_| ())
            .map(|stream| {
                TcpConnect::new(stream)
            }));

        info!("P2P server has been started at {:?}", &self.address);
    }
}
