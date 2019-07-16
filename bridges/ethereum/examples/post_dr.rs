use log::*;
use std::{
    sync::Arc,
    time::{SystemTime, UNIX_EPOCH},
};

use web3::{
    contract,
    futures::{future, Future},
    types::U256,
};
use witnet_data_structures::{chain::DataRequestOutput, proto::ProtobufConvert};
use witnet_ethereum_bridge::{
    config::{read_config, Config},
    eth::EthState,
};

fn data_request_example() -> DataRequestOutput {
    let start = SystemTime::now();
    let since_the_epoch = start
        .duration_since(UNIX_EPOCH)
        .expect("Time went backwards");

    let build_dr_str = include_str!("../../../examples/bitcoin_price.json");
    let build_dr: serde_json::Value = serde_json::from_str(build_dr_str).unwrap();
    let mut data_request_output: DataRequestOutput =
        serde_json::from_value(build_dr["params"]["dro"].clone()).unwrap();
    data_request_output.data_request.not_before = since_the_epoch.as_secs();

    data_request_output
}

fn eth_event_stream(
    config: Arc<Config>,
    eth_state: Arc<EthState>,
) -> impl Future<Item = (), Error = ()> {
    let wbi_contract = eth_state.wbi_contract.clone();

    let data_request_output = data_request_example();

    let tally_value = U256::from_dec_str("50000000000000000").unwrap();
    let data_request_bytes = data_request_output.to_pb_bytes().unwrap();

    wbi_contract
        .call(
            "postDataRequest",
            (data_request_bytes, tally_value),
            config.eth_account,
            contract::Options::with(|opt| {
                opt.value = Some(U256::from_dec_str("250000000000000000").unwrap());
                opt.gas = Some(1_000_000.into());
            }),
        )
        .map(|tx| {
            debug!("posted dr to wbi: {:?}", tx);
        })
        .map_err(|e| error!("Error posting dr to wbi: {}", e))
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
