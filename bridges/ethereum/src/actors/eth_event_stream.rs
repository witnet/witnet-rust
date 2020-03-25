//! Stream of Ethereum events

use crate::{
    actors::ClaimMsg,
    config::Config,
    eth::{read_u256_from_event_log, EthState, WrbEvent},
};
use async_jsonrpc_client::futures::Stream;
use futures::{future::Either, sink::Sink};
use log::*;
use std::{
    sync::Arc,
    time::{Duration, Instant},
};
use tokio::{sync::mpsc, timer::Interval};
use web3::{
    contract,
    futures::Future,
    types::{FilterBuilder, U256},
};
use witnet_data_structures::chain::Hash;

/// Stream of ethereum events
/// This function returns a future which has a nested future inside.
/// This is because we want to be able to exit the process in the case when
/// we fail to connect to the node, so we await on the outer future, and in
/// the error case we exit the main function.
pub fn eth_event_stream(
    config: &Config,
    eth_state: Arc<EthState>,
    tx: mpsc::Sender<ClaimMsg>,
) -> impl Future<Item = impl Future<Item = (), Error = ()>, Error = String> {
    let web3 = &eth_state.web3;
    let accounts = eth_state.accounts.clone();
    if !accounts.contains(&config.eth_account) {
        return Either::A(futures::failed(format!(
            "Account does not exist: {}\nAvailable accounts:\n{:#?}",
            config.eth_account, accounts
        )));
    }

    let contract_address = config.wrb_contract_addr;
    let eth_event_polling_rate_ms = config.eth_event_polling_rate_ms;
    let eth_account = config.eth_account;
    let interval_ms = config.read_dr_hash_interval_ms;

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

    // Helper function to parse an ethereum event log as one of the possible WRB events
    let parse_as_wrb_event = move |value: &web3::types::Log| -> Result<WrbEvent, ()> {
        match &value.topics[0] {
            x if x == &post_dr_event_sig => {
                Ok(WrbEvent::PostedRequest(read_u256_from_event_log(&value)?))
            }
            x if x == &inclusion_dr_event_sig => {
                Ok(WrbEvent::IncludedRequest(read_u256_from_event_log(&value)?))
            }
            x if x == &post_tally_event_sig => {
                Ok(WrbEvent::PostedResult(read_u256_from_event_log(&value)?))
            }
            _ => Err(()),
        }
    };

    let fut = web3.eth_filter()
        .create_logs_filter(post_dr_filter)
        .map_err(|e| {
            format!("Failed to create logs filter: {}", e)
        })
        .map(move |filter| {
            debug!("Created filter: {:?}", filter);
            info!("Subscribed to ethereum events");
            filter
                // This poll interval was set to 0 in the example, which resulted in the
                // bridge having 100% cpu usage...
                .stream(Duration::from_millis(eth_event_polling_rate_ms))
                .then(move |res| match res {
                    Ok(value) => {
                        debug!("Got ethereum event: {:?}", value);

                        Ok(parse_as_wrb_event(&value))
                    }
                    Err(e) => {
                        error!("ethereum event error = {:?}", e);
                        // Without this line the stream will stop on the first failure
                        Ok(Err(()))
                    }
                })
                .for_each(move |value| {
                    let tx4 = tx.clone();
                    let eth_state2 = eth_state.clone();
                    let fut: Box<dyn Future<Item = (), Error = ()> + Send> =
                        match value {
                            Ok(WrbEvent::PostedRequest(dr_id)) => {
                                info!("[{}] New data request posted to WRB", dr_id);

                                Box::new(
                                    eth_state.wrb_requests.write().map(move |mut wrb_requests| {
                                        wrb_requests.insert_posted(dr_id);
                                    }).and_then(move |()| {
                                        tx4.send(ClaimMsg::NewDr(dr_id))
                                            .map(|_| ())
                                            .map_err(|e| error!("Error sending message to PostActorMessage channel: {:?}", e))
                                    })
                                )
                            }
                            Ok(WrbEvent::IncludedRequest(dr_id)) => {
                                let mut retries = 0;
                                Box::new(
                                    Interval::new(Instant::now(), Duration::from_millis(interval_ms))
                                        .map_err(|e| error!("Error creating interval: {:?}", e))
                                        .map(move |i| (i, eth_state2.clone()))
                                        .and_then(move |(_, eth_state2)| {
                                            debug!("[{}] Reading dr_tx_hash for id, try {}", dr_id, retries);
                                            retries += 1;

                                            eth_state2.wrb_contract
                                                .query(
                                                    "readDrHash",
                                                    (dr_id, ),
                                                    eth_account,
                                                    contract::Options::default(),
                                                    None,
                                                )
                                                .then(move |dr_tx_hash: Result<U256, _>| {
                                                    match dr_tx_hash {
                                                        Ok(dr_tx_hash) if dr_tx_hash == U256::zero() => {
                                                            warn!("readDrHash: returned null hash for data request id {}, retrying in {} ms", dr_id, interval_ms);
                                                            Either::A(futures::finished(()))
                                                        }
                                                        Ok(dr_tx_hash) => {
                                                            let dr_tx_hash = Hash::SHA256(dr_tx_hash.into());
                                                            info!(
                                                                "[{}] Data request included in witnet with dr_tx_hash: {}",
                                                                dr_id, dr_tx_hash
                                                            );
                                                            Either::B(eth_state2.wrb_requests.write().map(move |mut wrb_requests| {
                                                                wrb_requests.insert_included(dr_id, dr_tx_hash);
                                                            }).then(|_| {
                                                                // Exit interval loop
                                                                futures::failed(())
                                                            }))
                                                        }
                                                        Err(e) => {
                                                            warn!("readDrHash: {}, for data request id {}, retrying in {} ms", e, dr_id, interval_ms);
                                                            Either::A(futures::finished(()))
                                                        }
                                                    }
                                                })
                                        })
                                        .for_each(|_| {
                                            Ok(())
                                        })
                                )
                            }
                            Ok(WrbEvent::PostedResult(dr_id)) => {
                                info!("[{}] Data request has been resolved!", dr_id);

                                // TODO: actually get result?
                                let result = vec![];
                                Box::new(eth_state.wrb_requests.write().map(move |mut wrb_requests| {
                                    wrb_requests.insert_result(dr_id, result);
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
        });

    Either::B(fut)
}
