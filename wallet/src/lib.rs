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
use std::sync::Arc;
use std::time::Duration;

use actix::prelude::*;
use failure::Error;
use jsonrpc_core as rpc;
use jsonrpc_pubsub as pubsub;

use witnet_config::config::Config;
use witnet_data_structures::chain::EpochConstants;
use witnet_net::{client::tcp::JsonRpcClient, server::ws::Server};

mod account;
mod actors;
mod constants;
mod crypto;
mod db;
mod model;
mod params;
mod repository;
mod signal;
mod types;

/// Run the Witnet wallet application.
pub fn run(conf: Config) -> Result<(), Error> {
    let session_expires_in = Duration::from_secs(conf.wallet.session_expires_in);
    let requests_timeout = Duration::from_millis(conf.wallet.requests_timeout);
    let server_addr = conf.wallet.server_addr;
    let db_path = conf.wallet.db_path;
    let db_file_name = conf.wallet.db_file_name;
    let node_url = conf.wallet.node_url;
    let rocksdb_opts = conf.rocksdb.to_rocksdb_options();
    let epoch_constants = EpochConstants {
        checkpoint_zero_timestamp: conf.consensus_constants.checkpoint_zero_timestamp,
        checkpoints_period: conf.consensus_constants.checkpoints_period,
    };

    // Db-encryption params
    let db_hash_iterations = conf.wallet.db_encrypt_hash_iterations;
    let db_iv_length = conf.wallet.db_encrypt_iv_length;
    let db_salt_length = conf.wallet.db_encrypt_salt_length;

    // Whether wallet is in testnet mode or not
    let testnet = conf.wallet.testnet;

    // Master-key generation params
    let seed_password = conf.wallet.seed_password;
    let master_key_salt = conf.wallet.master_key_salt;
    let id_hash_iterations = conf.wallet.id_hash_iterations;
    let id_hash_function = conf.wallet.id_hash_function;

    // Wallet concurrency
    let concurrency = conf.wallet.concurrency.unwrap_or_else(num_cpus::get);

    let system = System::new("witnet-wallet");

    let node_jsonrpc_server_address = conf.jsonrpc.server_address;
    let client = node_url.map_or_else(
        || {
            log::warn!("No node url in config! To connect to a Witnet node, you must manually add the address to the configuration file as follows:\n\
                        [wallet]\n\
                        node_url = \"{}\"\n", node_jsonrpc_server_address);
            Ok(None)
        },
        |url| {
            if url != node_jsonrpc_server_address.to_string() {
                log::warn!("The local Witnet node JSON-RPC server is configured to listen at {} but the wallet will connect to {}", node_jsonrpc_server_address, url);
            }
            JsonRpcClient::start(url.as_ref()).map(Some)
        },
    )?;

    let db = Arc::new(
        ::rocksdb::DB::open(&rocksdb_opts, db_path.join(db_file_name))
            .map_err(|e| failure::format_err!("{}", e))?,
    );
    let params = params::Params {
        testnet,
        seed_password,
        master_key_salt,
        id_hash_iterations,
        id_hash_function,
        db_hash_iterations,
        db_iv_length,
        db_salt_length,
        epoch_constants,
    };

    let worker = actors::Worker::start(concurrency, db.clone(), params);

    let app = actors::App::start(actors::app::Params {
        testnet,
        worker,
        client,
        session_expires_in,
        requests_timeout,
    });
    let mut handler = pubsub::PubSubHandler::new(rpc::MetaIoHandler::default());

    actors::app::connect_routes(&mut handler, app.clone(), Arbiter::current());

    let server = Server::build().handler(handler).addr(server_addr).start()?;
    let controller = actors::Controller::start(server, app);

    signal::ctrl_c(move || {
        controller.do_send(actors::controller::Shutdown);
    });

    system.run()?;

    log::info!("Waiting for db to shut down...");
    while Arc::strong_count(&db) > 1 {
        std::thread::sleep(Duration::from_millis(500));
    }
    log::info!("Db shut down finished.");

    Ok(())
}
