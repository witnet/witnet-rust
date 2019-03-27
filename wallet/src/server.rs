//! Websockets JSON-RPC server

use actix::{
    Actor, ActorFuture, AsyncContext, Context, ContextFutureSpawner, Handler, Message,
    ResponseActFuture, StreamHandler, Supervised, System, SystemRegistry, SystemService,
    WrapFuture,
};
use async_jsonrpc_client::{
    transports::{shared::EventLoopHandle, tcp::TcpSocket},
    DuplexTransport, Transport,
};
use futures::{future::Future, stream::Stream};
use jsonrpc_pubsub::{PubSubHandler, Session, Subscriber, SubscriptionId};
use jsonrpc_ws_server::{
    jsonrpc_core,
    jsonrpc_core::{MetaIoHandler, Params, Value},
    RequestContext, Server, ServerBuilder,
};

use log::{debug, info, warn};
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    net::SocketAddr,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc, Mutex,
    },
};

/// List of subscriptions from the websockects client (sheikah)
// TODO: this is defined twice: once here and once in node/json_rpc_methods?
pub type Subscriptions = Arc<
    Mutex<
        HashMap<
            &'static str,
            HashMap<
                jsonrpc_pubsub::SubscriptionId,
                (
                    jsonrpc_pubsub::Sink,
                    Option<jsonrpc_pubsub::SubscriptionId>,
                    Value,
                ),
            >,
        >,
    >,
>;

// Helper macro to add multiple JSON-RPC methods at once
macro_rules! add_methods {
    // No args: do nothing
    ($io:expr, $reg:expr $(,)*) => {};
    // add_methods!(io, reg, ("getBlockChain", get_block_chain))
    ($io:expr, $reg:expr, ($method_jsonrpc:expr, $method_rust:expr $(,)*), $($args:tt)*) => {
        // Base case:
        {
            let reg = $reg.clone();
            $io.add_method($method_jsonrpc, move |params: Params| {
                $method_rust(&reg, params.parse())
            });
        }
        // Recursion!
        add_methods!($io, $reg, $($args)*);
    };
}

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
    let mut io = PubSubHandler::new(MetaIoHandler::default());

    add_methods!(
        io,
        registry,
        ("say_hello", say_hello),
        ("getBlockChain", |r, p| forward_call("getBlockChain", r, p)),
        ("inventory", |r, p| forward_call("inventory", r, p)),
        ("getBlock", |r, p| forward_call("getBlock", r, p)),
        ("getOutput", |r, p| forward_call("getOutput", r, p)),
        ("getWalletInfos", get_wallet_infos),
        ("createMnemonics", create_mnemonics),
        ("importSeed", import_seed),
        ("createWallet", create_wallet),
        ("unlockWallet", unlock_wallet),
        ("getTransactions", get_transactions),
        ("sendVTT", send_vtt),
        ("generateAddress", generate_address),
        ("createDataRequest", create_data_request),
        ("runDataRequest", run_data_request),
        ("sendDataRequest", send_data_request),
        ("lockWallet", lock_wallet),
    );

    // We need two Arcs, one for subscribe and one for unsuscribe
    let registryu = registry.clone();
    let atomic_counter = AtomicUsize::new(1);
    io.add_subscription(
        "witnet_subscription",
        (
            "witnet_subscribe",
            move |params: Params, _meta, subscriber: Subscriber| {
                debug!("Called witnet_subscribe");
                let params_vec: Vec<Value> = match params {
                    Params::Array(v) => v,
                    _ => {
                        // Ignore errors with `.ok()` because an error here means the connection was closed
                        subscriber
                            .reject(jsonrpc_core::Error::invalid_params("Expected array"))
                            .ok();
                        return;
                    }
                };

                let method_name: String = match serde_json::from_value(params_vec[0].clone()) {
                    Ok(s) => s,
                    Err(e) => {
                        // Ignore errors with `.ok()` because an error here means the connection was closed
                        subscriber
                            .reject(jsonrpc_core::Error::invalid_params(e.to_string()))
                            .ok();
                        return;
                    }
                };

                let method_params = params_vec.get(1).cloned().unwrap_or_default();

                let add_subscription = |method_name: String, subscriber: Subscriber| {
                    let idd = atomic_counter.fetch_add(1, Ordering::SeqCst).to_string();
                    let id = SubscriptionId::String(idd.clone());
                    if let Ok(sink) = subscriber.assign_id(id.clone()) {
                        registry
                            .get::<JsonRpcClient>()
                            .do_send(JsonRpcForwardSubscribeMsg::new(
                                method_name.to_string(),
                                method_params,
                                idd,
                                sink,
                            ));
                    } else {
                        // Session closed before we got a chance to reply
                        debug!("Failed to assing id: session closed");
                    }
                };

                match method_name.as_str() {
                    "newBlocks" => {
                        debug!("New subscription to {}", method_name);
                        add_subscription(method_name, subscriber);
                    }
                    e => {
                        debug!("Unknown subscription method: {}", e);
                        // Ignore errors with `.ok()` because an error here means the connection was closed
                        subscriber
                            .reject(jsonrpc_core::Error::invalid_params(format!(
                                "Unknown subscription: {}",
                                e
                            )))
                            .ok();
                        return;
                    }
                }
            },
        ),
        (
            "witnet_unsubscribe",
            move |id: SubscriptionId,
                  _meta: Option<Arc<Session>>|
                  -> Box<dyn Future<Item = Value, Error = jsonrpc_core::Error> + Send> {
                debug!("Closing subscription {:?}", id);
                // When the session is closed, meta is none and the lock cannot be acquired
                Box::new(
                    registryu
                        .get::<JsonRpcClient>()
                        .send(ForwardUnsubscribe { client_id: id })
                        .then(|r| match r {
                            Ok(Ok(v)) => Ok(v),
                            Ok(Err(e)) => Err(e),
                            Err(e) => {
                                let mut err = jsonrpc_core::Error::internal_error();
                                err.message = e.to_string();
                                Err(err)
                            }
                        }),
                )
            },
        ),
    );

    // Start the WebSockets JSON-RPC server in a new thread and bind to address "addr"
    ServerBuilder::with_meta_extractor(io, |context: &RequestContext| {
        Arc::new(Session::new(context.sender()))
    })
    .start(addr)
}

