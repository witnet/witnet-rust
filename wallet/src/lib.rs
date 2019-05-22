//! Wallet implementation for Witnet
//!
//! The way a client interacts with the Wallet is through a Websockets server. After running it you
//! can interact with it from the web-browser's javascript console:
//! ```js
//! var sock= (() => { let s = new WebSocket('ws://localhost:3030');s.addEventListener('message', (e) => {  console.log('Rcv =>', e.data) });return s; })();
//! sock.send('{"jsonrpc":"2.0","method":"getBlockChain","id":"1"}');
//! ```

#![deny(rust_2018_idioms)]
#![deny(non_upper_case_globals)]
#![deny(non_camel_case_types)]
#![deny(non_snake_case)]
#![deny(unused_mut)]
#![deny(missing_docs)]
use actix::prelude::*;
use futures::{future, Future as _};
use jsonrpc_core as rpc;
use serde_json as json;

use witnet_config::config::Config;
use witnet_net::server::ws;

mod actors;
mod client;
mod err_codes;
mod error;
mod response;
mod wallet;

/// Helper macro to build jsonrpc internal-error types
macro_rules! internal_error {
    () => {
        rpc::Error {
            code: rpc::ErrorCode::ServerError(err_codes::INTERNAL_ERROR),
            message: "Internal error.".into(),
            data: None,
        }
    };
}

/// Helper macro to add multiple JSON-RPC methods at once
macro_rules! routes {
    // No args: do nothing
    ($io:expr, $app:expr $(,)?) => {};
    ($io:expr, $app:expr, ($method_jsonrpc:expr, $actor_msg:ty $(,)?), $($args:tt)*) => {
        // Base case:
        {
            let app_addr = $app.clone();
            $io.add_method($method_jsonrpc, move |params: rpc::Params| {
                log::debug!("Handling request for method: {}", $method_jsonrpc);
                let addr = app_addr.clone();
                // Try to parse the request params into the actor message
                future::result(params.parse::<$actor_msg>())
                    .and_then(move |msg| {
                        // Then send the parsed message to the actor
                        addr.send(msg)
                            .map_err(error::Error::Mailbox)
                            .flatten()
                            .and_then(
                                |x|
                                future::result(json::to_value(x)).map_err(error::Error::Serialization)
                            )
                            .map_err(|err| match err {
                                error::Error::Mailbox(MailboxError::Closed) => {
                                    log::error!("Mailbox closed");
                                    internal_error!()
                                }
                                error::Error::Mailbox(MailboxError::Timeout) => {
                                    log::error!("Mailbox timed out");
                                    rpc::Error {
                                        code: rpc::ErrorCode::ServerError(err_codes::TIMEOUT_ERROR),
                                        message: "Timeout error.".into(),
                                        data: None,
                                    }
                                }
                                error::Error::Rad(err) => {
                                    rpc::Error {
                                        code: rpc::ErrorCode::ServerError(err_codes::RAD_ERROR),
                                        message: format!("{}", err),
                                        data: None,
                                    }
                                }
                                error::Error::Storage(err) => {
                                    log::error!("Database: {}", err);
                                    internal_error!()
                                }
                                error::Error::Serialization(err) => {
                                    log::error!("Serialization: {}", err);
                                    internal_error!()
                                }
                            })
                    })
            });
        }
        // Recursion!
        routes!($io, $app, $($args)*);
    };
}

/// Macro to add multiple JSON-RPC methods that forward the request to the Node at once
macro_rules! forwarded_routes {
    ($io:expr $(,)*) => {};
    ($io:expr, $method_jsonrpc:expr, $($args:tt)*) => {
        // Base case:
        {
            $io.add_method($method_jsonrpc, move |params: rpc::Params| {
                client::send(client::request($method_jsonrpc).params(params))
            });
        }
        // Recursion!
        forwarded_routes!($io, $($args)*);
    };
}

/// Run the websockets server for the Witnet wallet.
pub fn run(conf: Config) -> Result<i32, failure::Error> {
    let workers = conf.wallet.workers;
    let addr = conf.wallet.server_addr;
    let db_path = conf.wallet.db_path;

    let system = System::new("witnet-wallet");
    let app = actors::App::build().start(db_path);

    let _server = ws::Server::new(move || {
        let mut io = rpc::IoHandler::default();

        forwarded_routes!(io, "getBlock", "getBlockChain", "getOutput", "inventory",);

        routes!(
            io,
            app,
            ("getWalletInfos", actors::app::GetWalletInfos),
            ("createMnemonics", actors::app::CreateMnemonics),
            ("importSeed", actors::app::ImportSeed),
            ("createWallet", actors::app::CreateWallet),
            ("lockWallet", actors::app::LockWallet),
            ("unlockWallet", actors::app::UnlockWallet),
            ("getTransactions", actors::app::GetTransactions),
            ("sendVTT", actors::app::SendVtt),
            ("generateAddress", actors::app::GenerateAddress),
            ("createDataRequest", actors::app::CreateDataRequest),
            ("runRadRequest", actors::app::RunRadRequest),
            ("sendDataRequest", actors::app::SendDataRequest),
        );

        io
    })
    .workers(workers)
    .addr(addr)
    .start()?;

    Ok(system.run())
}
