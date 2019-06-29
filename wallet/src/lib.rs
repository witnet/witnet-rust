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
use std::time::Duration;

use actix::prelude::*;
use failure::Error;
use jsonrpc_core as rpc;
use jsonrpc_pubsub as pubsub;

use witnet_config::config::Config;
use witnet_net::{client::tcp::JsonRpcClient, server::ws::Server};

mod actors;
mod api;
mod app;
mod crypto;
mod signal;
mod storage;
mod validation;
mod wallet;

/// Run the Witnet wallet application.
pub fn run(conf: Config) -> Result<(), Error> {
    let session_expires_in = Duration::from_secs(conf.wallet.session_expires_in);
    let server_addr = conf.wallet.server_addr;
    let db_path = conf.wallet.db_path;
    let db_file_name = conf.wallet.db_file_name;
    let node_url = conf.wallet.node_url;
    let mut rocksdb_opts = conf.rocksdb.to_rocksdb_options();
    // https://github.com/facebook/rocksdb/wiki/Merge-Operator
    rocksdb_opts.set_merge_operator(
        "wallet merge operator",
        storage::storage_merge_operator,
        None,
    );

    // Db-encryption params used by the Storage actor
    let db_encrypt_hash_iterations = conf.wallet.db_encrypt_hash_iterations;
    let db_encrypt_iv_length = conf.wallet.db_encrypt_iv_length;
    let db_encrypt_salt_length = conf.wallet.db_encrypt_salt_length;

    // Master-key generation params used by the Crypto actor
    let seed_password = conf.wallet.seed_password;
    let master_key_salt = conf.wallet.master_key_salt;
    let id_hash_iterations = conf.wallet.id_hash_iterations;
    let id_hash_function = conf.wallet.id_hash_function;

    let node_client = node_url.clone().map_or_else(
        || Ok(None),
        |url| JsonRpcClient::start(url.as_ref()).map(Some),
    )?;
    let db = rocksdb::DB::open(&rocksdb_opts, db_path.join(db_file_name))
        .map_err(|e| failure::format_err!("{}", e))?;

    let system = System::new("witnet-wallet");
    let storage = actors::Storage::start(
        db_encrypt_hash_iterations,
        db_encrypt_iv_length,
        db_encrypt_salt_length,
    );
    let crypto = actors::Crypto::start(
        seed_password,
        master_key_salt,
        id_hash_iterations,
        id_hash_function,
    );
    let rad_executor = actors::RadExecutor::start();
    let app = actors::App::start(
        db,
        storage,
        crypto,
        rad_executor,
        node_client,
        session_expires_in,
    );
    let mut handler = pubsub::PubSubHandler::new(rpc::MetaIoHandler::default());

    api::connect_routes(&mut handler, app.clone());

    let server = Server::build().handler(handler).addr(server_addr).start()?;
    let controller = actors::Controller::start(server, app);

    signal::ctrl_c(move || {
        controller.do_send(actors::controller::Shutdown);
    });

    system.run()?;

    Ok(())
}
