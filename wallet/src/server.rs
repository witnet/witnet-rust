//! Websockets JSON-RPC server

use actix::{Actor, ActorFuture, Arbiter, Context, Handler, Message, ResponseActFuture, Supervised, System, SystemRegistry, SystemService, WrapFuture};
use futures::future::Future;
use jsonrpc_ws_server::jsonrpc_core::{IoHandler, Params, Value};
use jsonrpc_ws_server::{Server, ServerBuilder};
use serde::{Serialize, Deserialize};
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
    let reg = registry.clone();
    io.add_method("getWalletInfos", move |params: Params| {
        get_wallet_infos(&reg, params.parse())
    });
    let reg = registry.clone();
    io.add_method("createMnemonics", move |params: Params| {
        create_mnemonics(&reg, params.parse())
    });
    let reg = registry.clone();
    io.add_method("importSeed", move |params: Params| {
        import_seed(&reg, params.parse())
    });
    let reg = registry.clone();
    io.add_method("createWallet", move |params: Params| {
        create_wallet(&reg, params.parse())
    });
    let reg = registry.clone();
    io.add_method("unlockWallet", move |params: Params| {
        unlock_wallet(&reg, params.parse())
    });
    let reg = registry.clone();
    io.add_method("getTransactions", move |params: Params| {
        get_transactions(&reg, params.parse())
    });
    let reg = registry.clone();
    io.add_method("sendVTT", move |params: Params| {
        send_vtt(&reg, params.parse())
    });
    let reg = registry.clone();
    io.add_method("generateAddress", move |params: Params| {
        generate_address(&reg, params.parse())
    });
    let reg = registry.clone();
    io.add_method("createDataRequest", move |params: Params| {
        create_data_request(&reg, params.parse())
    });
    let reg = registry.clone();
    io.add_method("runDataRequest", move |params: Params| {
        run_data_request(&reg, params.parse())
    });
    let reg = registry.clone();
    io.add_method("sendDataRequest", move |params: Params| {
        send_data_request(&reg, params.parse())
    });
    let reg = registry.clone();
    io.add_method("lockWallet", move |params: Params| {
        lock_wallet(&reg, params.parse())
    });

    // Start the WebSockets JSON-RPC server in a new thread and bind to address "addr"
    ServerBuilder::new(io).start(addr)
}

#[derive(Deserialize)]
struct LockWalletParams {
    wallet_id: String,
    #[serde(default)] // default to false
    wipe: bool,
}

fn lock_wallet(
    registry: &SystemRegistry,
    params: jsonrpc_ws_server::jsonrpc_core::Result<LockWalletParams>,
) -> impl Future<Item = Value, Error = jsonrpc_ws_server::jsonrpc_core::Error> {
    let params = match params {
        Ok(x) => x,
        Err(e) => return Box::new(futures::failed(e))
    };

    let x = true;
    Box::new(futures::done(serde_json::to_value(x).map_err(|e| {
        let mut err = jsonrpc_ws_server::jsonrpc_core::Error::internal_error();
        err.message = e.to_string();
        err
    })))
}

fn send_data_request(
    registry: &SystemRegistry,
    params: jsonrpc_ws_server::jsonrpc_core::Result<DataRequest>,
) -> impl Future<Item = Value, Error = jsonrpc_ws_server::jsonrpc_core::Error> {
    let params = match params {
        Ok(x) => x,
        Err(e) => return Box::new(futures::failed(e))
    };

    let x = RadonValue{};
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
    registry: &SystemRegistry,
    params: jsonrpc_ws_server::jsonrpc_core::Result<DataRequest>,
) -> impl Future<Item = Value, Error = jsonrpc_ws_server::jsonrpc_core::Error> {
    let params = match params {
        Ok(x) => x,
        Err(e) => return Box::new(futures::failed(e))
    };

    let x = RadonValue{};
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
    registry: &SystemRegistry,
    params: jsonrpc_ws_server::jsonrpc_core::Result<CreateDataRequestParams>,
) -> impl Future<Item = Value, Error = jsonrpc_ws_server::jsonrpc_core::Error> {
    let params = match params {
        Ok(x) => x,
        Err(e) => return Box::new(futures::failed(e))
    };

    let x = DataRequest{};
    Box::new(futures::done(serde_json::to_value(x).map_err(|e| {
        let mut err = jsonrpc_ws_server::jsonrpc_core::Error::internal_error();
        err.message = e.to_string();
        err
    })))
}

// TODO: is this pkh?
#[derive(Serialize)]
struct Address {}

