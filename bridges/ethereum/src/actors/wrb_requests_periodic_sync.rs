//! Periodically check the state of the requests in the WRB

use crate::actors::ClaimMsg;
use crate::eth::DrState;
use crate::{config::Config, eth::EthState};
use async_jsonrpc_client::futures::Stream;
use ethabi::Bytes;
use futures::{future::Either, sink::Sink};
use std::sync::Arc;
use tokio::sync::mpsc;
use web3::{contract, futures::Future, types::U256};
use witnet_data_structures::chain::Hash;

/// Check for new requests in the WRB
pub fn get_new_requests(
    config: Arc<Config>,
    eth_state: Arc<EthState>,
    tx: mpsc::Sender<ClaimMsg>,
) -> impl Future<Item = (), Error = ()> {
    eth_state
        .wrb_contract
        .query(
            "requestsCount",
            (),
            config.eth_account,
            contract::Options::default(),
            None,
        )
        .map_err(|e| log::error!("requestsCount: {:?}", e))
        .and_then({
            let eth_state = Arc::clone(&eth_state);

            move |new_num_requests: U256| {
                eth_state
                    .wrb_requests
                    .read()
                    .map(|wrb_requests| wrb_requests.requests().len())
                    .map(move |old_num_requests| (U256::from(old_num_requests), new_num_requests))
            }
        })
        .and_then(move |(old_num_requests, new_num_requests)| {
            log::debug!(
                "{} new requests in WRB",
                new_num_requests - old_num_requests
            );
            futures::stream::unfold(old_num_requests, move |dr_id| {
                if dr_id >= new_num_requests {
                    None
                } else {
                    Some(
                        check_wrb_new_dr_state(
                            config.clone(),
                            eth_state.clone(),
                            tx.clone(),
                            dr_id,
                        )
                        .map(move |a| {
                            let b = dr_id + 1;
                            (a, b)
                        }),
                    )
                }
            })
            .then(|_| Ok(()))
            .for_each(|_| Ok(()))
        })
        .then(|_| {
            log::debug!("New WRB Requests synchronization finished");
            Ok(())
        })
}

/// Check the state of the existing requests in the WRB
pub fn wrb_requests_periodic_sync(
    config: Arc<Config>,
    eth_state: Arc<EthState>,
) -> impl Future<Item = (), Error = ()> {
    // On startup, read all the existing data requests in the wrb in order to
    // build a local copy and use it for faster queries.
    // This should be able to run in parallel with the other actors.
    eth_state
        .wrb_requests
        .read()
        .map(|wrb_requests| {
            let requests = wrb_requests.requests();
            let mut non_resolved_requests =
                Vec::with_capacity(requests.len() - wrb_requests.resolved().len());
            for (dr_id, dr_state) in requests {
                match dr_state {
                    DrState::Resolved { .. } => {
                        // Ignore resolved requests because their state cannot change anymore
                    }
                    _ => {
                        non_resolved_requests.push(*dr_id);
                    }
                }
            }
            non_resolved_requests.sort_unstable();

            non_resolved_requests
        })
        .and_then(move |non_resolved_requests| {
            log::debug!(
                "Checking the {} requests in non-resolved state",
                non_resolved_requests.len()
            );
            // This is a for loop using futures
            // for dr_id in non_resolved_requests
            futures::stream::unfold(0, move |index| {
                non_resolved_requests.get(index).copied().map(|dr_id| {
                    check_wrb_existing_dr_state(config.clone(), eth_state.clone(), dr_id).map(
                        move |a| {
                            let b = index + 1;
                            (a, b)
                        },
                    )
                })
            })
            .then(|_| Ok(()))
            .for_each(|_| Ok(()))
        })
        .then(|_| {
            log::debug!("Periodic WRB Requests synchronization finished");
            Ok(())
        })
}

