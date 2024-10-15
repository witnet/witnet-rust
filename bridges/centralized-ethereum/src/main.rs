//! Witnet <> Ethereum bridge

use actix::{Actor, System, SystemRegistry};
use std::{path::PathBuf, process::exit, sync::Arc};
use structopt::StructOpt;

use witnet_centralized_ethereum_bridge::{
    actors::{
        dr_database::DrDatabase, dr_reporter::DrReporter, dr_sender::DrSender,
        eth_poller::EthPoller, watch_dog::WatchDog, wit_poller::WitPoller,
    },
    check_ethereum_node_running, check_witnet_node_running, config, create_wrb_contract,
};
use witnet_config::config::Config as NodeConfig;
use witnet_net::client::tcp::JsonRpcClient;
use witnet_node::storage_mngr;

/// Command line usage and flags
#[derive(Debug, StructOpt)]
struct App {
    /// Path of the config file
    #[structopt(short = "c", long)]
    config: Option<PathBuf>,
    /// Read config from environment
    #[structopt(long = "env", conflicts_with = "config")]
    env: bool,
}

fn init_logger() {
    // Debug log level by default
    let mut log_level = log::LevelFilter::Debug;
    if let Ok(rust_log) = std::env::var("RUST_LOG") {
        if rust_log.contains("witnet") {
            log_level = env_logger::Logger::from_default_env().filter();
        }
    }

    env_logger::Builder::from_env(env_logger::Env::default())
        .filter_module("witnet_centralized_ethereum_bridge", log_level)
        .init();
}

fn main() {
    init_logger();

    if let Err(err) = run(|| {
        // FIXME(#72): decide what to do when interrupt signals are received
        ctrlc::set_handler(move || {
            close();
        })
        .expect("Error setting handler for both SIGINT (Ctrl+C) and SIGTERM (kill)");
    }) {
        log::error!("{}", err);
        std::process::exit(1);
    }
}

/// Function to run the main system
fn run(callback: fn()) -> Result<(), String> {
    let app = App::from_args();
    let config = if app.env {
        config::from_env()
            .map(Arc::new)
            .map_err(|e| format!("Error reading configuration from environment: {}", e))?
    } else {
        config::from_file(
            app.config
                .unwrap_or_else(|| "witnet_centralized_ethereum_bridge.toml".into()),
        )
        .map(Arc::new)
        .map_err(|e| format!("Error reading configuration file: {}", e))?
    };

    // Init system
    let system = System::new();

    // Init actors
    system.block_on(async {
        // Call cb function (register interrupt handlers)
        callback();

        // Check if Ethereum and Witnet nodes are running before starting actors
        check_ethereum_node_running(&config.eth_jsonrpc_url)
            .await
            .expect("ethereum node not running");

        check_witnet_node_running(&config.witnet_jsonrpc_socket.to_string())
            .await
            .expect("witnet node not running");

        // Start DrDatabase actor
        let dr_database_addr = DrDatabase::default().start();
        SystemRegistry::set(dr_database_addr);

        // Web3 contract using HTTP transport with an Ethereum client
        let (web3, wrb_contract) =
            create_wrb_contract(&config.eth_jsonrpc_url, config.eth_witnet_oracle);

        let wrb_contract = Arc::new(wrb_contract);

        // Start EthPoller actor
        let eth_poller_addr =
            EthPoller::from_config(&config, web3.clone(), wrb_contract.clone()).start();
        SystemRegistry::set(eth_poller_addr);

        // Start DrReporter actor
        let dr_reporter_addr =
            DrReporter::from_config(&config, web3.clone(), wrb_contract.clone()).start();
        SystemRegistry::set(dr_reporter_addr);

        // Start Json-RPC actor connected to Witnet node
        let node_client = JsonRpcClient::start(&config.witnet_jsonrpc_socket.to_string())
            .expect("JSON WIT/RPC node client failed to start");

        // Start WitPoller actor
        let wit_poller_addr = WitPoller::from_config(&config, node_client.clone()).start();
        SystemRegistry::set(wit_poller_addr);

        // Start DrSender actor
        let dr_sender_addr = DrSender::from_config(&config, node_client.clone()).start();
        SystemRegistry::set(dr_sender_addr);

        // Initialize Storage Manager
        let mut node_config = NodeConfig::default();
        node_config.storage.db_path = config.storage.db_path.clone();
        storage_mngr::start_from_config(node_config);

        // Start WatchDog actor
        if config.watch_dog_enabled {
            let watch_dog_addr = WatchDog::from_config(&config, wrb_contract.clone()).start();
            SystemRegistry::set(watch_dog_addr);
        }
    });

    // Run system
    system.run().map_err(|error| error.to_string())
}

/// Function to close the main system
pub fn close() {
    log::info!("Closing bridge");

    // FIXME(#72): find out how to gracefully stop the system
    // System::current().stop();

    // Process exit
    exit(0);
}