fn generate_address(
    registry: &SystemRegistry,
    params: jsonrpc_ws_server::jsonrpc_core::Result<()>,
) -> impl Future<Item = Value, Error = jsonrpc_ws_server::jsonrpc_core::Error> {
    let params = match params {
        Ok(x) => x,
        Err(e) => return Box::new(futures::failed(e))
    };

    let x = Address{};
    Box::new(futures::done(serde_json::to_value(x).map_err(|e| {
        let mut err = jsonrpc_ws_server::jsonrpc_core::Error::internal_error();
        err.message = e.to_string();
        err
    })))
}

#[derive(Deserialize)]
struct SendVttParams {
    wallet_id: String,
    to_address: Vec<u8>,
    amount: u64,
    fee: u64,
    subject: String,
}

fn send_vtt(
    registry: &SystemRegistry,
    params: jsonrpc_ws_server::jsonrpc_core::Result<SendVttParams>,
) -> impl Future<Item = Value, Error = jsonrpc_ws_server::jsonrpc_core::Error> {
    let params = match params {
        Ok(x) => x,
        Err(e) => return Box::new(futures::failed(e))
    };

    let x = Transaction{};
    Box::new(futures::done(serde_json::to_value(x).map_err(|e| {
        let mut err = jsonrpc_ws_server::jsonrpc_core::Error::internal_error();
        err.message = e.to_string();
        err
    })))
}

#[derive(Deserialize)]
struct GetTransactionsParams {
    wallet_id: String,
    limit: u32,
    page: u32,
}

// TODO: import data_structures crate
#[derive(Serialize)]
struct Transaction {}

fn get_transactions(
    registry: &SystemRegistry,
    params: jsonrpc_ws_server::jsonrpc_core::Result<GetTransactionsParams>,
) -> impl Future<Item = Value, Error = jsonrpc_ws_server::jsonrpc_core::Error> {
    let params = match params {
        Ok(x) => x,
        Err(e) => return Box::new(futures::failed(e))
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
    registry: &SystemRegistry,
    params: jsonrpc_ws_server::jsonrpc_core::Result<UnlockWalletParams>,
) -> impl Future<Item = Value, Error = jsonrpc_ws_server::jsonrpc_core::Error> {
    let params = match params {
        Ok(x) => x,
        Err(e) => return Box::new(futures::failed(e))
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
            info: WalletInfo { id: "".to_string(), caption: "".to_string() },
            seed: SeedInfo::Wip3(Seed(vec![])),
            epochs: EpochsInfo { last: 0, born: 0 },
            purpose: DerivationPath(format!("m/44'/60'/0'/0")),
            accounts: vec![]
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
    ExternalKeyChain,
    InternalKeyChain,
    RadKeyChain,
}

#[derive(Deserialize)]
struct CreateWalletParams {
    name: String,
    password: String,
}

fn create_wallet(
    registry: &SystemRegistry,
    params: jsonrpc_ws_server::jsonrpc_core::Result<CreateWalletParams>,
) -> impl Future<Item = Value, Error = jsonrpc_ws_server::jsonrpc_core::Error> {
    let params = match params {
        Ok(x) => x,
        Err(e) => return Box::new(futures::failed(e))
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
    registry: &SystemRegistry,
    params: jsonrpc_ws_server::jsonrpc_core::Result<ImportSeedParams>,
) -> impl Future<Item = Value, Error = jsonrpc_ws_server::jsonrpc_core::Error> {
    let params = match params {
        Ok(x) => x,
        Err(e) => return Box::new(futures::failed(e))
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
    registry: &SystemRegistry,
    params: jsonrpc_ws_server::jsonrpc_core::Result<()>,
) -> impl Future<Item = Value, Error = jsonrpc_ws_server::jsonrpc_core::Error> {
    let params = match params {
        Ok(x) => x,
        Err(e) => return Box::new(futures::failed(e))
    };

    let x = Mnemonics{};
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
    registry: &SystemRegistry,
    params: jsonrpc_ws_server::jsonrpc_core::Result<()>,
) -> impl Future<Item = Value, Error = jsonrpc_ws_server::jsonrpc_core::Error> {
    let params = match params {
        Ok(x) => x,
        Err(e) => return Box::new(futures::failed(e))
    };

    let x: Vec<WalletInfo> = vec![];
    Box::new(futures::done(serde_json::to_value(x).map_err(|e| {
        let mut err = jsonrpc_ws_server::jsonrpc_core::Error::internal_error();
        err.message = e.to_string();
        err
    })))
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