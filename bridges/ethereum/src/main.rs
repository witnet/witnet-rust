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

    let contract_address = config.wbi_contract_addr;
    // Why read files at runtime when you can read files at compile time
    let contract_abi_json = include_bytes!("../wbi_abi.json");

    let _contract = Contract::from_json(web3.eth(), contract_address, contract_abi_json).unwrap();

    // TODO: replace with actual "new data request" event
    // Filter for Hello event in our contract
    let filter = FilterBuilder::default()
        .address(vec![contract_address])
        .topics(
            Some(vec![
                "d282f389399565f3671145f5916e51652b60eee8e5c759293a2f5771b8ddfd2e"
                    .parse()
                    .unwrap(),
            ]),
            None,
            None,
            None,
        )
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
        .create_logs_filter(filter)
        .then(|filter| {
            filter
                .unwrap()
                // This poll interval was set to 0 in the example, which resulted in the
                // bridge having 100% cpu usage...
                .stream(time::Duration::from_secs(1))
                .for_each(|log| {
                    println!("Got ethereum log: {:?}", log);
                    Ok(())
                })
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
