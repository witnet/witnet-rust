//! Websockets JSON-RPC server

use actix::{
    Actor, ActorFuture, Context, Handler, Message, ResponseActFuture, Supervised, System,
    SystemRegistry, SystemService, WrapFuture,
};
use async_jsonrpc_client::transports::shared::EventLoopHandle;
use async_jsonrpc_client::transports::tcp::TcpSocket;
use async_jsonrpc_client::Transport;
use futures::future::Future;
use jsonrpc_ws_server::jsonrpc_core::{IoHandler, Params, Value};
use jsonrpc_ws_server::{Server, ServerBuilder};
use log::{debug, info, warn};
use serde::{Deserialize, Serialize};
use std::net::SocketAddr;

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
    let mut io = IoHandler::new();

    add_methods!(
        io,
        registry,
        ("say_hello", say_hello),
        ("getBlockChain", |r, p| forward_call("getBlockChain", r, p)),
        ("inventory", |r, p| forward_call("inventory", r, p)),
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

    // Start the WebSockets JSON-RPC server in a new thread and bind to address "addr"
    ServerBuilder::new(io).start(addr)
}

#[derive(Debug, Deserialize)]
struct LockWalletParams {
    wallet_id: String,
    #[serde(default)] // default to false
    wipe: bool,
}

fn lock_wallet(
    _registry: &SystemRegistry,
    params: jsonrpc_ws_server::jsonrpc_core::Result<LockWalletParams>,
) -> impl Future<Item = Value, Error = jsonrpc_ws_server::jsonrpc_core::Error> {
    let _params = match params {
        Ok(x) => x,
        Err(e) => return Box::new(futures::failed(e)),
    };

    let x = true;
    Box::new(futures::done(serde_json::to_value(x).map_err(|e| {
        let mut err = jsonrpc_ws_server::jsonrpc_core::Error::internal_error();
        err.message = e.to_string();
        err
    })))
}

fn send_data_request(
    _registry: &SystemRegistry,
    params: jsonrpc_ws_server::jsonrpc_core::Result<DataRequest>,
) -> impl Future<Item = Value, Error = jsonrpc_ws_server::jsonrpc_core::Error> {
    let _params = match params {
        Ok(x) => x,
        Err(e) => return Box::new(futures::failed(e)),
    };

    let x = Transaction {};
    Box::new(futures::done(serde_json::to_value(x).map_err(|e| {
        let mut err = jsonrpc_ws_server::jsonrpc_core::Error::internal_error();
        err.message = e.to_string();
        err
    })))
}

// TODO: radon crate
#[derive(Serialize)]
struct RadonValue {}

