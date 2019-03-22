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

use futures::Stream;
use log::*;
use std::{collections::HashMap, collections::HashSet, net::SocketAddr, rc::Rc, sync::Arc};

use super::{
    connection::JsonRpc, json_rpc_methods::jsonrpc_io_handler, newline_codec::NewLineCodec,
};
use crate::actors::json_rpc::json_rpc_methods::{AddrJsonRpc, Subscriptions};
use crate::actors::messages::{InboundTcpConnect, NewBlock};
use crate::config_mngr;
use futures::sync::mpsc;
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
    jsonrpc_io: Option<Rc<PubSubHandler<AddrJsonRpc>>>,
    // TODO
    subscriptions: Subscriptions,
}

/// Required traits for being able to retrieve storage manager address from registry
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
                    debug!("JSON-RPC interface explicitly disabled by configuration.");
                    ctx.stop();
                    return fut::ok(());
                }

                debug!("Starting JSON-RPC interface.");
                let server_addr = config.jsonrpc.server_address;
                act.server_addr = Some(server_addr);
                // Create and store the JSON-RPC method handler
                let jsonrpc_io = jsonrpc_io_handler(act.subscriptions.clone());
                act.jsonrpc_io = Some(Rc::new(jsonrpc_io));

                // Bind TCP listener to this address
                // FIXME(#176): running `yes | nc 127.0.0.1 1234` freezes the entire actor system
                let listener = match TcpListener::bind(&server_addr) {
                    Ok(listener) => listener,
                    Err(e) => {
                        // Shutdown the entire system on error
                        // For example, when the server_addr is already in use
                        // FIXME(#72): gracefully stop the system?
                        error!("Could not start JSON-RPC server: {:?}", e);
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

                debug!("JSON-RPC interface is now running at {}", server_addr);

                fut::ok(())
            })
            .map_err(|err, _, _| log::error!("JsonRpcServer config failed: {}", err))
            .wait(ctx);
    }

    fn add_connection(&mut self, parent: Addr<JsonRpcServer>, stream: TcpStream) {
        debug!(
            "Add session (currently {} open connections)",
            1 + self.open_connections.len()
        );

        // Get a reference to the JSON-RPC method handler
        let jsonrpc_io = Rc::clone(self.jsonrpc_io.as_ref().unwrap());
        let (transport_sender, transport_receiver) = mpsc::channel(16);
        // TODO: transport_receiver should forward the message to framed.

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
        if let Ok(mut ss) = self.subscriptions.lock() {
            info!("Removing session from subscriptions map");
            //ss.retain(|k, v| v.retain(|x| ))
            for (_method, v) in ss.iter_mut() {
                v.remove(&msg.addr);
            }
        } else {
            log::error!("Failed to adquire lock in Unregister");
        }
    }
}

impl Handler<NewBlock> for JsonRpcServer {
    type Result = ();

    fn handle(&mut self, msg: NewBlock, ctx: &mut Self::Context) -> Self::Result {
        info!("Got NewBlock message, sending notifications...");
        let block = serde_json::to_value(msg.block).unwrap();
        if let Ok(subs) = self.subscriptions.lock() {
            let empty_map = HashMap::new();
            for v in subs.get("newBlocks").unwrap_or(&empty_map).values() {
                for (subscription, (sink, _subscription_params)) in v {
                    info!("Sending NewBlock notification!");
                    struct SubRes {
                        result: serde_json::Value,
                        subscription: jsonrpc_pubsub::SubscriptionId,
                    }
                    impl From<SubRes> for jsonrpc_core::Params {
                        fn from(x: SubRes) -> Self {
                            let mut map = serde_json::Map::new();
                            map.insert("result".to_string(), x.result);
                            map.insert(
                                "subscription".to_string(),
                                match x.subscription {
                                    jsonrpc_pubsub::SubscriptionId::Number(x) => {
                                        serde_json::Value::Number(serde_json::Number::from(x))
                                    }
                                    jsonrpc_pubsub::SubscriptionId::String(s) => {
                                        serde_json::Value::String(s)
                                    }
                                },
                            );
                            jsonrpc_core::Params::Map(map)
                        }
                    }
                    let r = SubRes {
                        result: block.clone(),
                        subscription: subscription.clone(),
                    };
                    let params = jsonrpc_core::Params::from(r);
                    ctx.spawn(
                        sink.notify(params)
                            .into_actor(self)
                            .then(|_res, _act, _ctx| {
                                info!("Actix sent the message");
                                actix::fut::ok(())
                            }),
                    );
                }
            }
        } else {
            // Mutex error
        }
    }
}
