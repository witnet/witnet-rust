use actix::prelude::*;
use actix::StreamHandler;
use tokio::net::{TcpListener, TcpStream};
use tokio_util::codec::FramedRead;

use std::{collections::HashMap, collections::HashSet, net::SocketAddr, rc::Rc, sync::Arc};

use super::{
    connection::JsonRpc, json_rpc_methods::jsonrpc_io_handler, newline_codec::NewLineCodec,
    SubscriptionResult, Subscriptions,
};
use crate::{
    actors::messages::{BlockNotify, InboundTcpConnect, NodeStatusNotify, SuperBlockNotify},
    config_mngr,
};
use futures_util::compat::Compat01As03;
use jsonrpc_pubsub::{PubSubHandler, Session};
use witnet_futures_utils::ActorFutureExt;

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
                    return fut::Either::left(fut::result(Ok(None)));
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

                let fut = async move {
                    // Bind TCP listener to this address
                    // FIXME(#176): running `yes | nc 127.0.0.1 1234` freezes the entire actor system
                    let listener = match TcpListener::bind(&server_addr).await {
                        Ok(listener) => listener,
                        Err(e) => {
                            // Shutdown the entire system on error
                            // For example, when the server_addr is already in use
                            // FIXME(#72): gracefully stop the system?
                            log::error!("Could not start JSON-RPC server: {:?}", e);
                            panic!("Could not start JSON-RPC server: {:?}", e);
                        }
                    };

                    Ok(Some((server_addr, listener)))
                }
                .into_actor(act);

                fut::Either::right(fut)
            })
            .and_then(|opt, _act, ctx| {
                if opt.is_none() {
                    return fut::ok(());
                }

                let (server_addr, listener) = opt.unwrap();
                // Add message stream which will return a InboundTcpConnect for each incoming TCP connection
                let stream = async_stream::stream! {
                    loop {
                        match listener.accept().await {
                            Ok((st, _addr)) => {
                                yield InboundTcpConnect::new(st);
                            }
                            Err(err) => {
                                log::error!("Error incoming listener: {}", err);
                            }
                        }
                    }
                };
                ctx.add_message_stream(stream);

                log::debug!("JSON-RPC interface is now running at {}", server_addr);

                fut::ok(())
            })
            .map_err(|err, _, _| log::error!("JsonRpcServer config failed: {}", err))
            .map(|_res: Result<(), ()>, _act, _ctx| ())
            .wait(ctx);
    }

    fn add_connection(&mut self, parent: Addr<JsonRpcServer>, stream: TcpStream) {
        log::debug!(
            "Add session (currently {} open connections)",
            1 + self.open_connections.len()
        );

        // Get a reference to the JSON-RPC method handler
        let jsonrpc_io = Rc::clone(self.jsonrpc_io.as_ref().unwrap());
        let (transport_sender_01, transport_receiver_01) =
            jsonrpc_core::futures::sync::mpsc::channel(16);
        let transport_receiver = Compat01As03::new(transport_receiver_01);

        // Create a new `JsonRpc` actor which will listen to this stream
        let addr = JsonRpc::create(|ctx| {
            let (r, w) = stream.into_split();
            JsonRpc::add_stream(FramedRead::new(r, NewLineCodec), ctx);
            JsonRpc::add_stream(transport_receiver, ctx);
            JsonRpc {
                framed: io::FramedWrite::new(w, NewLineCodec, ctx),
                parent,
                jsonrpc_io,
                session: Arc::new(Session::new(transport_sender_01)),
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

/// Unregister a closed connection from the list of open connections
pub struct Unregister {
    pub addr: Addr<JsonRpc>,
}

impl Message for Unregister {
    type Result = ();
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
                subs.get("blocks").unwrap_or(&empty_map)
            {
                log::debug!("Sending block notification!");
                let r = SubscriptionResult {
                    result: block.clone(),
                    subscription: subscription.clone(),
                };
                let fut01 = sink.notify(r.into());
                ctx.spawn(
                    Compat01As03::new(fut01)
                        .into_actor(self)
                        .then(move |res, _act, _ctx| {
                            if let Err(e) = res {
                                log::error!("Failed to send block notification: {:?}", e);
                            }

                            actix::fut::ok(())
                        })
                        .map(|_res: Result<(), ()>, _act, _ctx| ()),
                );
            }
        } else {
            log::error!("Failed to acquire lock in BlockNotify handle");
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
                    let fut01 = sink.notify(params);
                    ctx.spawn(
                        Compat01As03::new(fut01)
                            .into_actor(self)
                            .then(move |res, _, _| {
                                if let Err(e) = res {
                                    log::error!("Failed to send notification: {:?}", e);
                                }

                                actix::fut::ok(())
                            })
                            .map(|_res: Result<(), ()>, _act, _ctx| ()),
                    );
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

    fn handle(&mut self, msg: NodeStatusNotify, ctx: &mut Self::Context) -> Self::Result {
        if let Ok(subs) = self.subscriptions.lock() {
            let empty_map = HashMap::new();
            for (subscription, (sink, _subscription_params)) in
                subs.get("status").unwrap_or(&empty_map)
            {
                log::debug!("Sending node status notification ({:?})", msg.node_status);
                let r = SubscriptionResult {
                    result: serde_json::to_value(msg.node_status).unwrap(),
                    subscription: subscription.clone(),
                };
                let fut01 = sink.notify(r.into());
                ctx.spawn(
                    Compat01As03::new(fut01)
                        .into_actor(self)
                        .then(move |res, _act, _ctx| {
                            if let Err(e) = res {
                                log::error!("Failed to send node status: {:?}", e);
                            }

                            actix::fut::ok(())
                        })
                        .map(|_res: Result<(), ()>, _act, _ctx| ()),
                );
            }
        } else {
            log::error!("Failed to acquire lock in NodeStatusNotify handle");
        }
    }
}