fn check_wrb_new_dr_state(
    config: Arc<Config>,
    eth_state: Arc<EthState>,
    tx: mpsc::Sender<ClaimMsg>,
    dr_id: U256,
) -> impl Future<Item = (), Error = ()> {
    log::info!("[{}] Getting new request", dr_id);
    let eth_account = config.eth_account;
    eth_state
        .wrb_contract
        .query(
            "readResult",
            (dr_id,),
            eth_account,
            contract::Options::default(),
            None,
        )
        .map_err(|e| log::error!("readResult: {:?}", e))
        .map(move |x: Bytes| (x, dr_id))
        .and_then(move |(result, dr_id)| {
            if !result.is_empty() {
                // In resolved state
                log::info!("[{}] Request has been resolved", dr_id);
                Either::A(eth_state.wrb_requests.write().map(move |mut wrb_requests| {
                    wrb_requests.insert_result(dr_id, result);
                }))
            } else {
                // Not in Resolved state
                Either::B(
                    eth_state
                        .wrb_contract
                        .query(
                            "readDrHash",
                            (dr_id,),
                            eth_account,
                            contract::Options::default(),
                            None,
                        )
                        .map_err(|e| log::error!("readResult: {:?}", e))
                        .map(move |x: U256| (x, dr_id))
                        .and_then(move |(dr_tx_hash, dr_id)| {
                            if dr_tx_hash != U256::from(0) {
                                // In included state
                                log::info!("[{}] Request has been included", dr_id);
                                Either::A(eth_state.wrb_requests.write().map(
                                    move |mut wrb_requests| {
                                        let dr_tx_hash = Hash::SHA256(dr_tx_hash.into());
                                        wrb_requests.insert_included(dr_id, dr_tx_hash);
                                    },
                                ))
                            } else {
                                log::info!("[{}] Request has been posted", dr_id);
                                // Not in included state, must be in posted state
                                Either::B(eth_state.wrb_requests.write().and_then(
                                    move |mut wrb_requests| {
                                        wrb_requests.insert_posted(dr_id);

                                        tx.send(ClaimMsg::NewDr(dr_id))
                                            .map_err(|e| {
                                                log::error!(
                                                    "Failed to send ClaimMsg message: {}",
                                                    e
                                                )
                                            })
                                            .map(|_| ())
                                    },
                                ))
                            }
                        }),
                )
            }
        })
}

fn check_wrb_existing_dr_state(
    config: Arc<Config>,
    eth_state: Arc<EthState>,
    dr_id: U256,
) -> impl Future<Item = (), Error = ()> {
    log::debug!("[{}] Getting existing request", dr_id);
    let eth_account = config.eth_account;
    eth_state
        .wrb_contract
        .query(
            "readResult",
            (dr_id,),
            eth_account,
            contract::Options::default(),
            None,
        )
        .map_err(|e| log::error!("readResult: {:?}", e))
        .map(move |x: Bytes| (x, dr_id))
        .and_then(move |(result, dr_id)| {
            if !result.is_empty() {
                // In resolved state
                log::info!("[{}] Request has been resolved", dr_id);
                Either::A(eth_state.wrb_requests.write().map(move |mut wrb_requests| {
                    wrb_requests.insert_result(dr_id, result);
                }))
            } else {
                // Not in Resolved state
                Either::B(
                    eth_state
                        .wrb_contract
                        .query(
                            "readDrHash",
                            (dr_id,),
                            eth_account,
                            contract::Options::default(),
                            None,
                        )
                        .map_err(|e| log::error!("readResult: {:?}", e))
                        .map(move |x: U256| (x, dr_id))
                        .and_then(move |(dr_tx_hash, dr_id)| {
                            if dr_tx_hash != U256::from(0) {
                                // In included state
                                // Check if the data request was already in this state
                                Either::A(eth_state.wrb_requests.read().and_then(
                                    move |wrb_requests| {
                                        if wrb_requests.included().contains_left(&dr_id) {
                                            // Already included, nothing to do
                                            Either::B(futures::finished(()))
                                        } else {
                                            // New included: update state
                                            log::info!("[{}] Request has been included", dr_id);
                                            // Drop read lock
                                            std::mem::drop(wrb_requests);

                                            // Upgrade to a write lock and write state
                                            Either::A(eth_state.wrb_requests.write().map(
                                                move |mut wrb_requests| {
                                                    let dr_tx_hash =
                                                        Hash::SHA256(dr_tx_hash.into());
                                                    wrb_requests.insert_included(dr_id, dr_tx_hash);
                                                },
                                            ))
                                        }
                                    },
                                ))
                            } else {
                                // Not in included state, must be in posted state
                                // Nothing to do here, new data requests are handled
                                // by get_new_requests
                                Either::B(futures::finished(()))
                            }
                        }),
                )
            }
        })
}
