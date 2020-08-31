use actix::prelude::*;

use witnet_net::client::tcp::jsonrpc;
use witnet_net::server::ws::Server;

use crate::types;

pub mod error;
pub mod handlers;
pub mod methods;
pub mod params;
pub mod routes;
mod state;

pub use error::*;
pub use handlers::*;
pub use params::*;
pub use routes::*;

pub type Result<T> = std::result::Result<T, Error>;

pub type ResponseFuture<T> = actix::ResponseFuture<T, Error>;

pub type ResponseActFuture<T> = actix::ResponseActFuture<App, T, Error>;

pub struct App {
    server: Option<Server>,
    params: Params,
    state: state::State,
}

impl Actor for App {
    type Context = Context<Self>;

    fn started(&mut self, ctx: &mut Self::Context) {
        // Subscribe to new blocks and blocks consolidation notifications from a Witnet node
        self.node_subscribe("newBlocks", ctx);
        self.node_subscribe("consolidatedBlocks", ctx);

        let mut handler =
            jsonrpc_pubsub::PubSubHandler::new(jsonrpc_core::MetaIoHandler::default());
        connect_routes(&mut handler, ctx.address(), Arbiter::current());

        if let Ok(server) = Server::build()
            .handler(handler)
            .addr(self.params.server_addr)
            .start()
        {
            self.server = Some(server);
        }
    }
}