#[derive(Debug, Deserialize)]
struct LockWalletParams {
    wallet_id: String,
    #[serde(default)] // default to false
    wipe: bool,
}

fn lock_wallet(
    _registry: &SystemRegistry,
    params: jsonrpc_core::Result<LockWalletParams>,
) -> impl Future<Item = Value, Error = jsonrpc_core::Error> {
    let _params = match params {
        Ok(x) => x,
        Err(e) => return Box::new(futures::failed(e)),
    };

    let x = true;
    Box::new(futures::done(serde_json::to_value(x).map_err(|e| {
        let mut err = jsonrpc_core::Error::internal_error();
        err.message = e.to_string();
        err
    })))
}

fn send_data_request(
    _registry: &SystemRegistry,
    params: jsonrpc_core::Result<DataRequest>,
) -> impl Future<Item = Value, Error = jsonrpc_core::Error> {
    let _params = match params {
        Ok(x) => x,
        Err(e) => return Box::new(futures::failed(e)),
    };

    let x = Transaction {};
    Box::new(futures::done(serde_json::to_value(x).map_err(|e| {
        let mut err = jsonrpc_core::Error::internal_error();
        err.message = e.to_string();
        err
    })))
}

// TODO: radon crate
#[derive(Serialize)]
struct RadonValue {}

fn run_data_request(
    _registry: &SystemRegistry,
    params: jsonrpc_core::Result<DataRequest>,
) -> impl Future<Item = Value, Error = jsonrpc_core::Error> {
    let _params = match params {
        Ok(x) => x,
        Err(e) => return Box::new(futures::failed(e)),
    };

    let x = RadonValue {};
    Box::new(futures::done(serde_json::to_value(x).map_err(|e| {
        let mut err = jsonrpc_core::Error::internal_error();
        err.message = e.to_string();
        err
    })))
}

#[derive(Debug, Deserialize)]
struct CreateDataRequestParams {
    not_before: u64,
    retrieve: Vec<RADRetrieveArgs>,
    aggregate: RADAggregateArgs,
    consensus: RADConsensusArgs,
    deliver: Vec<RADDeliverArgs>,
}

// TODO: radon crate
#[derive(Debug, Deserialize, Serialize)]
struct RADType(String);

