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

use std::sync::{Arc, Mutex, RwLock};
use std::time::Duration;

use actix::prelude::*;
use failure::Error;

use witnet_config::config::Config;
use witnet_data_structures::chain::{CheckpointBeacon, EpochConstants};
use witnet_net::client::tcp::JsonRpcClient;

use crate::actors::app;

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
    let node_urls = conf.wallet.node_url;
    let rocksdb_opts = conf.rocksdb.to_rocksdb_options();
    let epoch_constants = EpochConstants {
        checkpoint_zero_timestamp: conf.consensus_constants.checkpoint_zero_timestamp,
        checkpoints_period: conf.consensus_constants.checkpoints_period,
    };
    let genesis_hash = conf.consensus_constants.genesis_hash;
    let genesis_prev_hash = conf.consensus_constants.bootstrap_hash;

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

    // Weights
    let max_vt_weight = conf.consensus_constants.max_vt_weight;
    let max_dr_weight = conf.consensus_constants.max_dr_weight;

    // Wallet concurrency
    let concurrency = conf.wallet.concurrency.unwrap_or_else(num_cpus::get);

    // How many blocks to ask a Witnet node for when synchronizing
    let node_sync_batch_size = conf.wallet.node_sync_batch_size;

    // Size of the address synchronization batch
    let sync_address_batch_length = conf.wallet.sync_address_batch_length;

    // Allow to use outputs that have not been confirmed by a superblock in new transactions
    let use_unconfirmed_utxos = conf.wallet.use_unconfirmed_utxos;

    let system = System::new();

    let consensus_constants = conf.consensus_constants;

    let pending_transactions_timeout_seconds = conf.wallet.pending_transactions_timeout_seconds;

    let db = Arc::new(
        ::rocksdb::DB::open(&rocksdb_opts, db_path.join(db_file_name))
            .map_err(|e| failure::format_err!("{}", e))?,
    );

    // Initialize actors inside system context
    system.block_on(async {
        let node_subscriptions = Arc::new(Mutex::new(Default::default()));
        let node_client_actor =
            JsonRpcClient::start_with_subscriptions(node_urls.clone(), node_subscriptions.clone())
                .map_err(|_| app::Error::NodeNotConnected)?;
        let node_client = Arc::new(app::NodeClient {
            actor: node_client_actor,
        });

        // Trigger connection validation. Due to how the JSON-RPC client works, it will keep
        // retrying connection using a different URL each time.
        node_client.valid_connection().await;

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
            node_sync_batch_size,
            genesis_hash,
            genesis_prev_hash,
            sync_address_batch_length,
            max_vt_weight,
            max_dr_weight,
            consensus_constants,
            use_unconfirmed_utxos,
            pending_transactions_timeout_seconds,
        };

        let last_beacon = Arc::new(RwLock::new(CheckpointBeacon {
            checkpoint: 0,
            hash_prev_block: genesis_prev_hash,
        }));
        let network = String::from(if testnet { "Testnet" } else { "Mainnet" });
        let node_params = params::NodeParams {
            client: node_client.clone(),
            last_beacon,
            network,
            requests_timeout,
            subscriptions: node_subscriptions,
        };

        // Start wallet actors
        let worker = actors::Worker::start(concurrency, db.clone(), node_params, params);
        let app = actors::App::start(actors::app::Params {
            testnet,
            worker,
            client: node_client,
            server_addr,
            session_expires_in,
            requests_timeout,
        });

        // Intercept SIGTERM signal to gracefully close the wallet
        signal::ctrl_c(move || {
            app.do_send(actors::app::Shutdown);
        });

        Result::<(), Error>::Ok(())
    })?;
    system.run()?;

    log::info!("Waiting for db to shut down...");
    while Arc::strong_count(&db) > 1 {
        std::thread::sleep(Duration::from_millis(500));
    }
    log::info!("Db shut down finished.");

    Ok(())
}
