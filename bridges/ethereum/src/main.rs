use async_jsonrpc_client::{futures::Stream, DuplexTransport, Transport};
use bimap::BiMap;
use ethabi::{Bytes, Token};
use futures::sink::Sink;
use log::*;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::{net::SocketAddr, path::Path, sync::Arc, time};
use tokio::sync::mpsc;
use web3::{
    contract,
    contract::Contract,
    futures::{future, Future},
    types::U256,
    types::{FilterBuilder, H160},
};
use witnet_data_structures::{
    chain::{Block, DataRequestOutput, Hash, Hashable},
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
    web3: &mut web3::Web3<web3::transports::Http>,
    tx: mpsc::Sender<ActorMessage>,
) -> impl Future<Item = (), Error = ()> {
    // Example from
    // https://github.com/tomusdrw/rust-web3/blob/master/examples/simple_log_filter.rs

    let accounts = web3.eth().accounts().wait().unwrap();
    debug!("Web3 accounts: {:?}", accounts);
    let account0 = accounts[0];

    // Why read files at runtime when you can read files at compile time
    let contract_abi_json: &[u8] = include_bytes!("../wbi_abi.json");
    let contract_abi = ethabi::Contract::load(contract_abi_json).unwrap();
    let contract_address = config.wbi_contract_addr;
    let contract = Contract::new(web3.eth(), contract_address, contract_abi.clone());

    //debug!("WBI events: {:?}", contract_abi.events);
    let post_dr_event = contract_abi.event("PostDataRequest").unwrap().clone();
    let inclusion_dr_event = contract_abi.event("InclusionDataRequest").unwrap().clone();
    let post_tally_event = contract_abi.event("PostResult").unwrap().clone();

    let post_dr_event_sig = post_dr_event.signature();
    let inclusion_dr_event_sig = inclusion_dr_event.signature();
    let post_tally_event_sig = post_tally_event.signature();

    /*
    let post_dr_filter = FilterBuilder::default()
        .from_block(0.into())
        //.address(vec![contract_address])
        .topic_filter(
                post_dr_event.filter(RawTopicFilter::default()).unwrap()

        )
        .build();
    */

    // Example call
    /*
    let call_future = contract
        .call("hello", (), accounts[0], Options::default())
        .then(|tx| {
            debug!("got tx: {:?}", tx);
            Result::<(), ()>::Ok(())
        });
    */

    info!(
        "Subscribing to contract {:?} topic {:?}",
        contract_address,
        post_dr_event.signature()
    );
    let post_dr_filter = FilterBuilder::default()
        .from_block(0.into())
        .address(vec![contract_address])
        .topics(
            Some(vec![
                post_dr_event_sig,
                inclusion_dr_event_sig,
                post_tally_event_sig,
            ]),
            None, //Some(vec![inclusion_dr_event.signature()]),
            None, //Some(vec![post_tally_event.signature()]),
            None,
        )
        .build();

    web3.eth_filter()
        .create_logs_filter(post_dr_filter)
        .map_err(|e| error!("Failed to create logs filter: {}", e))
        .then(move |filter| {
            // TODO: for some reason, this is never executed
            let filter = filter.unwrap();
            debug!("Created filter: {:?}", filter);
            filter
                // This poll interval was set to 0 in the example, which resulted in the
                // bridge having 100% cpu usage...
                .stream(time::Duration::from_secs(1))
                .map_err(|e| error!("ethereum event error = {:?}", e))
                .map(move |value| {
                    let tx3 = tx.clone();
                    debug!("Got ethereum event: {:?}", value);
                    let fut: Box<dyn Future<Item = (), Error = ()> + Send> = match &value.topics[0]
                    {
                        x if x == &post_dr_event_sig => {
                            debug!("PostDrEvent types: {:?}", post_dr_event.inputs);
                            let event_types = vec![ethabi::ParamType::Uint(0)];
                            let event_data = ethabi::decode(&event_types, &value.data.0);
                            debug!("Event data: {:?}", event_data);
                            let dr_id = &event_data.unwrap()[0];
                            info!("New posted data request, id: {}", dr_id);
                            // Get data request info
                            let dr_id = match dr_id {
                                Token::Uint(x) => *x,
                                _ => panic!("Wrong type"),
                            };

                            let contract = contract.clone();
                            Box::new(
                                contract
                                    .query(
                                        "read_dr",
                                        (dr_id,),
                                        account0,
                                        contract::Options::default(),
                                        None,
                                    )
                                    .then(move |res| {
                                        let dr_bytes: Bytes = res.unwrap();
                                        //let dr_string = String::from_utf8_lossy(&dr_bytes);
                                        //debug!("{}", dr_string);

                                        // Claim dr
                                        let poe: Bytes = vec![];
                                        info!("Claiming dr {}", dr_id);

                                        contract
                                            .call(
                                                "claim_drs",
                                                (vec![dr_id], poe),
                                                account0,
                                                contract::Options::default(),
                                            )
                                            .then(|tx| {
                                                debug!("claim_drs tx: {:?}", tx);
                                                Result::<(), ()>::Ok(())
                                            })
                                            .then(move |_| {
                                                let dr_output = serde_json::from_str(
                                                    &String::from_utf8_lossy(&dr_bytes),
                                                )
                                                .unwrap();
                                                // Assuming claim is successful
                                                // Post dr in witnet
                                                tx3.send(ActorMessage::PostDr(dr_id, dr_output))
                                                    .map(|_| ())
                                                    .map_err(|_| ())
                                            })
                                    }),
                            )
                        }
                        x if x == &inclusion_dr_event_sig => {
                            debug!(
                                "InclusionDataRequest types: {:?}",
                                inclusion_dr_event.inputs
                            );
                            let event_types = vec![ethabi::ParamType::Uint(0)];
                            let event_data = ethabi::decode(&event_types, &value.data.0);
                            debug!("Event data: {:?}", event_data);
                            let dr_id = &event_data.unwrap()[0];
                            // Get data request info
                            let dr_id = match dr_id {
                                Token::Uint(x) => *x,
                                _ => panic!("Wrong type"),
                            };

                            debug!("Reading dr_tx_hash for id {}", dr_id);
                            Box::new(
                                contract
                                    .query(
                                        "read_dr_hash",
                                        (dr_id,),
                                        accounts[0],
                                        contract::Options::default(),
                                        None,
                                    )
                                    .then(move |res: Result<U256, _>| {
                                        let dr_tx_hash = res.unwrap();
                                        let dr_tx_hash = Hash::SHA256(dr_tx_hash.into());
                                        info!(
                                            "New included data request, id: {} with dr_tx_hash: {}",
                                            dr_id, dr_tx_hash
                                        );
                                        tx3.send(ActorMessage::WaitForTally(dr_id, dr_tx_hash))
                                            .map(|_| ())
                                            .map_err(|_| ())
                                    }),
                            )
                        }
                        x if x == &post_tally_event_sig => {
                            debug!("PostResult types: {:?}", inclusion_dr_event.inputs);
                            let event_types = vec![ethabi::ParamType::Uint(0)];
                            let event_data = ethabi::decode(&event_types, &value.data.0);
                            debug!("Event data: {:?}", event_data);
                            let dr_id = &event_data.unwrap()[0];
                            // Get data request info
                            let dr_id = match dr_id {
                                Token::Uint(x) => *x,
                                _ => panic!("Wrong type"),
                            };

                            info!("Data request with id: {} has been resolved!", dr_id);
                            Box::new(
                                tx3.send(ActorMessage::TallyClaimed(dr_id))
                                    .map(|_| ())
                                    .map_err(|_| ()),
                            )
                        }
                        _ => {
                            panic!("Received unknown ethereum event");
                        }
                    };

                    tokio::spawn(fut);
                })
                .for_each(|_| Ok(()))
        })

    /*
    web3.eth_filter().create_blocks_filter().then(|filter| {
        filter.unwrap().stream(time::Duration::from_secs(1))
            .map_err(|e| error!("ethereum block filter error = {:?}", e))
            .then(move |block_hash| {
                debug!("Got ethereum block: {:?}", block_hash.unwrap());
                web3.eth().block(BlockId::Hash(block_hash.unwrap())).map(|block| {
                    debug!("Block contents: {:?}", block);
                })
            })
            .for_each(|_| Ok(()))
    }).map_err(|e| error!("ethereum block filter could not be created: {:?}", e))
    */
}

