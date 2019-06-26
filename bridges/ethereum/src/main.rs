use async_jsonrpc_client::{futures::Stream, DuplexTransport, Transport};
use log::*;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::{net::SocketAddr, path::Path, sync::Arc, time};
use web3::{
    contract::Contract,
    futures::{future, Future},
    types::FilterBuilder,
    types::H160,
};
use witnet_data_structures::{
    chain::{Block, Hash, Hashable},
    proto::ProtobufConvert,
};

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Config {
    witnet_jsonrpc_addr: SocketAddr,
    eth_client_url: String,
    wbi_contract_addr: H160,
    eth_account: H160,
}

/// Load configuration from a file written in Toml format.
fn from_file<S: AsRef<Path>>(file: S) -> Result<Config, toml::de::Error> {
    use std::fs::File;
    use std::io::Read;

    let f = file.as_ref();
    let mut contents = String::new();

    debug!("Loading config from `{}`", f.to_string_lossy());

    let mut file = File::open(file).unwrap();
    file.read_to_string(&mut contents).unwrap();
    toml::from_str(&contents)
}

fn read_config() -> Config {
    from_file("witnet_ethereum_bridge.toml").unwrap()
}

fn eth_event_stream(
    config: Arc<Config>,
    web3: web3::Web3<web3::transports::Http>,
) -> impl Future<Item = (), Error = ()> {
    // Example from
    // https://github.com/tomusdrw/rust-web3/blob/master/examples/simple_log_filter.rs

    let accounts = web3.eth().accounts().wait().unwrap();
    debug!("Web3 accounts: {:?}", accounts);

    // Why read files at runtime when you can read files at compile time
    let contract_abi_json: &[u8] = include_bytes!("../wbi_abi.json");
    let contract_abi = ethabi::Contract::load(contract_abi_json).unwrap();
    let contract_address = config.wbi_contract_addr;
    let _contract = Contract::new(web3.eth(), contract_address, contract_abi.clone());

    // TODO: replace with actual "new data request" event
    //debug!("WBI events: {:?}", contract_abi.events);
    let post_dr_event = contract_abi.event("PostDataRequest").unwrap();
    /*
    let post_dr_filter = FilterBuilder::default()
        .from_block(0.into())
        //.address(vec![contract_address])
        .topic_filter(
                post_dr_event.filter(RawTopicFilter::default()).unwrap()

        )
        .build();
    */

    info!(
        "Subscribing to contract {:?} topic {:?}",
        contract_address,
        post_dr_event.signature()
    );
    let post_dr_filter = FilterBuilder::default()
        //.from_block(0.into())
        .address(vec![contract_address])
        .topics(Some(vec![post_dr_event.signature()]), None, None, None)
        .build();

    // Example call
    /*
    let call_future = contract
        .call("hello", (), accounts[0], Options::default())
        .then(|tx| {
            debug!("got tx: {:?}", tx);
            Result::<(), ()>::Ok(())
        });
    */

    web3.eth_filter()
        .create_logs_filter(post_dr_filter)
        .then(|filter| {
            // TODO: for some reason, this is never executed
            let filter = filter.unwrap();
            debug!("Created filter: {:?}", filter);
            filter
                // This poll interval was set to 0 in the example, which resulted in the
                // bridge having 100% cpu usage...
                .stream(time::Duration::from_secs(0))
                .map(|value| {
                    debug!("Got ethereum event: {:?}", value);
                })
                .map_err(|e| error!("ethereum event error = {:?}", e))
                .for_each(|_| Ok(()))
        })
        .map_err(|_| ())
}

fn witnet_block_stream(
    config: Arc<Config>,
) -> (
    async_jsonrpc_client::transports::shared::EventLoopHandle,
    impl Future<Item = (), Error = ()>,
) {
    let witnet_addr = config.witnet_jsonrpc_addr.to_string();
    // Important: the handle cannot be dropped, otherwise the client stops
    // processing events
    let (handle, witnet_client) =
        async_jsonrpc_client::transports::tcp::TcpSocket::new(&witnet_addr).unwrap();
    let witnet_subscription_id_value = witnet_client
        .execute("witnet_subscribe", json!(["newBlocks"]))
        .wait()
        .unwrap();
    let witnet_subscription_id: String = match witnet_subscription_id_value {
        serde_json::Value::String(s) => s,
        _ => panic!("Not a string"),
    };
    info!(
        "Subscribed to witnet newBlocks with subscription id \"{}\"",
        witnet_subscription_id
    );

    let fut = witnet_client
        .subscribe(&witnet_subscription_id.into())
        .map(|value| {
            // TODO: get current epoch to distinguish between old blocks that are sent
            // to us while synchronizing and new blocks
            let block = serde_json::from_value::<Block>(value).unwrap();
            debug!("Got witnet block: {:?}", block);

            for dr in &block.txns.data_request_txns {
                let dr_inclusion_proof = dr.data_proof_of_inclusion(&block).unwrap();
                debug!(
                    "Proof of inclusion for data request {}:\nData: {:?}\n{:?}",
                    dr.hash(),
                    dr.body.dr_output.to_pb_bytes().unwrap(),
                    dr_inclusion_proof
                );
            }

            for tally in &block.txns.tally_txns {
                let tally_inclusion_proof = tally.data_proof_of_inclusion(&block).unwrap();
                let Hash::SHA256(dr_pointer_bytes) = tally.dr_pointer;
                debug!(
                    "Proof of inclusion for tally        {}:\nData: {:?}\n{:?}",
                    tally.hash(),
                    [&dr_pointer_bytes[..], &tally.tally].concat(),
                    tally_inclusion_proof
                );
            }
        })
        .map_err(|e| error!("witnet notification error = {:?}", e))
        .for_each(|_| Ok(()))
        .then(|_| Ok(()));

    (handle, fut)
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
        .filter_module("witnet_ethereum_bridge", log_level)
        .init();
}

fn main() {
    init_logger();
    let config = Arc::new(read_config());
    let (_eloop, web3_http) = web3::transports::Http::new(&config.eth_client_url).unwrap();
    let web3 = web3::Web3::new(web3_http);
    let ees = eth_event_stream(Arc::clone(&config), web3);
    let (_handle, wbs) = witnet_block_stream(Arc::clone(&config));

    tokio::run(future::ok(()).map(move |_| {
        tokio::spawn(wbs);
        tokio::spawn(ees);
    }));
}
