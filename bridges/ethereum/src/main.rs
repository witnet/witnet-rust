use async_jsonrpc_client::{futures::Stream, DuplexTransport, Transport};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::{net::SocketAddr, path::Path, sync::Arc, time};
use web3::{
    contract::Contract,
    futures::{future, Future},
    types::FilterBuilder,
    types::H160,
};
use witnet_data_structures::chain::Block;

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

    println!("Loading config from `{}`", f.to_string_lossy());

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
    println!("Web3 accounts: {:?}", accounts);

    // Why read files at runtime when you can read files at compile time
    let contract_abi_json: &[u8] = include_bytes!("../wbi_abi.json");
    let contract_abi = ethabi::Contract::load(contract_abi_json).unwrap();
    let contract_address = config.wbi_contract_addr;
    let _contract = Contract::new(web3.eth(), contract_address, contract_abi.clone());

    // TODO: replace with actual "new data request" event
    //println!("WBI events: {:?}", contract_abi.events);
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

    println!(
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
            println!("got tx: {:?}", tx);
            Result::<(), ()>::Ok(())
        });
    */

    web3.eth_filter()
        .create_logs_filter(post_dr_filter)
        .then(|filter| {
            // TODO: for some reason, this is never executed
            let filter = filter.unwrap();
            println!("Created filter: {:?}", filter);
            filter
                // This poll interval was set to 0 in the example, which resulted in the
                // bridge having 100% cpu usage...
                .stream(time::Duration::from_secs(0))
                .map(|value| {
                    println!("Got ethereum event: {:?}", value);
                })
                .map_err(|e| println!("ethereum event error = {:?}", e))
                .for_each(|_| Ok(()))
        })
        .map_err(|_| ())
}

fn witnet_block_stream(config: Arc<Config>) -> impl Future<Item = (), Error = ()> {
    let witnet_addr = config.witnet_jsonrpc_addr.to_string();
    let (_handle, witnet_client) =
        async_jsonrpc_client::transports::tcp::TcpSocket::new(&witnet_addr).unwrap();
    let witnet_subscription_id_value = witnet_client
        .execute("witnet_subscribe", json!(["newBlocks"]))
        .wait()
        .unwrap();
    let witnet_subscription_id: String = match witnet_subscription_id_value {
        serde_json::Value::String(s) => s,
        _ => panic!("Not a string"),
    };
    println!(
        "Suscribed to witnet newBlocks with subscription id {}",
        witnet_subscription_id
    );

    witnet_client
        .subscribe(&witnet_subscription_id.into())
        .map(|value| {
            let block = serde_json::from_value::<Block>(value).unwrap();
            println!("Got witnet block: {:?}", block);
        })
        .map_err(|e| println!("witnet notification error = {:?}", e))
        .for_each(|_| Ok(()))
        .then(|_| Ok(()))
}

fn main() {
    env_logger::init();

    let config = Arc::new(read_config());
    let (_eloop, web3_http) = web3::transports::Http::new(&config.eth_client_url).unwrap();
    let web3 = web3::Web3::new(web3_http);
    let ees = eth_event_stream(Arc::clone(&config), web3);

    let wbs = witnet_block_stream(Arc::clone(&config));

    tokio::run(future::ok(()).map(move |_| {
        tokio::spawn(wbs);
        tokio::spawn(ees);
    }));
}
