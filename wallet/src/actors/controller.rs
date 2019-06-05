//! # Controller actor
//!
//! The Controller actor holds the address of the App actor and the instance of the Websockets server, and is in charge of graceful shutdown of the entire system.
//! See `Controller` struct for more info.
use std::net;
use std::path::PathBuf;

use actix::prelude::*;
use jsonrpc_core as rpc;
use jsonrpc_pubsub as pubsub;

use super::App;
use crate::api;
use crate::error;
use witnet_net::server::ws::Server;

/// Controller actor.
pub struct Controller {
    _server: Server,
    _app: Addr<App>,
}

impl Controller {
    pub fn build() -> ControllerBuilder {
        ControllerBuilder::new()
    }
}

/// Controller builder used to set optional parameters using the builder-pattern.
pub struct ControllerBuilder {
    server_addr: net::SocketAddr,
    db_path: PathBuf,
    node_url: Option<String>,
}

impl ControllerBuilder {
    /// Create a Controller builder with default values
    pub fn new() -> Self {
        let server_addr = net::SocketAddr::V4(net::SocketAddrV4::new(
            net::Ipv4Addr::new(127, 0, 0, 1),
            3200,
        ));

        Self {
            server_addr,
            db_path: ".witnet_wallet".into(),
            node_url: None,
        }
    }

    /// Set the address for the websockets server.
    ///
    /// By default it will use `127.0.0.1:3200`;
    pub fn server_addr(mut self, addr: net::SocketAddr) -> Self {
        self.server_addr = addr;
        self
    }

    /// Set the path for the database where the wallet files is stored.
    ///
    /// By default it will use `.witnet_wallet` in current directory.
    pub fn db_path(mut self, path: PathBuf) -> Self {
        self.db_path = path;
        self
    }

    /// Set the url of the node this wallet should use.
    ///
    /// By default the wallet won't try to communicate with the node.
    pub fn node_url(mut self, url: Option<String>) -> Self {
        self.node_url = url;
        self
    }

    /// Start the `Controller` actor and its services, e.g.: server, storage, node client, and so on.
    pub fn start(self) -> Result<Addr<Controller>, error::Error> {
        let app = App::build()
            .node_url(self.node_url)
            .db_path(self.db_path)
            .start()?;

        let mut handler = pubsub::PubSubHandler::new(rpc::MetaIoHandler::default());

        api::connect_routes(&mut handler, app.clone());

        let server = Server::build()
            .handler(handler)
            .addr(self.server_addr)
            .start()
            .map_err(error::Error::Server)?;

        let controller = Controller {
            _app: app,
            _server: server,
        };

        Ok(controller.start())
    }
}

impl Actor for Controller {
    type Context = Context<Self>;
}
