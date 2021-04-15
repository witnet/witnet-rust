//! Witnet <> Ethereum bridge

use actix::{Actor, System, SystemRegistry};
use std::{path::PathBuf, process::exit, sync::Arc};
use structopt::StructOpt;

use web3::{contract, types::U256};
use witnet_centralized_ethereum_bridge::{
    actors::{
        dr_database::DrDatabase, dr_reporter::DrReporter, dr_sender::DrSender,
        eth_poller::EthPoller, wit_poller::WitPoller,
    },
    check_ethereum_node_running, check_witnet_node_running, config, create_wrb_contract,
};

/// Command line usage and flags
#[derive(Debug, StructOpt)]
struct App {
    /// Path of the config file
    #[structopt(short = "c", long)]
    config: Option<PathBuf>,
    /// Post data request and exit
    #[structopt(long = "post-dr")]
    post_dr: bool,
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

async fn post_example_dr(config: Arc<config::Config>) {
    log::info!("Posting an example of Data Request");
    let wrb_contract = create_wrb_contract(&config);
    let tally_value = U256::from_dec_str("500000000000000").unwrap();

    log::info!("calling postDataRequest");

    let res = wrb_contract
        .call_with_confirmations(
            "postDataRequest",
            (
                config.request_example_contract_addr,
                U256::from(0),
                tally_value,
            ),
            config.eth_account,
            contract::Options::with(|opt| {
                opt.value = Some(U256::from_dec_str("2500000000000000").unwrap());
                // The cost of posting a data request is mainly the storage, so
                // big data requests may need bigger amounts of gas
                opt.gas = config.gas_limits.post_data_request.map(Into::into);
            }),
            1,
        )
        .await;
    log::info!("The receipt is {:?}", res);
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
    let config = config::from_file(
        app.config
            .unwrap_or_else(|| "witnet_centralized_ethereum_bridge.toml".into()),
    )
    .map(Arc::new)
    .map_err(|e| format!("Error reading configuration file: {}", e))?;

    // Init system
    let system = System::new("bridge");
    let condition = app.post_dr;

    // Init actors
    system.block_on(async {
        // Call cb function (register interrupt handlers)
        callback();

        if condition {
            post_example_dr(config).await;
            log::info!("post post_example DR");
        } else {
            // Check if ethereum and witnet nodes are running
            check_ethereum_node_running(&config)
                .await
                .expect("ethereum node not running");
            check_witnet_node_running(&config)
                .await
                .expect("witnet node not running");

            // Start EthPoller actor
            // TODO: Remove unwrap
            let eth_poller_addr = EthPoller::from_config(&config).unwrap().start();
            SystemRegistry::set(eth_poller_addr);

            // Start WitPoller actor
            let wit_poller_addr = WitPoller::from_config(&config).unwrap().start();
            SystemRegistry::set(wit_poller_addr);

            // Start DrSender actor
            let dr_sender_addr = DrSender::from_config(&config).unwrap().start();
            SystemRegistry::set(dr_sender_addr);

            // Start DrReporter actor
            let dr_reporter_addr = DrReporter::from_config(&config).unwrap().start();
            SystemRegistry::set(dr_reporter_addr);

            // Start DrDatabase actor
            let dr_database_addr = DrDatabase::default().start();
            SystemRegistry::set(dr_database_addr);
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
