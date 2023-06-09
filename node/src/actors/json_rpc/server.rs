use std::collections::HashMap;

use actix::prelude::*;
use witnet_config::config::{Config, JsonRPC};
use witty_jsonrpc::prelude::*;

use crate::{
    actors::messages::{BlockNotify, NodeStatusNotify, SuperBlockNotify},
    utils::stop_system_if_panicking,
};

use super::{SubscriptionResult, Subscriptions};

/// JSON RPC server
#[derive(Default)]
pub struct JsonRpcServer {
    config: JsonRPC,
    /// A multi-transport JSON-RPC server
    server: Option<WittyMultiServer>,
    /// List of subscriptions
    subscriptions: Subscriptions,
}

impl Drop for JsonRpcServer {
    fn drop(&mut self) {
        log::trace!("Dropping JsonRpcServer");
        stop_system_if_panicking("JsonRpcServer");
    }
}

/// Required traits for beInboundTcpConnecting able to retrieve storage manager address from registry
impl Supervised for JsonRpcServer {}
impl SystemService for JsonRpcServer {}

impl JsonRpcServer {
    /// Create a new instance of JsonRpcServer with configuration data in place.
    pub fn from_config(config: &Config) -> Self {
        let mut server = Self::default();
        server.config = config.jsonrpc.clone();

        server
    }

    /// Method to process the configuration received from ConfigManager
    pub fn initialize(mut self, runtime: tokio::runtime::Handle) -> Result<Self, failure::Error> {
        let subscriptions = self.subscriptions.clone();

        let enabled = self.config.enabled
            && self
                .config
                .tcp_address
                .or(self.config.http_address)
                .or(self.config.ws_address)
                .is_some();

        // Do not start the server if enabled = false or no transport is configured
        if !enabled {
            log::warn!("JSON-RPC interface explicitly disabled by configuration or no address has been configured");

            return Ok(self);
        }

        // Create multi-transport server
        let mut server = WittyMultiServer::new().with_runtime(runtime);

        // Attach JSON-RPC methods and subscriptions
        super::api::attach_api(
            &mut server,
            self.config.enable_sensitive_methods,
            subscriptions,
            &Some(actix::System::current()),
        );

        // Add HTTP transport if enabled
        if let Some(address) = self.config.http_address {
            let address = address.to_string();
            log::info!("HTTP JSON-RPC interface will listen on {}", address);
            server.add_transport(witty_jsonrpc::transports::http::HttpTransport::new(
                witty_jsonrpc::transports::http::HttpTransportSettings { address },
            ));
        }
        // Add TCP transport if enabled
        if let Some(address) = self.config.tcp_address {
            let address = address.to_string();
            log::info!("TCP JSON-RPC interface will listen on {}", address);
            server.add_transport(witty_jsonrpc::transports::tcp::TcpTransport::new(
                witty_jsonrpc::transports::tcp::TcpTransportSettings { address },
            ));
        }
        // Add WebSockets transport if enabled
        if let Some(address) = self.config.ws_address {
            let address = address.to_string();
            log::info!("WebSockets JSON-RPC interface will listen on {}", address);
            server.add_transport(witty_jsonrpc::transports::ws::WsTransport::new(
                witty_jsonrpc::transports::ws::WsTransportSettings { address },
            ));
        }

        // Finally, try to start listening.
        // If it starts successfully, attach it to the actor, otherwise call it a day
        match server.start() {
            Ok(_) => {
                log::info!("JSON-RPC server is now up and running on all configured transports.");
                self.server = Some(server);
            }
            Err(error) => {
                log::error!("Error trying to start JSON-RPC server: {:?}", error);

                return Err(error.into());
            }
        }

        Ok(self)
    }
}

impl Actor for JsonRpcServer {
    type Context = Context<Self>;

    /// Method to be executed when the actor is started.
    ///
    /// Because this actor is mostly initiated outside of the context of actix and before anything
    /// else, we do nothing here.
    fn started(&mut self, _ctx: &mut Self::Context) {}
}

impl Handler<BlockNotify> for JsonRpcServer {
    type Result = ();

    fn handle(&mut self, msg: BlockNotify, _ctx: &mut Self::Context) -> Self::Result {
        log::debug!("Got NewBlock message, sending notifications...");
        let block = serde_json::to_value(msg.block).unwrap();
        if let Ok(subs) = self.subscriptions.lock() {
            let empty_map = HashMap::new();
            for (subscription, (sink, _subscription_params)) in
                subs.get("blocks").unwrap_or(&empty_map)
            {
                log::debug!("Sending block notification!");
                let notification = jsonrpc_core::Params::from(SubscriptionResult {
                    result: block.clone(),
                    subscription: subscription.clone(),
                });
                if let Err(e) = sink.notify(notification) {
                    log::error!("Failed to send notification: {:?}", e);
                }
            }
        } else {
            log::error!("Failed to acquire lock in BlockNotify handle");
        }
    }
}

impl Handler<SuperBlockNotify> for JsonRpcServer {
    type Result = ();

    fn handle(&mut self, msg: SuperBlockNotify, _ctx: &mut Self::Context) -> Self::Result {
        log::debug!("Got SuperBlockNotify message, sending notifications...");
        log::trace!(
            "Notifying consolidation of 1 superblock and {} blocks: {:?}",
            msg.consolidated_block_hashes.len(),
            msg.consolidated_block_hashes
        );

        let hashes = serde_json::to_value(msg)
            .expect("JSON serialization of SuperBlockNotify should never fail");
        if let Ok(subscriptions) = self.subscriptions.lock() {
            if let Some(superblocks_subscriptions) = subscriptions.get("superblocks") {
                for (subscription, (sink, _params)) in superblocks_subscriptions {
                    log::debug!("Sending superblock notification through sink {:?}", sink);
                    let notification = jsonrpc_core::Params::from(SubscriptionResult {
                        result: hashes.clone(),
                        subscription: subscription.clone(),
                    });
                    if let Err(e) = sink.notify(notification) {
                        log::error!("Failed to send notification: {:?}", e);
                    }
                }
            } else {
                log::debug!("No subscriptions for superblocks notifications");
            }
        } else {
            log::error!("Failed to acquire lock in SuperBlockNotify handle");
        }
    }
}

impl Handler<NodeStatusNotify> for JsonRpcServer {
    type Result = ();

    fn handle(&mut self, msg: NodeStatusNotify, _ctx: &mut Self::Context) -> Self::Result {
        if let Ok(subs) = self.subscriptions.lock() {
            let empty_map = HashMap::new();
            for (subscription, (sink, _subscription_params)) in
                subs.get("status").unwrap_or(&empty_map)
            {
                log::debug!("Sending node status notification ({:?})", msg.node_status);
                let notification = jsonrpc_core::Params::from(SubscriptionResult {
                    result: serde_json::to_value(msg.node_status).unwrap(),
                    subscription: subscription.clone(),
                });
                if let Err(e) = sink.notify(notification) {
                    log::error!("Failed to send notification: {:?}", e);
                }
            }
        } else {
            log::error!("Failed to acquire lock in NodeStatusNotify handle");
        }
    }
}
