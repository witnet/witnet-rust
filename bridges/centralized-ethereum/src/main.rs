//! Witnet <> Ethereum bridge

use actix::{Actor, System, SystemRegistry};
use std::{path::PathBuf, process::exit, sync::Arc};
use structopt::StructOpt;

use web3::contract::Contract;
use web3::{contract, types::U256};
use witnet_centralized_ethereum_bridge::config::Config;
use witnet_centralized_ethereum_bridge::{
    actors::{
        dr_database::DrDatabase, dr_reporter::DrReporter, dr_sender::DrSender,
        eth_poller::EthPoller, wit_poller::WitPoller,
    },
    config,
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

async fn post_example_dr(config: Arc<Config>) {
    log::info!("Posting example DR");
    let web3_http = web3::transports::Http::new(&config.eth_client_url)
        .map_err(|e| format!("Failed to connect to Ethereum client.\nError: {:?}", e))
        .unwrap();
    let web3 = web3::Web3::new(web3_http);
    // Why read files at runtime when you can read files at compile time
    let wrb_contract_abi_json: &[u8] = include_bytes!("../wrb_abi.json");
    let wrb_contract_abi = ethabi::Contract::load(wrb_contract_abi_json)
        .map_err(|e| format!("Unable to load WRB contract from ABI: {:?}", e))
        .unwrap();
    let wrb_contract_address = config.wrb_contract_addr;
    let wrb_contract = Contract::new(web3.eth(), wrb_contract_address, wrb_contract_abi);

    let tally_value = U256::from_dec_str("500000000000000").unwrap();

    log::info!("calling postDataRequest");

    let res = wrb_contract
        .call_with_confirmations(
            "postDataRequest",
            (config.block_relay_contract_addr, U256::from(0), tally_value),
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
