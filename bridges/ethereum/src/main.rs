//! Witnet <> Ethereum bridge
use log::*;
use std::{
    path::PathBuf,
    sync::Arc,
    time::{SystemTime, UNIX_EPOCH},
};
use structopt::StructOpt;
use web3::{
    contract,
    futures::{future, Future},
    types::U256,
};
use witnet_data_structures::{chain::DataRequestOutput, proto::ProtobufConvert};
use witnet_ethereum_bridge::{
    actors::{
        block_relay_and_poi::block_relay_and_poi,
        block_relay_check::block_relay_check,
        claim_and_post::{claim_and_post, claim_ticker},
        eth_event_stream::eth_event_stream,
        tally_finder::tally_finder,
        wbi_requests_initial_sync::wbi_requests_initial_sync,
        witnet_block_stream::witnet_block_stream,
    },
    config::Config,
    eth::EthState,
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
        .filter_module("witnet_ethereum_bridge", log_level)
        .init();
}

fn data_request_example() -> DataRequestOutput {
    let start = SystemTime::now();
    let since_the_epoch = start
        .duration_since(UNIX_EPOCH)
        .expect("Time went backwards");

    let build_dr_str = include_str!("../../../examples/bitcoin_price.json");
    let build_dr: serde_json::Value = serde_json::from_str(build_dr_str).unwrap();
    let mut data_request_output: DataRequestOutput =
        serde_json::from_value(build_dr["params"]["dro"].clone()).unwrap();
    data_request_output.data_request.time_lock = since_the_epoch.as_secs();

    data_request_output
}

fn post_example_dr(
    config: Arc<Config>,
    eth_state: Arc<EthState>,
) -> impl Future<Item = (), Error = ()> {
    let wbi_contract = eth_state.wbi_contract.clone();

    let data_request_output = data_request_example();

    let tally_value = U256::from_dec_str("500000000000000").unwrap();
    let data_request_bytes = data_request_output.to_pb_bytes().unwrap();

    wbi_contract
        .call(
            "postDataRequest",
            (data_request_bytes, tally_value),
            config.eth_account,
            contract::Options::with(|opt| {
                opt.value = Some(U256::from_dec_str("2500000000000000").unwrap());
                // The cost of posting a data request is mainly the storage, so
                // big data requests may need bigger amounts of gas
                opt.gas = Some(1_000_000.into());
            }),
        )
        .map(|tx| {
            info!("posted dr to wbi: {:?}", tx);
        })
        .map_err(|e| error!("Error posting dr to wbi: {}", e))
}

/// Command line usage and flags
#[derive(Debug, StructOpt)]
struct App {
    /// Path of the config file
    #[structopt(short = "c", long)]
    config: Option<PathBuf>,
    /// Post data request and exit
    #[structopt(long = "post-dr")]
    post_dr: bool,
    /// Read data requests state and exit
    #[structopt(long = "read-requests", conflicts_with = "post_dr")]
    read_requests: bool,
}

fn main() {
    init_logger();

    if let Err(err) = run() {
        error!("{}", err);
        std::process::exit(1);
    }
}

fn run() -> Result<(), String> {
    let app = App::from_args();

    let config = witnet_ethereum_bridge::config::from_file(
        app.config
            .unwrap_or_else(|| "witnet_ethereum_bridge.toml".into()),
    )
    .map(Arc::new)
    .map_err(|e| format!("Error reading configuration file: {}", e))?;

    let eth_state = EthState::create(&config).map(Arc::new)?;

    if app.post_dr {
        // Post example data request to WBI and exit
        let fut = post_example_dr(Arc::clone(&config), Arc::clone(&eth_state));
        tokio::run(future::ok(()).map(move |_| {
            tokio::spawn(fut);
        }));
    } else {
        let wbi_requests_initial_sync_fut =
            wbi_requests_initial_sync(Arc::clone(&config), Arc::clone(&eth_state));
        if app.read_requests {
            // Read all the requests from WBI and exit
            tokio::run(future::ok(()).map(move |_| {
                tokio::spawn(wbi_requests_initial_sync_fut);
            }));
        } else {
            let (block_relay_check_tx, block_relay_check_fut) =
                block_relay_check(&config, Arc::clone(&eth_state));
            let (block_relay_and_poi_tx, block_relay_and_poi_fut) = block_relay_and_poi(
                Arc::clone(&config),
                Arc::clone(&eth_state),
                block_relay_check_tx,
            );
            let (_handle, claim_and_post_tx, claim_and_post_fut) =
                claim_and_post(Arc::clone(&config), Arc::clone(&eth_state));
            let eth_event_fut =
                eth_event_stream(&config, Arc::clone(&eth_state), claim_and_post_tx.clone());
            let (_handle, witnet_block_fut) =
                witnet_block_stream(Arc::clone(&config), block_relay_and_poi_tx.clone());
            let claim_ticker_fut = claim_ticker(Arc::clone(&config), claim_and_post_tx);

            let (_handle, tally_finder_fut) = tally_finder(
                Arc::clone(&config),
                Arc::clone(&eth_state),
                block_relay_and_poi_tx,
            );

            tokio::run(future::ok(()).map(move |_| {
                // Wait here to ensure that the Ethereum client is running before starting
                // the entire system
                let eth_event_fut = match eth_event_fut.wait() {
                    Ok(x) => x,
                    Err(e) => {
                        error!("{}", e);
                        return;
                    }
                };

                // Wait here to ensure that the Witnet node is running before starting
                // the entire system
                let witnet_event_fut = match witnet_block_fut.wait() {
                    Ok(x) => x,
                    Err(e) => {
                        error!("{}", e);
                        return;
                    }
                };

                tokio::spawn(wbi_requests_initial_sync_fut);
                if config.subscribe_to_witnet_blocks {
                    tokio::spawn(witnet_event_fut);
                }
                tokio::spawn(eth_event_fut);
                tokio::spawn(claim_and_post_fut);
                tokio::spawn(block_relay_and_poi_fut);
                tokio::spawn(claim_ticker_fut);
                tokio::spawn(block_relay_check_fut);
                tokio::spawn(tally_finder_fut);
            }));
        }
    }

    Ok(())
}
