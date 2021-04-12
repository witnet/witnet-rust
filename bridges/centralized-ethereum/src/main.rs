//! Witnet <> Ethereum bridge

use actix::{Actor, System, SystemRegistry};
use std::process::exit;

use witnet_centralized_ethereum_bridge::actors::{
    dr_database::DrDatabase, dr_reporter::DrReporter, dr_sender::DrSender, eth_poller::EthPoller,
    wit_poller::WitPoller,
};

fn init_logger() {
    // Info log level by default
    let mut log_level = log::LevelFilter::Info;
    if let Ok(rust_log) = std::env::var("RUST_LOG") {
        if rust_log.contains("witnet") {
            log_level = env_logger::Logger::from_default_env().filter();
        }
    }

    env_logger::Builder::from_env(env_logger::Env::default())
        .filter_module("witnet_centralized-ethereum_bridge", log_level)
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
    // Init system
    let system = System::new("node");

    // Init actors
    system.block_on(async {
        // Call cb function (register interrupt handlers)
        callback();

        // Start EthPoller actor
        let eth_poller_addr = EthPoller::default().start();
        SystemRegistry::set(eth_poller_addr);

        // Start WitPoller actor
        let wit_poller_addr = WitPoller::default().start();
        SystemRegistry::set(wit_poller_addr);

        // Start DrSender actor
        let dr_sender_addr = DrSender::default().start();
        SystemRegistry::set(dr_sender_addr);

        // Start DrReporter actor
        let dr_reporter_addr = DrReporter::default().start();
        SystemRegistry::set(dr_reporter_addr);

        // Start DrDatabase actor
        let dr_database_addr = DrDatabase::default().start();
        SystemRegistry::set(dr_database_addr);
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