#[derive(Debug, Deserialize, Serialize)]
struct RADRetrieveArgs {
    kind: RADType,
    url: String,
    script: Vec<Value>,
}

#[derive(Debug, Deserialize, Serialize)]
struct RADAggregateArgs {
    script: Vec<Value>,
}

#[derive(Debug, Deserialize, Serialize)]
struct RADConsensusArgs {
    script: Vec<Value>,
}

#[derive(Debug, Deserialize, Serialize)]
struct RADDeliverArgs {
    kind: RADType,
    url: String,
}

// TODO: data_structures crate
#[derive(Debug, Deserialize, Serialize)]
struct DataRequest {}

fn create_data_request(
    _registry: &SystemRegistry,
    params: jsonrpc_core::Result<CreateDataRequestParams>,
) -> impl Future<Item = Value, Error = jsonrpc_core::Error> {
    let _params = match params {
        Ok(x) => x,
        Err(e) => return Box::new(futures::failed(e)),
    };

    let x = DataRequest {};
    Box::new(futures::done(serde_json::to_value(x).map_err(|e| {
        let mut err = jsonrpc_core::Error::internal_error();
        err.message = e.to_string();
        err
    })))
}

// TODO: is this pkh?
#[derive(Serialize)]
struct Address {}

#[derive(Debug, Deserialize)]
struct GenerateAddressParams {
    wallet_id: String,
}

fn generate_address(
    _registry: &SystemRegistry,
    params: jsonrpc_ws_server::jsonrpc_core::Result<GenerateAddressParams>,
) -> impl Future<Item = Value, Error = jsonrpc_ws_server::jsonrpc_core::Error> {
    match params {
        Ok(x) => x,
        Err(e) => return Box::new(futures::failed(e)),
    };

    let x = Address {};
    Box::new(futures::done(serde_json::to_value(x).map_err(|e| {
        let mut err = jsonrpc_core::Error::internal_error();
        err.message = e.to_string();
        err
    })))
}

#[derive(Debug, Deserialize)]
struct SendVttParams {
    wallet_id: String,
    to_address: Vec<u8>,
    amount: u64,
    fee: u64,
    subject: String,
}

fn send_vtt(
    _registry: &SystemRegistry,
    params: jsonrpc_core::Result<SendVttParams>,
) -> impl Future<Item = Value, Error = jsonrpc_core::Error> {
    let _params = match params {
        Ok(x) => x,
        Err(e) => return Box::new(futures::failed(e)),
    };

    let x = Transaction {};
    Box::new(futures::done(serde_json::to_value(x).map_err(|e| {
        let mut err = jsonrpc_core::Error::internal_error();
        err.message = e.to_string();
        err
    })))
}

#[derive(Debug, Deserialize)]
struct GetTransactionsParams {
    wallet_id: String,
    limit: u32,
    page: u32,
}

// TODO: import data_structures crate
#[derive(Serialize)]
struct Transaction {}

fn get_transactions(
    _registry: &SystemRegistry,
    params: jsonrpc_core::Result<GetTransactionsParams>,
) -> impl Future<Item = Value, Error = jsonrpc_core::Error> {
    let _params = match params {
        Ok(x) => x,
        Err(e) => return Box::new(futures::failed(e)),
    };

    let x: Vec<Transaction> = vec![];
    Box::new(futures::done(serde_json::to_value(x).map_err(|e| {
        let mut err = jsonrpc_core::Error::internal_error();
        err.message = e.to_string();
        err
    })))
}

#[derive(Debug, Deserialize, Serialize)]
struct UnlockWalletParams {
    id: String,
    password: String,
}

fn unlock_wallet(
    _registry: &SystemRegistry,
    params: jsonrpc_core::Result<UnlockWalletParams>,
) -> impl Future<Item = Value, Error = jsonrpc_core::Error> {
    let _params = match params {
        Ok(x) => x,
        Err(e) => return Box::new(futures::failed(e)),
    };

    let x = Wallet::for_test();
    Box::new(futures::done(serde_json::to_value(x).map_err(|e| {
        let mut err = jsonrpc_core::Error::internal_error();
        err.message = e.to_string();
        err
    })))
}

#[derive(Debug, Deserialize, Serialize)]
struct Wallet {
    version: u32,
    info: WalletInfo,
    seed: SeedInfo,
    epochs: EpochsInfo,
    purpose: DerivationPath,
    accounts: Vec<Account>,
}