fn run_data_request(
    _registry: &SystemRegistry,
    params: jsonrpc_ws_server::jsonrpc_core::Result<DataRequest>,
) -> impl Future<Item = Value, Error = jsonrpc_ws_server::jsonrpc_core::Error> {
    let _params = match params {
        Ok(x) => x,
        Err(e) => return Box::new(futures::failed(e)),
    };

    let x = RadonValue {};
    Box::new(futures::done(serde_json::to_value(x).map_err(|e| {
        let mut err = jsonrpc_ws_server::jsonrpc_core::Error::internal_error();
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
    params: jsonrpc_ws_server::jsonrpc_core::Result<CreateDataRequestParams>,
) -> impl Future<Item = Value, Error = jsonrpc_ws_server::jsonrpc_core::Error> {
    let _params = match params {
        Ok(x) => x,
        Err(e) => return Box::new(futures::failed(e)),
    };

    let x = DataRequest {};
    Box::new(futures::done(serde_json::to_value(x).map_err(|e| {
        let mut err = jsonrpc_ws_server::jsonrpc_core::Error::internal_error();
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
        let mut err = jsonrpc_ws_server::jsonrpc_core::Error::internal_error();
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
    params: jsonrpc_ws_server::jsonrpc_core::Result<SendVttParams>,
) -> impl Future<Item = Value, Error = jsonrpc_ws_server::jsonrpc_core::Error> {
    let _params = match params {
        Ok(x) => x,
        Err(e) => return Box::new(futures::failed(e)),
    };

    let x = Transaction {};
    Box::new(futures::done(serde_json::to_value(x).map_err(|e| {
        let mut err = jsonrpc_ws_server::jsonrpc_core::Error::internal_error();
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
    params: jsonrpc_ws_server::jsonrpc_core::Result<GetTransactionsParams>,
) -> impl Future<Item = Value, Error = jsonrpc_ws_server::jsonrpc_core::Error> {
    let _params = match params {
        Ok(x) => x,
        Err(e) => return Box::new(futures::failed(e)),
    };

    let x: Vec<Transaction> = vec![];
    Box::new(futures::done(serde_json::to_value(x).map_err(|e| {
        let mut err = jsonrpc_ws_server::jsonrpc_core::Error::internal_error();
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
    params: jsonrpc_ws_server::jsonrpc_core::Result<UnlockWalletParams>,
) -> impl Future<Item = Value, Error = jsonrpc_ws_server::jsonrpc_core::Error> {
    let _params = match params {
        Ok(x) => x,
        Err(e) => return Box::new(futures::failed(e)),
    };

    let x = Wallet::for_test();
    Box::new(futures::done(serde_json::to_value(x).map_err(|e| {
        let mut err = jsonrpc_ws_server::jsonrpc_core::Error::internal_error();
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
    params: jsonrpc_ws_server::jsonrpc_core::Result<CreateWalletParams>,
) -> impl Future<Item = Value, Error = jsonrpc_ws_server::jsonrpc_core::Error> {
    let _params = match params {
        Ok(x) => x,
        Err(e) => return Box::new(futures::failed(e)),
    };

    let x = Wallet::for_test();
    Box::new(futures::done(serde_json::to_value(x).map_err(|e| {
        let mut err = jsonrpc_ws_server::jsonrpc_core::Error::internal_error();
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
    params: jsonrpc_ws_server::jsonrpc_core::Result<ImportSeedParams>,
) -> impl Future<Item = Value, Error = jsonrpc_ws_server::jsonrpc_core::Error> {
    let _params = match params {
        Ok(x) => x,
        Err(e) => return Box::new(futures::failed(e)),
    };

    let x = true;
    Box::new(futures::done(serde_json::to_value(x).map_err(|e| {
        let mut err = jsonrpc_ws_server::jsonrpc_core::Error::internal_error();
        err.message = e.to_string();
        err
    })))
}

// TODO: implemented in PR #432
#[derive(Debug, Deserialize, Serialize)]
struct Mnemonics {}

fn create_mnemonics(
    _registry: &SystemRegistry,
    params: jsonrpc_ws_server::jsonrpc_core::Result<()>,
) -> impl Future<Item = Value, Error = jsonrpc_ws_server::jsonrpc_core::Error> {
    match params {
        Ok(x) => x,
        Err(e) => return Box::new(futures::failed(e)),
    };

    let x = Mnemonics {};
    Box::new(futures::done(serde_json::to_value(x).map_err(|e| {
        let mut err = jsonrpc_ws_server::jsonrpc_core::Error::internal_error();
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
    params: jsonrpc_ws_server::jsonrpc_core::Result<()>,
) -> impl Future<Item = Value, Error = jsonrpc_ws_server::jsonrpc_core::Error> {
    match params {
        Ok(x) => x,
        Err(e) => return Box::new(futures::failed(e)),
    };

    let x: Vec<WalletInfo> = vec![];
    Box::new(futures::done(serde_json::to_value(x).map_err(|e| {
        let mut err = jsonrpc_ws_server::jsonrpc_core::Error::internal_error();
        err.message = e.to_string();
        err
    })))
}

/// Forwards a JSON-RPC call to the node
fn forward_call(
    method: &str,
    registry: &SystemRegistry,
    params: jsonrpc_ws_server::jsonrpc_core::Result<Value>,
) -> impl Future<Item = Value, Error = jsonrpc_ws_server::jsonrpc_core::Error> {
    registry
        .get::<JsonRpcClient>()
        .send(JsonRpcMsg::new(method, params.unwrap_or(Value::Null)))
        .then(|x| match x {
            Err(e) => {
                let mut err = jsonrpc_ws_server::jsonrpc_core::Error::internal_error();
                err.message = e.to_string();
                Err(err)
            }
            Ok(s) => match s {
                Ok(s) => Ok(Value::from(s)),
                Err(e) => {
                    let mut err = jsonrpc_ws_server::jsonrpc_core::Error::internal_error();
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
    params: jsonrpc_ws_server::jsonrpc_core::Result<SayHelloParams>,
) -> impl Future<Item = Value, Error = jsonrpc_ws_server::jsonrpc_core::Error> {
    registry
        .get::<HiActor>()
        .send(Greeting {
            name: params.map(|x| x.name).unwrap_or_else(|_| "Anon".into()),
        })
        .then(|x| match x {
            Err(e) => {
                let mut err = jsonrpc_ws_server::jsonrpc_core::Error::internal_error();
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
}

impl JsonRpcClient {
    fn new(url: &str) -> Self {
        let (handle, s) = TcpSocket::new(url).unwrap();
        Self { handle, s }
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
                    Ok(x) => serde_json::to_string(&x).map_err(|e| e.to_string()),
                    Err(e) => {
                        warn!("Error: {}", e);
                        Err(e.to_string())
                    }
                })
            });

        Box::new(fut)
    }
}
