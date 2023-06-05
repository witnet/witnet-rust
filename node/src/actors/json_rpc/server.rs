use std::collections::HashMap;

use actix::prelude::*;
use futures_util::TryFutureExt;
use witty_jsonrpc::prelude::*;

use crate::{
    actors::messages::{BlockNotify, NodeStatusNotify, SuperBlockNotify},
    config_mngr,
    utils::stop_system_if_panicking,
};

use super::{SubscriptionResult, Subscriptions};

/// JSON RPC server
#[derive(Default)]
pub struct JsonRpcServer {
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
    /// Method to process the configuration received from ConfigManager
    fn initialize(&mut self, ctx: &mut <Self as Actor>::Context) {
        let subscriptions = self.subscriptions.clone();

        config_mngr::get()
            .and_then(|config| {
                let enabled = config.jsonrpc.enabled && config
                    .jsonrpc
                    .tcp_address
                    .or(config.jsonrpc.http_address)
                    .or(config.jsonrpc.ws_address)
                    .is_some();

                // Do not start the server if enabled = false or no transport is configured
                if !enabled {
                    log::debug!("JSON-RPC interface explicitly disabled by configuration or no address has been configured");
                    return futures::future::ok(None);
                }

                // Create multi-transport server
                let mut server = WittyMultiServer::new();

                // Attach JSON-RPC methods and subscriptions
                super::api::attach_api(
                    &mut server,
                    config.jsonrpc.enable_sensitive_methods,
                    subscriptions,
                    &Some(actix::System::current()),
                );

                // Add HTTP transport if enabled
                if let Some(address) = config.jsonrpc.http_address {
                    let address = address.to_string();
                    log::info!("HTTP JSON-RPC interface will listen on {}", address);
                    server.add_transport(witty_jsonrpc::transports::http::HttpTransport::new(
                        witty_jsonrpc::transports::http::HttpTransportSettings {
                            address
                        },
                    ));
                }
                // Add TCP transport if enabled
                if let Some(address) = config.jsonrpc.tcp_address {
                    let address = address.to_string();
                    log::info!("TCP JSON-RPC interface will listen on {}", address);
                    server.add_transport(witty_jsonrpc::transports::tcp::TcpTransport::new(
                        witty_jsonrpc::transports::tcp::TcpTransportSettings {
                            address
                        },
                    ));
                }
                // Add WebSockets transport if enabled
                if let Some(address) = config.jsonrpc.ws_address {
                    let address = address.to_string();
                    log::info!("WebSockets JSON-RPC interface will listen on {}", address);
                    server.add_transport(witty_jsonrpc::transports::ws::WsTransport::new(
                        witty_jsonrpc::transports::ws::WsTransportSettings {
                            address
                        },
                    ));
                }

                // Finally, try to start listening
                let server = server.start().ok().map(|_| server);

                futures::future::ok(server)
            })
            .into_actor(self)
            .and_then(move |server, act, ctx| {
                // If the server started successfully, attach it to the actor, otherwise call it a day
                if server.is_some() {
                    act.server = server;
                } else {
                    ctx.stop();
                }

                futures::future::ok(())
            })
            .map(|_res, _act, _ctx| ())
            .wait(ctx);
    }
}

impl Actor for JsonRpcServer {
    type Context = Context<Self>;

    /// Method to be executed when the actor is started
    fn started(&mut self, ctx: &mut Self::Context) {
        // Send message to config manager and process its response
        self.initialize(ctx);
    }
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