impl Wallet {
    fn for_test() -> Self {
        Wallet {
            version: 0,
            info: WalletInfo {
                id: "".to_string(),
                caption: "".to_string(),
            },
            seed: SeedInfo::Wip3(Seed(vec![])),
            epochs: EpochsInfo { last: 0, born: 0 },
            purpose: DerivationPath("m/44'/60'/0'/0".to_string()),
            accounts: vec![],
        }
    }
}

#[derive(Debug, Deserialize, Serialize)]
enum SeedInfo {
    Wip3(Seed),
}

#[derive(Debug, Deserialize, Serialize)]
struct Seed(Vec<u8>);

#[derive(Debug, Deserialize, Serialize)]
struct EpochsInfo {
    last: u32,
    born: u32,
}

#[derive(Debug, Deserialize, Serialize)]
struct DerivationPath(String);

#[derive(Debug, Deserialize, Serialize)]
struct Account {
    key_path: KeyPath,
    key_chains: Vec<KeyChain>,
    balance: u64,
}

#[derive(Debug, Deserialize, Serialize)]
struct KeyPath(Vec<ChildNumber>);

#[derive(Debug, Deserialize, Serialize)]
struct ChildNumber(u32);

#[derive(Debug, Deserialize, Serialize)]
enum KeyChain {
    External,
    Internal,
    Rad,
}

#[derive(Debug, Deserialize)]
struct CreateWalletParams {
    name: String,
    password: String,
}

