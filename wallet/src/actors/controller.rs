//! # Controller actor
//!
//! The Controller actor holds the address of the App actor and the instance of the Websockets server, and is in charge of graceful shutdown of the entire system.
//! See `Controller` struct for more info.
use std::net;
use std::path::PathBuf;

use actix::prelude::*;
use futures::future;
use jsonrpc_core as rpc;
use jsonrpc_pubsub as pubsub;
use serde_json::{self as json, json};

use super::{app, App};
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

/// Helper macro to add multiple JSON-RPC methods at once
macro_rules! routes {
    ($io:expr, $app:expr $(,)?) => {};
    ($io:expr, $app:expr, ($method_jsonrpc:expr, $actor_msg:ty $(,)?), $($args:tt)*) => {
        {
            let app_addr = $app.clone();
            $io.add_method($method_jsonrpc, move |params: rpc::Params| {
                log::debug!("Handling request for method: {}", $method_jsonrpc);
                let addr = app_addr.clone();
                // Try to parse the request params into the actor message
                future::result(params.parse::<$actor_msg>())
                    .and_then(move |msg| {
                        // Then send the parsed message to the actor
                        addr.send(msg)
                            .map_err(error::Error::Mailbox)
                            .flatten()
                            .and_then(
                                |x|
                                future::result(json::to_value(x)).map_err(error::Error::Serialization)
                            )
                            .map_err(|err| error::ApiError::Execution(err).into())
                    })
            });
        }
        routes!($io, $app, $($args)*);
    };
}

/// Macro to add multiple JSON-RPC methods that forward the request to the Node at once
macro_rules! forwarded_routes {
    ($io:expr, $app:expr $(,)?) => {};
    ($io:expr, $app:expr, $method_jsonrpc:expr, $($args:tt)*) => {
        {
            let app_addr = $app.clone();
            $io.add_method($method_jsonrpc, move |params: rpc::Params| {
                log::debug!("Forwarding request for method: {}", $method_jsonrpc);
                app_addr.send(app::Forward($method_jsonrpc.to_string(), params))
                    .map_err(error::Error::Mailbox)
                    .flatten()
                    .and_then(|x| {
                        future::result(json::to_value(x)).map_err(error::Error::Serialization)
                    })
                    .map_err(|err| error::ApiError::Execution(err).into())
            });
        }
        forwarded_routes!($io, $app, $($args)*);
    };
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
        let app_addr = App::build()
            .node_url(self.node_url)
            .db_path(self.db_path)
            .start()?;
        let mut handler = pubsub::PubSubHandler::new(rpc::MetaIoHandler::default());

        handler.add_subscription(
            "notifications",
            ("subscribeNotifications", {
                let addr = app_addr.clone();
                move |_, _, subscriber| addr.do_send(app::Subscribe(subscriber))
            }),
            ("unsubscribeNotifications", {
                let addr = app_addr.clone();
                move |id, _| {
                    addr.send(app::Unsubscribe(id))
                        .map_err(error::Error::Mailbox)
                        .and_then(|_| future::ok(json!({"status": "ok"})))
                        .map_err(|err| error::ApiError::Execution(err).into())
                }
            }),
        );

        forwarded_routes!(
            handler,
            app_addr,
            "getBlock",
            "getBlockChain",
            "getOutput",
            "inventory",
        );

        routes!(
            handler,
            app_addr,
            ("getWalletInfos", app::GetWalletInfos),
            ("createMnemonics", app::CreateMnemonics),
            ("importSeed", app::ImportSeed),
            ("createWallet", app::CreateWallet),
            ("lockWallet", app::LockWallet),
            ("unlockWallet", app::UnlockWallet),
            ("getTransactions", app::GetTransactions),
            ("sendVTT", app::SendVtt),
            ("generateAddress", app::GenerateAddress),
            ("createDataRequest", app::CreateDataRequest),
            ("runRadRequest", app::RunRadRequest),
            ("sendDataRequest", app::SendDataRequest),
        );

        let server = Server::build()
            .handler(handler)
            .addr(self.server_addr)
            .start()
            .map_err(error::Error::Server)?;

        let controller = Controller {
            _app: app_addr,
            _server: server,
        };

        Ok(controller.start())
    }
}

impl Actor for Controller {
    type Context = Context<Self>;
}
