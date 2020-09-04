use actix::prelude::*;
// use actix::{
//     io::FramedWrite, Actor, ActorContext, Addr, AsyncContext, Context, Handler, Message,
//     StreamHandler,
// };
use tokio::{
    codec::FramedRead,
    io::AsyncRead,
    net::{TcpListener, TcpStream},
};

use futures::{sync::mpsc, Stream};
use std::{collections::HashMap, collections::HashSet, net::SocketAddr, rc::Rc, sync::Arc};

use super::{
    connection::JsonRpc, json_rpc_methods::jsonrpc_io_handler, newline_codec::NewLineCodec,
    SubscriptionResult, Subscriptions,
};
use crate::{
    actors::messages::{BlockNotify, InboundTcpConnect, SuperBlockNotify},
    config_mngr,
};
use jsonrpc_pubsub::{PubSubHandler, Session};

/// JSON RPC server
#[derive(Default)]
pub struct JsonRpcServer {
    /// Server address
    server_addr: Option<SocketAddr>,
    /// Open connections, stored as instances of the `JsonRpc` actor
    open_connections: HashSet<Addr<JsonRpc>>,
    /// JSON-RPC methods
    // Stored as an `Rc` to avoid creating a new handler for each connection
    jsonrpc_io: Option<Rc<PubSubHandler<Arc<Session>>>>,
    /// List of subscriptions
    subscriptions: Subscriptions,
}

/// Required traits for beInboundTcpConnecting able to retrieve storage manager address from registry
impl Supervised for JsonRpcServer {}
impl SystemService for JsonRpcServer {}

impl JsonRpcServer {
    /// Method to process the configuration received from ConfigManager
    fn process_config(&mut self, ctx: &mut <Self as Actor>::Context) {
        config_mngr::get()
            .into_actor(self)
            .and_then(|config, act, ctx| {
                let enabled = config.jsonrpc.enabled;

                // Do not start the server if enabled = false
                if !enabled {
                    log::debug!("JSON-RPC interface explicitly disabled by configuration.");
                    ctx.stop();
                    return fut::ok(());
                }

                log::debug!("Starting JSON-RPC interface.");
                let server_addr = config.jsonrpc.server_address;
                act.server_addr = Some(server_addr);
                // Create and store the JSON-RPC method handler
                let jsonrpc_io = jsonrpc_io_handler(
                    act.subscriptions.clone(),
                    config.jsonrpc.enable_sensitive_methods,
                );
                act.jsonrpc_io = Some(Rc::new(jsonrpc_io));

                // Bind TCP listener to this address
                // FIXME(#176): running `yes | nc 127.0.0.1 1234` freezes the entire actor system
                let listener = match TcpListener::bind(&server_addr) {
                    Ok(listener) => listener,
                    Err(e) => {
                        // Shutdown the entire system on error
                        // For example, when the server_addr is already in use
                        // FIXME(#72): gracefully stop the system?
                        log::error!("Could not start JSON-RPC server: {:?}", e);
                        panic!("Could not start JSON-RPC server: {:?}", e);
                    }
                };

                // Add message stream which will return a InboundTcpConnect for each incoming TCP connection
                ctx.add_message_stream(
                    listener
                        .incoming()
                        .map_err(|_| ())
                        .map(InboundTcpConnect::new),
                );

                log::debug!("JSON-RPC interface is now running at {}", server_addr);

                fut::ok(())
            })
            .map_err(|err, _, _| log::error!("JsonRpcServer config failed: {}", err))
            .wait(ctx);
    }

