use log::*;
use std::sync::Arc;

use web3::types::U256;
use web3::{
    contract,
    futures::{future, Future},
};
use witnet_ethereum_bridge::config::{read_config, Config};
use witnet_ethereum_bridge::eth::EthState;

fn eth_event_stream(
    _config: Arc<Config>,
    eth_state: Arc<EthState>,
) -> impl Future<Item = (), Error = ()> {
    let accounts = eth_state.accounts.clone();
    let wbi_contract = eth_state.wbi_contract.clone();

    // TODO: how to access the public mapping "requests"?
    // (this doesn't work)
    let requests: U256 = wbi_contract
        .query(
            "requests",
            (),
            accounts[0],
            contract::Options::with(|opt| {
                opt.value = Some(1000.into());
                opt.gas = Some(1_000_000.into());
            }),
            None,
        )
        .wait()
        .unwrap();
    info!("Got requests: {:?}", requests);

    future::finished(())
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
        .filter_module("post_dr", log_level)
        .init();
}

fn main() {
    init_logger();
    let config = Arc::new(read_config().unwrap());
    let eth_state = Arc::new(EthState::create(&config).unwrap());

    let ees = eth_event_stream(Arc::clone(&config), Arc::clone(&eth_state));

    tokio::run(future::ok(()).map(move |_| {
        tokio::spawn(ees);
    }));
}
