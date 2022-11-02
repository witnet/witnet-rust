//! Websockets server implementation.
use std::net;
use std::sync::Arc;

use jsonrpc_pubsub as pubsub;
use jsonrpc_ws_server as server;

mod error;

pub use error::Error;

type PubSubHandler = pubsub::PubSubHandler<Arc<pubsub::Session>>;

/// TODO: doc
pub struct Server(server::Server);

impl Server {
    /// TODO: doc
    pub fn build() -> ServerBuilder {
        ServerBuilder::default()
    }
}

/// Server configuration builder.
pub struct ServerBuilder {
    handler: PubSubHandler,
    addr: net::SocketAddr,
}

impl Default for ServerBuilder {
    fn default() -> Self {
        Self {
            handler: PubSubHandler::default(),
            addr: net::SocketAddr::V4(net::SocketAddrV4::new(
                net::Ipv4Addr::new(127, 0, 0, 1),
                3200,
            )),
        }
    }
}

impl ServerBuilder {
    /// Set handler
    pub fn handler(mut self, handler: PubSubHandler) -> Self {
        self.handler = handler;
        self
    }

    /// Set the socket address to bind to.
    pub fn addr(mut self, addr: net::SocketAddr) -> Self {
        self.addr = addr;
        self
    }

    /// Starts a JsonRPC Websockets server.
    pub fn start(self) -> Result<Server, Box<Error>> {
        let Self { handler, addr } = self;

        server::ServerBuilder::with_meta_extractor(handler, |context: &server::RequestContext| {
            Arc::new(pubsub::Session::new(context.sender()))
        })
        .start(&addr)
        .map(Server)
        .map_err(|err| Box::new(Error(err)))
    }
}
