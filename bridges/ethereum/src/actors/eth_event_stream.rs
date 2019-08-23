//! Stream of Ethereum events

use crate::{
    actors::{ActorMessage, PostActorMessage},
    config::Config,
    eth::{read_u256_from_event_log, EthState, WbiEvent},
};
use async_jsonrpc_client::futures::Stream;
use futures::sink::Sink;
use log::*;
use std::{process, sync::Arc, time};
use tokio::sync::mpsc;
use web3::{
    contract,
    futures::Future,
    types::{FilterBuilder, U256},
};
use witnet_data_structures::chain::Hash;

/// Stream of ethereum events
pub fn eth_event_stream(
    config: Arc<Config>,
    eth_state: Arc<EthState>,
    _tx: mpsc::Sender<ActorMessage>,
    tx4: mpsc::Sender<PostActorMessage>,
) -> impl Future<Item = (), Error = ()> {
    let web3 = &eth_state.web3;
    let accounts = eth_state.accounts.clone();
    if !accounts.contains(&config.eth_account) {
        error!(
            "Account does not exists: {}\nAvailable accounts:\n{:#?}",
            config.eth_account, accounts
        );
        process::exit(1);
    }

    let contract_address = config.wbi_contract_addr;

    let post_dr_event_sig = eth_state.post_dr_event_sig;
    let inclusion_dr_event_sig = eth_state.inclusion_dr_event_sig;
    let post_tally_event_sig = eth_state.post_tally_event_sig;

    debug!(
        "Subscribing to contract {:?} topic {:?}",
        contract_address, post_dr_event_sig
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
            None,
            None,
            None,
        )
        .build();

    // Helper function to parse an ethereum event log as one of the possible WBI events
    let parse_as_wbi_event = move |value: &web3::types::Log| -> Result<WbiEvent, ()> {
        match &value.topics[0] {
            x if x == &post_dr_event_sig => {
                Ok(WbiEvent::PostedRequest(read_u256_from_event_log(&value)?))
            }
            x if x == &inclusion_dr_event_sig => {
                Ok(WbiEvent::IncludedRequest(read_u256_from_event_log(&value)?))
            }
            x if x == &post_tally_event_sig => {
                Ok(WbiEvent::PostedResult(read_u256_from_event_log(&value)?))
            }
            _ => Err(()),
        }
    };

    web3.eth_filter()
        .create_logs_filter(post_dr_filter)
        .map_err(|e| {
            error!("Failed to create logs filter: {}", e);
            process::exit(1);
        })
        .and_then(move |filter| {
            debug!("Created filter: {:?}", filter);
            info!("Subscribed to ethereum events");
            filter
                // This poll interval was set to 0 in the example, which resulted in the
                // bridge having 100% cpu usage...
                .stream(time::Duration::from_millis(config.eth_event_polling_rate_ms))
                .map_err(|e| error!("ethereum event error = {:?}", e))
                .map(move |value| {
                    debug!("Got ethereum event: {:?}", value);

                    parse_as_wbi_event(&value)
                })
                .for_each(move |value| {
                    let tx4 = tx4.clone();
                    let eth_state2 = eth_state.clone();
                    let fut: Box<dyn Future<Item = (), Error = ()> + Send> =
                        match value {
                            Ok(WbiEvent::PostedRequest(dr_id)) => {
                                info!("[{}] New data request posted to WBI", dr_id);

                                Box::new(
                                    eth_state.wbi_requests.write().map(move |mut wbi_requests| {
                                        wbi_requests.insert_posted(dr_id);
                                    }).and_then(move |()| {
                                        tx4.send(PostActorMessage::NewDr(dr_id))
                                            .map(|_| ())
                                            .map_err(|e| error!("Error sending message to PostActorMessage channel: {:?}", e))
                                    })
                                )
                            }
                            Ok(WbiEvent::IncludedRequest(dr_id)) => {
                                let contract = &eth_state.wbi_contract;
                                debug!("[{}] Reading dr_tx_hash for id", dr_id);
                                Box::new(
                                    contract
                                        .query(
                                            "readDrHash",
                                            (dr_id,),
                                            config.eth_account,
                                            contract::Options::default(),
                                            None,
                                        )
                                        .map_err(|e| error!("{:?}", e))
                                        .and_then(move |dr_tx_hash: U256| {
                                            let dr_tx_hash = Hash::SHA256(dr_tx_hash.into());
                                            info!(
                                                "[{}] Data request included in witnet with dr_tx_hash: {}",
                                                dr_id, dr_tx_hash
                                            );

                                            eth_state2.wbi_requests.write().map(move |mut wbi_requests| {
                                                wbi_requests.insert_included(dr_id, dr_tx_hash);
                                            })
                                        })
                                )
                            }
                            Ok(WbiEvent::PostedResult(dr_id)) => {
                                info!("[{}] Data request has been resolved!", dr_id);

                                // TODO: actually get result?
                                let result = vec![];
                                Box::new(eth_state.wbi_requests.write().map(move |mut wbi_requests| {
                                    wbi_requests.insert_result(dr_id, result);
                                }))
                            }
                            _ => {
                                warn!("Received unknown ethereum event");
                                Box::new(futures::finished(()))
                            }
                        };

                    fut
                })
                // Without this line the stream will stop on the first failure
                .then(|_| Ok(()))
        })
}
