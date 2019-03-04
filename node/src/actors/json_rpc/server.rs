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
use jsonrpc_core::IoHandler;
use log::*;
use std::{collections::HashSet, net::SocketAddr, rc::Rc};

use super::{
    connection::JsonRpc, json_rpc_methods::jsonrpc_io_handler, newline_codec::NewLineCodec,
};
use crate::actors::messages::InboundTcpConnect;
use crate::config_mngr;

/// JSON RPC server
#[derive(Default)]
pub struct JsonRpcServer {
    /// Server address
    server_addr: Option<SocketAddr>,
    /// Open connections, stored as instances of the `JsonRpc` actor
    open_connections: HashSet<Addr<JsonRpc>>,
    /// JSON-RPC methods
    // Stored as an `Rc` to avoid creating a new handler for each connection
    jsonrpc_io: Option<Rc<IoHandler<()>>>,
}

impl JsonRpcServer {
    /// Method to process the configuration received from ConfigManager
    fn process_config(&mut self, ctx: &mut <Self as Actor>::Context) {
        config_mngr::get()
            .into_actor(self)
            .and_then(|config, actor, ctx| {
                let enabled = config.jsonrpc.enabled;

                // Do not start the server if enabled = false
                if !enabled {
                    debug!("JSON-RPC interface explicitly disabled by configuration.");
                    ctx.stop();
                    return fut::ok(());
                }

                debug!("Starting JSON-RPC interface.");
                let server_addr = config.jsonrpc.server_address;
                actor.server_addr = Some(server_addr);
                // Create and store the JSON-RPC method handler
                let jsonrpc_io = jsonrpc_io_handler();
                actor.jsonrpc_io = Some(Rc::new(jsonrpc_io));

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

        // Create a new `JsonRpc` actor which will listen to this stream
        let addr = JsonRpc::create(|ctx| {
            let (r, w) = stream.split();
            JsonRpc::add_stream(FramedRead::new(r, NewLineCodec), ctx);
            JsonRpc {
                framed: io::FramedWrite::new(w, NewLineCodec, ctx),
                parent,
                jsonrpc_io,
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
    }
}