fn create_wallet(
    _registry: &SystemRegistry,
    params: jsonrpc_core::Result<CreateWalletParams>,
) -> impl Future<Item = Value, Error = jsonrpc_core::Error> {
    let _params = match params {
        Ok(x) => x,
        Err(e) => return Box::new(futures::failed(e)),
    };

    let x = Wallet::for_test();
    Box::new(futures::done(serde_json::to_value(x).map_err(|e| {
        let mut err = jsonrpc_core::Error::internal_error();
        err.message = e.to_string();
        err
    })))
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum ImportSeedParams {
    Mnemonics { mnemonics: Mnemonics },
    Seed { seed: String },
}

fn import_seed(
    _registry: &SystemRegistry,
    params: jsonrpc_core::Result<ImportSeedParams>,
) -> impl Future<Item = Value, Error = jsonrpc_core::Error> {
    let _params = match params {
        Ok(x) => x,
        Err(e) => return Box::new(futures::failed(e)),
    };

    let x = true;
    Box::new(futures::done(serde_json::to_value(x).map_err(|e| {
        let mut err = jsonrpc_core::Error::internal_error();
        err.message = e.to_string();
        err
    })))
}

// TODO: implemented in PR #432
#[derive(Debug, Deserialize, Serialize)]
struct Mnemonics {}

fn create_mnemonics(
    _registry: &SystemRegistry,
    params: jsonrpc_core::Result<()>,
) -> impl Future<Item = Value, Error = jsonrpc_core::Error> {
    match params {
        Ok(x) => x,
        Err(e) => return Box::new(futures::failed(e)),
    };

    let x = Mnemonics {};
    Box::new(futures::done(serde_json::to_value(x).map_err(|e| {
        let mut err = jsonrpc_core::Error::internal_error();
        err.message = e.to_string();
        err
    })))
}

#[derive(Debug, Deserialize, Serialize)]
struct WalletInfo {
    id: String,
    caption: String,
}

fn get_wallet_infos(
    _registry: &SystemRegistry,
    params: jsonrpc_core::Result<()>,
) -> impl Future<Item = Value, Error = jsonrpc_core::Error> {
    match params {
        Ok(x) => x,
        Err(e) => return Box::new(futures::failed(e)),
    };

    let x: Vec<WalletInfo> = vec![];
    Box::new(futures::done(serde_json::to_value(x).map_err(|e| {
        let mut err = jsonrpc_core::Error::internal_error();
        err.message = e.to_string();
        err
    })))
}

/// Forwards a JSON-RPC call to the node
fn forward_call(
    method: &str,
    registry: &SystemRegistry,
    params: jsonrpc_core::Result<Value>,
) -> impl Future<Item = Value, Error = jsonrpc_core::Error> {
    registry
        .get::<JsonRpcClient>()
        .send(JsonRpcMsg::new(method, params.unwrap_or(Value::Null)))
        .then(|x| match x {
            Err(e) => {
                let mut err = jsonrpc_core::Error::internal_error();
                err.message = e.to_string();
                Err(err)
            }
            Ok(s) => match s {
                Ok(s) => Ok(s),
                Err(e) => {
                    let mut err = jsonrpc_core::Error::internal_error();
                    err.message = e;
                    Err(err)
                }
            },
        })
}

/// Example JSON-RPC method parameters
#[derive(Debug, Deserialize)]
struct SayHelloParams {
    name: String,
}

/// Example JSON-RPC method
fn say_hello(
    registry: &SystemRegistry,
    params: jsonrpc_core::Result<SayHelloParams>,
) -> impl Future<Item = Value, Error = jsonrpc_core::Error> {
    registry
        .get::<HiActor>()
        .send(Greeting {
            name: params.map(|x| x.name).unwrap_or_else(|_| "Anon".into()),
        })
        .then(|x| match x {
            Err(e) => {
                let mut err = jsonrpc_core::Error::internal_error();
                err.message = e.to_string();
                Err(err)
            }
            Ok(s) => {
                debug!("(1): JSON-RPC reply: {}", s);
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
        debug!("HiActor actor has been started!");
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
        debug!("Got greeting from {}!", msg.name);
        format!("Hi, {}!", msg.name)
    }
}
/// poc
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
    info!("Done, system exited with code {}", code);
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

#[derive(Debug)]
struct JsonRpcClient {
    handle: EventLoopHandle,
    s: TcpSocket,
    subscriptions: Subscriptions,
    node_id_to_client_id: HashMap<SubscriptionId, SubscriptionId>,
}

impl JsonRpcClient {
    fn new(url: &str) -> Self {
        let (handle, s) = TcpSocket::new(url).unwrap();
        Self {
            handle,
            s,
            subscriptions: Default::default(),
            node_id_to_client_id: Default::default(),
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
        debug!(
            "JsonRpcClient actor has been started at address {}",
            self.s.addr()
        );
    }
}

/// Required traits for being able to retrieve actor address from registry
impl Supervised for JsonRpcClient {}
impl SystemService for JsonRpcClient {}

struct JsonRpcMsg {
    method: String,
    params: Value,
}

impl JsonRpcMsg {
    fn new<A: Into<String>, B: Into<Value>>(method: A, params: B) -> Self {
        Self {
            method: method.into(),
            params: params.into(),
        }
    }
}

impl Message for JsonRpcMsg {
    type Result = Result<Value, String>;
}

impl Handler<JsonRpcMsg> for JsonRpcClient {
    type Result = ResponseActFuture<Self, Value, String>;

    fn handle(&mut self, msg: JsonRpcMsg, _ctx: &mut Context<Self>) -> Self::Result {
        debug!(
            "Calling node method {} with params {}",
            msg.method, msg.params
        );
        let fut = self
            .s
            .execute(&msg.method, msg.params)
            .into_actor(self)
            .then(|x, _act, _ctx| {
                actix::fut::result(match x {
                    Ok(x) => Ok(x),
                    Err(e) => {
                        warn!("Error: {}", e);
                        Err(e.to_string())
                    }
                })
            });

        Box::new(fut)
    }
}

struct JsonRpcSubscribeMsg {
    method: String,
    params: Value,
}

impl JsonRpcSubscribeMsg {
    // TODO: remove once used
    #[allow(unused)]
    fn new<A: Into<String>, B: Into<Value>>(method: A, params: B) -> Self {
        Self {
            method: method.into(),
            params: params.into(),
        }
    }
}

impl Message for JsonRpcSubscribeMsg {
    type Result = Result<String, String>;
}

impl Handler<JsonRpcSubscribeMsg> for JsonRpcClient {
    type Result = ResponseActFuture<Self, String, String>;

    fn handle(&mut self, msg: JsonRpcSubscribeMsg, _ctx: &mut Context<Self>) -> Self::Result {
        debug!(
            "Subscribing to method {} with params {}",
            msg.method, msg.params
        );
        let subscribe = JsonRpcMsg::new(
            "witnet_subscribe".to_string(),
            Value::Array(vec![Value::String(msg.method.clone()), msg.params.clone()]),
        );
        let fut = self
            .s
            .execute(&subscribe.method, subscribe.params)
            .into_actor(self)
            .then(|x, act, ctx| match x {
                Ok(res) => {
                    info!("Subscribed successfully! Id: {:?}", res);
                    let res = match res {
                        Value::String(s) => s,
                        _ => panic!("Only String subscription ids are supported"),
                    };
                    let resc = res.clone();
                    let fut = act
                        .s
                        .subscribe(&res.clone().into())
                        .map(move |x| NormalNotification(resc.clone(), x));
                    JsonRpcClient::add_stream(fut, ctx);
                    actix::fut::ok(res)
                }
                Err(e) => {
                    warn!("Error: {}", e);
                    actix::fut::err(e.to_string())
                }
            });
        Box::new(fut)
    }
}

struct JsonRpcForwardSubscribeMsg {
    method: String,
    params: Value,
    id: String,
    sink: jsonrpc_pubsub::Sink,
}

impl JsonRpcForwardSubscribeMsg {
    fn new<A: Into<String>, B: Into<Value>, C: Into<String>>(
        method: A,
        params: B,
        id: C,
        sink: jsonrpc_pubsub::Sink,
    ) -> Self {
        Self {
            method: method.into(),
            params: params.into(),
            id: id.into(),
            sink,
        }
    }
}

impl Message for JsonRpcForwardSubscribeMsg {
    type Result = Result<String, String>;
}

impl Handler<JsonRpcForwardSubscribeMsg> for JsonRpcClient {
    type Result = ResponseActFuture<Self, String, String>;

    fn handle(
        &mut self,
        msg: JsonRpcForwardSubscribeMsg,
        _ctx: &mut Context<Self>,
    ) -> Self::Result {
        debug!(
            "Subscribing to method {} with params {}",
            msg.method, msg.params
        );
        let subscribe = JsonRpcMsg::new(
            "witnet_subscribe".to_string(),
            Value::Array(vec![Value::String(msg.method.clone()), msg.params.clone()]),
        );
        let fut = self
            .s
            .execute(&subscribe.method, subscribe.params)
            .into_actor(self)
            .then(|x, act, ctx| match x {
                Ok(res) => {
                    info!("Subscribed successfully! Id: {:?}", res);
                    let res = match res {
                        Value::String(s) => s,
                        _ => panic!("Only String subscription ids are supported"),
                    };
                    let resc = res.clone();
                    let empty_map = HashMap::new();
                    let fut = act
                        .s
                        .subscribe(&res.clone().into())
                        .map(move |x| ForwardNotification(resc.clone(), x));
                    JsonRpcClient::add_stream(fut, ctx);

                    if let Ok(mut s) = act.subscriptions.lock() {
                        let v = s.entry("newBlocks").or_insert(empty_map);
                        let node_sub_id = res.clone();
                        v.insert(
                            msg.id.clone().into(),
                            (msg.sink, Some(node_sub_id.clone().into()), msg.params),
                        );
                        info!("Mapping node id {} to client id {}", node_sub_id, msg.id);
                        act.node_id_to_client_id
                            .insert(node_sub_id.into(), msg.id.into());
                        info!("Subscribed to newBlocks");
                        info!("This session has {} subscriptions", v.len());
                    }

                    actix::fut::ok(res)
                }
                Err(e) => {
                    warn!("Error: {}", e);
                    actix::fut::err(e.to_string())
                }
            });
        Box::new(fut)
    }
}

struct JsonRpcUnsubscribe {
    id: SubscriptionId,
}

impl Message for JsonRpcUnsubscribe {
    type Result = Result<Value, String>;
}

impl Handler<JsonRpcUnsubscribe> for JsonRpcClient {
    type Result = ResponseActFuture<Self, Value, String>;

    fn handle(&mut self, msg: JsonRpcUnsubscribe, _ctx: &mut Self::Context) -> Self::Result {
        let id_value = match msg.id.clone() {
            jsonrpc_pubsub::SubscriptionId::Number(x) => Value::Number(serde_json::Number::from(x)),
            jsonrpc_pubsub::SubscriptionId::String(s) => Value::String(s),
        };
        /*
        let id_string = match msg.id.clone() {
            jsonrpc_pubsub::SubscriptionId::Number(x) => panic!("Integer ids are not supported by async_jsonrpc_client"),
            jsonrpc_pubsub::SubscriptionId::String(s) => s,
        };
        */
        let unsubscribe = JsonRpcMsg::new(
            "witnet_unsubscribe".to_string(),
            Value::Array(vec![id_value]),
        );

        info!("Handler<JsonRpcUnsubscribe> {:?}", msg.id);

        // TODO: somehow removing this fixes the closed socket bug?
        // Stop listening to notifications with this id
        //self.s.unsubscribe(&id_string.clone().into());

        // Call unsubscribe method
        let fut = self
            .s
            .execute(&unsubscribe.method, unsubscribe.params)
            .into_actor(self)
            .then(|res, _act, _ctx| match res {
                Ok(res) => {
                    info!("Unsubscribed from node successfully! Res: {:?}", res);
                    actix::fut::ok(res)
                }
                Err(e) => {
                    warn!("Error: {}", e);
                    actix::fut::err(e.to_string())
                }
            });
        Box::new(fut)
    }
}

#[derive(Debug)]
struct ForwardUnsubscribe {
    client_id: SubscriptionId,
}

impl Message for ForwardUnsubscribe {
    type Result = Result<jsonrpc_core::Value, jsonrpc_core::Error>;
}

impl Handler<ForwardUnsubscribe> for JsonRpcClient {
    type Result = Result<jsonrpc_core::Value, jsonrpc_core::Error>;

    fn handle(&mut self, msg: ForwardUnsubscribe, ctx: &mut Self::Context) -> Self::Result {
        info!("Called ForwardUnsubscribe: {:?}", msg.client_id);
        if let Ok(mut s) = self.subscriptions.lock() {
            for (_method, v) in s.iter_mut() {
                if let Some((_sink, Some(node_id), _sub_params)) = v.remove(&msg.client_id) {
                    info!("Removing node {:?} => client {:?}", node_id, msg.client_id);
                    self.node_id_to_client_id.remove(&node_id);
                    // We also need to unsubscribe from the node
                    ctx.address().do_send(JsonRpcUnsubscribe { id: node_id });
                }
            }
        } else {
            log::error!("Error unsubscribe: failed to acquire lock");
            let mut e = jsonrpc_core::Error::internal_error();
            e.message = "Error unsubscribe: failed to acquire lock".to_string();
            return Err(e);
        }

        Ok(jsonrpc_core::Value::Bool(true))
    }
}

#[derive(Debug)]
struct NormalNotification(String, jsonrpc_core::Value);

impl StreamHandler<NormalNotification, async_jsonrpc_client::Error> for JsonRpcClient {
    fn handle(
        &mut self,
        NormalNotification(id, item): NormalNotification,
        _ctx: &mut Self::Context,
    ) {
        info!("Got subscription for id {}: {}", id, item);
    }
}

#[derive(Debug)]
struct ForwardNotification(String, jsonrpc_core::Value);

impl StreamHandler<ForwardNotification, async_jsonrpc_client::Error> for JsonRpcClient {
    fn handle(
        &mut self,
        ForwardNotification(id, item): ForwardNotification,
        ctx: &mut Self::Context,
    ) {
        info!("Got subscription with id! {} {}", id, item);
        let idd: SubscriptionId = id.clone().into();
        // Now we need to send a notification to the client
        // TODO: SubRes could be imported as SubscriptionResult from witnet_node/json_rpc
        // but importing witnet_node results in a dependency cycle
        struct SubRes {
            result: Value,
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
                            Value::Number(serde_json::Number::from(x))
                        }
                        jsonrpc_pubsub::SubscriptionId::String(s) => Value::String(s),
                    },
                );
                jsonrpc_core::Params::Map(map)
            }
        }

        if let Some(client_id) = self.node_id_to_client_id.get(&idd) {
            let r = SubRes {
                result: item.clone(),
                subscription: client_id.clone(),
            };
            let params = jsonrpc_core::Params::from(r);
            if let Ok(ss) = self.subscriptions.lock() {
                for (_method, v) in ss.iter() {
                    if let Some((sink, Some(node_id), _sub_params)) = v.get(client_id) {
                        info!(
                            "Forwarding subscription to parent: {:?} => {:?}",
                            node_id, client_id
                        );
                        sink.notify(params.clone())
                            .into_actor(self)
                            .then(|_act, _res, _ctx| actix::fut::ok(()))
                            .wait(ctx);
                    }
                }
            }
        }
    }
}
