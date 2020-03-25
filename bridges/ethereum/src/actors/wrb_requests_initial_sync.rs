//! Read all the existing data requests from the WRB

use crate::{config::Config, eth::EthState};
use async_jsonrpc_client::futures::Stream;
use ethabi::Bytes;
use futures::future::Either;
use log::*;
use std::sync::Arc;
use web3::{contract, futures::Future, types::U256};
use witnet_data_structures::chain::Hash;

/// Read all the existing data requests from the WRB
pub fn wrb_requests_initial_sync(
    config: Arc<Config>,
    eth_state: Arc<EthState>,
) -> impl Future<Item = (), Error = ()> {
    // On startup, read all the existing data requests in the wrb in order to
    // build a local copy and use it for faster queries.
    // This should be able to run in parallel with the other actors.
    let wrb_contract = eth_state.wrb_contract.clone();
    wrb_contract
        .query(
            "requestsCount",
            (),
            config.eth_account,
            contract::Options::default(),
            None,
        )
        .map_err(|e| error!("requestsCount: {:?}", e))
        .and_then(move |num_requests: U256| {
            debug!("{} requests in WRB", num_requests);
            let eth_account = config.eth_account;
            futures::stream::unfold(U256::from(0), move |dr_id| {
                if dr_id >= num_requests {
                    None
                } else {
                    debug!("[{}] Getting request", dr_id);
                    let eth_state = eth_state.clone();
                    Some(
                        eth_state
                            .wrb_contract
                            .query(
                                "readResult",
                                (dr_id,),
                                eth_account,
                                contract::Options::default(),
                                None,
                            )
                            .map_err(|e| error!("readResult: {:?}", e))
                            .map(move |x: Bytes| (x, dr_id))
                            .and_then(move |(result, dr_id)| {
                                if !result.is_empty() {
                                    // In resolved state
                                    debug!("[{}] Request has already been resolved", dr_id);
                                    Either::A(eth_state.wrb_requests.write().map(
                                        move |mut wrb_requests| {
                                            wrb_requests.insert_result(dr_id, result);
                                        },
                                    ))
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
                                            .map_err(|e| error!("readResult: {:?}", e))
                                            .map(move |x: U256| (x, dr_id))
                                            .and_then(move |(dr_tx_hash, dr_id)| {
                                                if dr_tx_hash != U256::from(0) {
                                                    // In included state
                                                    debug!(
                                                        "[{}] Request has already been included",
                                                        dr_id
                                                    );
                                                    Either::A(eth_state.wrb_requests.write().map(
                                                        move |mut wrb_requests| {
                                                            let dr_tx_hash =
                                                                Hash::SHA256(dr_tx_hash.into());
                                                            wrb_requests
                                                                .insert_included(dr_id, dr_tx_hash);
                                                        },
                                                    ))
                                                } else {
                                                    debug!(
                                                        "[{}] Request has already been posted",
                                                        dr_id
                                                    );
                                                    // Not in included state, must be in posted state
                                                    Either::B(eth_state.wrb_requests.write().map(
                                                        move |mut wrb_requests| {
                                                            wrb_requests.insert_posted(dr_id);
                                                        },
                                                    ))
                                                }
                                            }),
                                    )
                                }
                            })
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
            info!("Initial WRB Requests synchronization finished!");
            Ok(())
        })
}
