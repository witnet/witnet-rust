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
use futures::{future, Future};
use jsonrpc_core as rpc;
use serde_json as json;

use witnet_config::config::Config;
use witnet_net::server::ws;

mod actors;
mod client;
mod err_codes;
mod handlers;
mod response;
mod routes;
mod wallet;

/// Run the websockets server for the Witnet wallet.
pub fn run(conf: Config) -> std::io::Result<()> {
    let workers = conf.wallet.workers;
    let addr = conf.wallet.server_addr;
    let db_path = conf.wallet.db_path;

    ws::Server::new(move || {
        let thread_db_path = db_path.clone();
        let storage = SyncArbiter::start(1, move || actors::Storage::new(thread_db_path.clone()));
        let app = actors::App.start();
        let mut io = rpc::IoHandler::default();

        forwarded_routes!(io, "getBlock", "getBlockChain", "getOutput", "inventory",);

        routes!(
            io,
            ("getWalletInfos", handlers::get_wallet_infos),
            // ("createMnemonics", handlers::create_mnemonics),
            ("importSeed", handlers::import_seed),
            // ("createWallet", handlers::create_wallet),
            // ("lockWallet", handlers::lock_wallet),
            // ("unlockWallet", handlers::unlock_wallet),
            // ("getTransactions", handlers::get_transactions),
            // ("sendVTT", handlers::send_vtt),
            // ("generateAddress", handlers::generate_address),
            // ("createDataRequest", handlers::create_data_request),
            // ("runDataRequest", handlers::run_data_request),
            // ("sendDataRequest", handlers::send_data_request),
        );

        io
    })
    .workers(workers)
    .addr(addr)
    .run()
}