    fn add_connection(&mut self, parent: Addr<JsonRpcServer>, stream: TcpStream) {
        log::debug!(
            "Add session (currently {} open connections)",
            1 + self.open_connections.len()
        );

        // Get a reference to the JSON-RPC method handler
        let jsonrpc_io = Rc::clone(self.jsonrpc_io.as_ref().unwrap());
        let (transport_sender, transport_receiver) = mpsc::channel(16);

        // Create a new `JsonRpc` actor which will listen to this stream
        let addr = JsonRpc::create(|ctx| {
            let (r, w) = stream.split();
            JsonRpc::add_stream(FramedRead::new(r, NewLineCodec), ctx);
            JsonRpc::add_stream(transport_receiver, ctx);
            JsonRpc {
                framed: io::FramedWrite::new(w, NewLineCodec, ctx),
                parent,
                jsonrpc_io,
                session: Arc::new(Session::new(transport_sender)),
            }
        });

        // Store the actor address
        self.open_connections.insert(addr);
    }

    fn remove_connection(&mut self, addr: &Addr<JsonRpc>) {
        self.open_connections.remove(addr);
        log::debug!(
            "Remove session (currently {} open connections)",
            self.open_connections.len()
        );
    }
}

impl Actor for JsonRpcServer {
    type Context = Context<Self>;

    /// Method to be executed when the actor is started
    fn started(&mut self, ctx: &mut Self::Context) {
        // Send message to config manager and process its response
        self.process_config(ctx);
    }
}

/// Handler for InboundTcpConnect messages (built from inbound connections)
impl Handler<InboundTcpConnect> for JsonRpcServer {
    /// Response for message, which is defined by `ResponseType` trait
    type Result = ();

    /// Method to handle the InboundTcpConnect message
    fn handle(&mut self, msg: InboundTcpConnect, ctx: &mut Self::Context) {
        self.add_connection(ctx.address(), msg.stream);
    }
}

#[derive(Message)]
/// Unregister a closed connection from the list of open connections
pub struct Unregister {
    pub addr: Addr<JsonRpc>,
}

impl Handler<Unregister> for JsonRpcServer {
    type Result = ();

    /// Method to remove a finished session
    fn handle(&mut self, msg: Unregister, _ctx: &mut Context<Self>) -> Self::Result {
        self.remove_connection(&msg.addr);
    }
}

impl Handler<BlockNotify> for JsonRpcServer {
    type Result = ();

    fn handle(&mut self, msg: BlockNotify, ctx: &mut Self::Context) -> Self::Result {
        log::debug!("Got NewBlock message, sending notifications...");
        let block = serde_json::to_value(msg.block).unwrap();
        if let Ok(subs) = self.subscriptions.lock() {
            let empty_map = HashMap::new();
            for (subscription, (sink, _subscription_params)) in
                subs.get("newBlocks").unwrap_or(&empty_map)
            {
                log::debug!("Sending NewBlock notification!");
                let r = SubscriptionResult {
                    result: block.clone(),
                    subscription: subscription.clone(),
                };
                ctx.spawn(
                    sink.notify(r.into())
                        .into_actor(self)
                        .then(move |res, _act, _ctx| {
                            if let Err(e) = res {
                                log::error!("Failed to send notification: {:?}", e);
                            }

                            actix::fut::ok(())
                        }),
                );
            }
        } else {
            log::error!("Failed to acquire lock in NewBlock handle");
        }
    }
}

impl Handler<SuperBlockNotify> for JsonRpcServer {
    type Result = ();

    fn handle(&mut self, msg: SuperBlockNotify, ctx: &mut Self::Context) -> Self::Result {
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
                    let params = jsonrpc_core::Params::from(SubscriptionResult {
                        result: hashes.clone(),
                        subscription: subscription.clone(),
                    });
                    ctx.spawn(sink.notify(params).into_actor(self).then(move |res, _, _| {
                        if let Err(e) = res {
                            log::error!("Failed to send notification: {:?}", e);
                        }

                        actix::fut::ok(())
                    }));
                }
            } else {
                log::warn!("Failed to find a subscription for superblocks notifications");
            }
        } else {
            log::error!("Failed to acquire lock in SuperBlockNotify handle");
        }
    }
}
