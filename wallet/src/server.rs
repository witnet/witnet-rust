//! Websockets JSON-RPC server

use actix::{Actor, ActorFuture, Arbiter, Context, Handler, Message, ResponseActFuture, Supervised, System, SystemRegistry, SystemService, WrapFuture};
use futures::future::Future;
use jsonrpc_ws_server::jsonrpc_core::{IoHandler, Params, Value};
use jsonrpc_ws_server::{Server, ServerBuilder};
use serde::Deserialize;
use std::net::SocketAddr;
use async_jsonrpc_client::Transport;
use async_jsonrpc_client::transports::tcp::TcpSocket;
use async_jsonrpc_client::transports::shared::EventLoopHandle;
use serde_json::json;
use async_jsonrpc_client::BatchTransport;

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
        say_hello(&reg, params.parse())
    });
    let reg = registry.clone();
    io.add_method("getBlockChain", move |params: Params| {
        forward_call("getBlockChain",&reg, params.parse())
    });
    let reg = registry.clone();
    io.add_method("inventory", move |params: Params| {
        forward_call("inventory",&reg, params.parse())
    });
    let reg = registry.clone();
    io.add_method("getOutput", move |params: Params| {
        forward_call("getOutput",&reg, params.parse())
    });

    // Start the WebSockets JSON-RPC server in a new thread and bind to address "addr"
    ServerBuilder::new(io).start(addr)
}

/// Forwards a JSON-RPC call to the node
fn forward_call(method: &str, registry: &SystemRegistry, params: jsonrpc_ws_server::jsonrpc_core::Result<Value>) -> impl Future<Item = Value, Error = jsonrpc_ws_server::jsonrpc_core::Error> {
    registry
        .get::<JsonRpcClient>()
        .send(JsonRpcMsg::new(method, params.unwrap_or(Value::Null)))
        .then(|x| match x {
            Err(e) => {
                let mut err = jsonrpc_ws_server::jsonrpc_core::Error::internal_error();
                err.message = e.to_string();
                Err(err)
            },
            Ok(s) => {
                match s {
                    Ok(s) => {
                        Ok(Value::from(s))
                    }
                    Err(e) => {
                        let mut err = jsonrpc_ws_server::jsonrpc_core::Error::internal_error();
                        err.message = e;
                        Err(err)
                    }
                }
            }
        })
}

/// Example JSON-RPC method parameters
#[derive(Deserialize)]
struct SayHelloParams {
    name: String,
}

/// Example JSON-RPC method
fn say_hello(
    registry: &SystemRegistry,
    params: jsonrpc_ws_server::jsonrpc_core::Result<SayHelloParams>,
) -> impl Future<Item = Value, Error = jsonrpc_ws_server::jsonrpc_core::Error> {
    registry
        .get::<HiActor>()
        .send(Greeting {
            name: params.map(|x| x.name).unwrap_or("Anon".into()),
        })
        .then(|x| match x {
            Err(e) => {
                let mut err = jsonrpc_ws_server::jsonrpc_core::Error::internal_error();
                err.message = e.to_string();
                Err(err)
            },
            Ok(s) => {
                println!("(1): JSON-RPC reply: {}", s);
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

    let jsonrpc_ws_client = JsonRpcClient::new("127.0.0.1:1234");
    s.registry().set(jsonrpc_ws_client.start());

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

struct JsonRpcClient {
    handle: EventLoopHandle,
    s: TcpSocket,
}

impl JsonRpcClient {
    fn new(url: &str) -> Self {
        let (handle, s) = TcpSocket::new(url).unwrap();
        Self {
            handle,
            s,
        }
    }
}

impl Default for JsonRpcClient {
    fn default() -> Self {
        Self::new("127.0.0.1:1234")
    }
}

impl Actor for JsonRpcClient {
    /// Every actor has to provide execution `Context` in which it can run
    type Context = Context<Self>;

    /// Method to be executed when the actor is started
    fn started(&mut self, _ctx: &mut Self::Context) {
        println!("JsonRpcClient actor has been started at address {}", self.s.addr());
    }
}

/// Required traits for being able to retrieve actor address from registry
impl Supervised for JsonRpcClient {}
impl SystemService for JsonRpcClient {}



struct JsonRpcMsg {
    method: String,
    params: serde_json::Value,
}

impl JsonRpcMsg {
    fn new<A: Into<String>, B: Into<serde_json::Value>>(method: A, params: B) -> Self {
        Self {
            method: method.into(),
            params: params.into(),
        }
    }
}

impl Message for JsonRpcMsg {
    type Result = Result<String, String>;
}

impl Handler<JsonRpcMsg> for JsonRpcClient {
    type Result = ResponseActFuture<Self, String, String>;

    fn handle(&mut self, msg: JsonRpcMsg, _ctx: &mut Context<Self>) -> Self::Result {
        println!("Calling {} with params {}", msg.method, msg.params);
        let fut = self.s.execute(&msg.method, msg.params).into_actor(self).then(|x, act, ctx| actix::fut::result(match x {
            Ok(x) => serde_json::to_string(&x).map_err(|e| e.to_string()),
            Err(e) => {
                println!("Error: {}", e);
                Err(e.to_string())
            },
        }));

        Box::new(fut)
    }
}