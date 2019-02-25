//! Websockets JSON-RPC server

use actix::{Actor, Context, Handler, Message, Supervised, System, SystemRegistry, SystemService};
use futures::future::Future;
use jsonrpc_ws_server::jsonrpc_core::{IoHandler, Params, Value};
use jsonrpc_ws_server::{Server, ServerBuilder};
use serde::Deserialize;
use std::net::SocketAddr;

/// Start a WebSockets JSON-RPC server in a new thread and bind to address "addr".
/// Returns a handle which will close the server when dropped.
///
/// We need a reference to the registry because the JSON-RPC handlers will run on a new thread,
/// and that thread does not have access to the Actix system running in the main thread.
/// A nice feature is that when the Actix system has not been started yet, the messages are
/// simply queued and nothing is lost.
fn start_ws_jsonrpc_server(
    addr: &SocketAddr,
    registry: SystemRegistry,
) -> Result<Server, jsonrpc_ws_server::Error> {
    // JSON-RPC supported methods
    let mut io = IoHandler::new();
    let reg = registry.clone();
    io.add_method("say_hello", move |params: Params| {
        say_hello(reg.clone(), params.parse())
    });
    let reg = registry.clone();
    io.add_method("say_hello2", move |params: Params| {
        say_hello(reg.clone(), params.parse())
    });

    // Start the WebSockets JSON-RPC server in a new thread and bind to address "addr"
    ServerBuilder::new(io).start(addr)
}

/// Example JSON-RPC method parameters
#[derive(Deserialize)]
struct SayHelloParams {
    name: String,
}

/// Example JSON-RPC method
fn say_hello(
    registry: SystemRegistry,
    params: jsonrpc_ws_server::jsonrpc_core::Result<SayHelloParams>,
) -> impl Future<Item = Value, Error = jsonrpc_ws_server::jsonrpc_core::Error> {
    registry
        .get::<HiActor>()
        .send(Greeting {
            name: params.map(|x| x.name).unwrap_or("Anon".into()),
        })
        .then(|x| match x {
            Err(_) => Err(jsonrpc_ws_server::jsonrpc_core::Error::internal_error()),
            Ok(s) => {
                println!("JSON-RPC reply: {}", s);
                Ok(Value::String(s))
            }
        })
}

/// Example actor which will receive messages from the JSON-RPC handlers
#[derive(Clone, Debug, Default)]
struct HiActor;

impl Actor for HiActor {
    /// Every actor has to provide execution `Context` in which it can run
    type Context = Context<Self>;

    /// Method to be executed when the actor is started
    fn started(&mut self, _ctx: &mut Self::Context) {
        println!("HiActor actor has been started!");
    }
}

/// Required traits for being able to retrieve actor address from registry
impl Supervised for HiActor {}
impl SystemService for HiActor {}

/// Example message sent to example actor
struct Greeting {
    name: String,
}

impl Message for Greeting {
    type Result = String;
}

impl Handler<Greeting> for HiActor {
    type Result = String;

    fn handle(&mut self, msg: Greeting, _ctx: &mut Context<Self>) -> String {
        println!("Got greeting from {}!", msg.name);
        format!("Hi, {}!", msg.name)
    }
}

pub fn websockets_actix_poc() {
    // Actix
    let system = System::new("wallet");
    let s = System::current();

    // Start actor here just to check that it is only started in the main thread
    HiActor::from_registry().do_send(Greeting {
        name: "nobody, actor initialized".into(),
    });

    // This clone is implemented as an Arc::clone
    let registry = s.registry().clone();

    // WebSockets server address
    let addr = "127.0.0.1:3030".parse().unwrap();
    // Start server before calling system.run()
    let _ws_server_handle =
        start_ws_jsonrpc_server(&addr, registry).expect("Failed to start WebSockets server");

    // Because system.run() blocks
    let code = system.run();
    println!("Done, system exited with code {}", code);
}

// JavaScript code to send a request:
/*
var socket = new WebSocket('ws://localhost:3030');

socket.addEventListener('message', function (event) {
    console.log('Message from server', event.data);
});

socket.addEventListener('open', function (event) {
    socket.send('{"jsonrpc":"2.0","method":"say_hello","params":{"name": "Tomasz"},"id":"1"}')
});

*/