fn witnet_block_stream(
    config: Arc<Config>,
    tx: mpsc::Sender<ActorMessage>,
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
        .map_err(|e| error!("witnet notification error = {:?}", e))
        .then(move |value| {
            let value = value.unwrap();
            let tx1 = tx.clone();
            // TODO: get current epoch to distinguish between old blocks that are sent
            // to us while synchronizing and new blocks
            let block = serde_json::from_value::<Block>(value).unwrap();
            debug!("Got witnet block: {:?}", block);
            tx1.send(ActorMessage::NewWitnetBlock(Box::new(block)))
        })
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

enum ActorMessage {
    PostDr(U256, DataRequestOutput),
    WaitForTally(U256, Hash),
    TallyClaimed(U256),
    NewWitnetBlock(Box<Block>),
}

fn main_actor(
    config: Arc<Config>,
    web3: &mut web3::Web3<web3::transports::Http>,
    rx: mpsc::Receiver<ActorMessage>,
) -> impl Future<Item = (), Error = ()> {
    let mut claimed_drs = BiMap::new();
    let mut waiting_for_tally = BiMap::new();

    let accounts = web3.eth().accounts().wait().unwrap();
    debug!("Web3 accounts: {:?}", accounts);

    // Why read files at runtime when you can read files at compile time
    let contract_abi_json: &[u8] = include_bytes!("../wbi_abi.json");
    let contract_abi = ethabi::Contract::load(contract_abi_json).unwrap();
    let contract_address = config.wbi_contract_addr;
    let contract = Contract::new(web3.eth(), contract_address, contract_abi.clone());

    let witnet_addr = config.witnet_jsonrpc_addr.to_string();
    // Important: the handle cannot be dropped, otherwise the client stops
    // processing events
    let (handle, witnet_client) =
        async_jsonrpc_client::transports::tcp::TcpSocket::new(&witnet_addr).unwrap();

    rx.for_each(move |value| {
        // Force moving handle to closure to avoid drop
        let _ = &handle;
        match value {
            ActorMessage::PostDr(dr_id, dr_output) => {
                let bdr_params = json!({"dro": dr_output, "fee": 0});
                let bdr_res = witnet_client.execute("buildDataRequest", bdr_params).wait();
                // TODO: this method should return the transaction hash,
                // so we can identify the transaction later in the block
                debug!("buildDataRequest: {:?}", bdr_res);
                claimed_drs.insert(dr_id, dr_output.hash());
            }
            ActorMessage::WaitForTally(dr_id, dr_tx_hash) => {
                claimed_drs.remove_by_left(&dr_id);
                waiting_for_tally.insert(dr_id, dr_tx_hash);
            }
            ActorMessage::TallyClaimed(dr_id) => {
                waiting_for_tally.remove_by_left(&dr_id);
            }
            ActorMessage::NewWitnetBlock(block) => {
                let block_hash: U256 = match block.hash() {
                    Hash::SHA256(x) => x.into(),
                };
                for dr in &block.txns.data_request_txns {
                    if let Some((dr_id, _)) = claimed_drs.remove_by_right(&dr.body.dr_output.hash())
                    {
                        let dr_inclusion_proof = dr.data_proof_of_inclusion(&block).unwrap();
                        debug!(
                            "Proof of inclusion for data request {}:\nData: {:?}\n{:?}",
                            dr.hash(),
                            dr.body.dr_output.to_pb_bytes().unwrap(),
                            dr_inclusion_proof
                        );
                        info!("Claimed dr got included in witnet block!");
                        info!("Sending proof of inclusion to WBI contract");

                        //let poi = dr_inclusion_proof.lemma;
                        let poi: Bytes = vec![];
                        // TODO: since the poi is not implemented, we have no way to get
                        // the dr transaction hash later on, for the tally.
                        // So we send the dr transaction hash as the block hash, which
                        // under the current implementation of the WBI will be stored as
                        // dr_hash
                        let drtx_hash: U256 = match dr.hash() {
                            Hash::SHA256(x) => x.into(),
                        };
                        tokio::spawn(
                            contract
                                .call(
                                    "report_dr_inclusion",
                                    (dr_id, poi, drtx_hash),
                                    accounts[0],
                                    contract::Options::default(),
                                )
                                .then(|tx| {
                                    debug!("report_dr_inclusion tx: {:?}", tx);
                                    Result::<(), ()>::Ok(())
                                }),
                        );
                    }
                }

                for tally in &block.txns.tally_txns {
                    if let Some((dr_id, _)) = waiting_for_tally.remove_by_right(&tally.dr_pointer) {
                        // Call report_result method of the WBI
                        let tally_inclusion_proof = tally.data_proof_of_inclusion(&block).unwrap();
                        let Hash::SHA256(dr_pointer_bytes) = tally.dr_pointer;
                        debug!(
                            "Proof of inclusion for tally        {}:\nData: {:?}\n{:?}",
                            tally.hash(),
                            [&dr_pointer_bytes[..], &tally.tally].concat(),
                            tally_inclusion_proof
                        );

                        // Call report_result
                        //let poi = dr_inclusion_proof.lemma;
                        let poi: Bytes = vec![];
                        let result: Bytes = tally.tally.clone();
                        tokio::spawn(
                            contract
                                .call(
                                    "report_result",
                                    (dr_id, poi, block_hash, result),
                                    accounts[0],
                                    contract::Options::default(),
                                )
                                .then(|tx| {
                                    debug!("report_result tx: {:?}", tx);
                                    Result::<(), ()>::Ok(())
                                }),
                        );
                    }
                }
            }
        }

        Ok(())
    })
    .map(|_| ())
    .map_err(|_| ())
}

fn main() {
    init_logger();
    let config = Arc::new(read_config());
    let (_eloop, web3_http) = web3::transports::Http::new(&config.eth_client_url).unwrap();
    let mut web3 = web3::Web3::new(web3_http);

    let (tx1, rx) = mpsc::channel(16);
    let tx2 = tx1.clone();

    let ees = eth_event_stream(Arc::clone(&config), &mut web3, tx1);
    let (_handle, wbs) = witnet_block_stream(Arc::clone(&config), tx2);
    let act = main_actor(Arc::clone(&config), &mut web3, rx);

    tokio::run(future::ok(()).map(move |_| {
        tokio::spawn(wbs);
        tokio::spawn(ees);
        tokio::spawn(act);
    }));
}
