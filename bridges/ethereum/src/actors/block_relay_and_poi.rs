//! Actor which receives Witnet superblocks, posts them to the block relay,
//! and sends proofs of inclusion to Ethereum

use crate::{
    actors::{handle_receipt, WitnetSuperBlock},
    config::Config,
    eth::{get_gas_price, EthState},
};

use async_jsonrpc_client::{transports::tcp::TcpSocket, Transport};
use ethabi::Bytes;
use futures::{future::Either, sink::Sink, stream::Stream, sync::oneshot};
use serde_json::json;
use std::sync::Arc;
use tokio::sync::mpsc;
use web3::{
    contract,
    futures::Future,
    types::{H160, U256},
};
use witnet_data_structures::{
    chain::{Block, Hash, Hashable},
    transaction::{DRTransaction, TallyTransaction},
};

/// Function to get blocks from the witnet client provided an array of block hashes
pub fn get_blocks(
    confirmed_block_hashes: Vec<Hash>,
    witnet_client: Arc<TcpSocket>,
) -> impl Future<Item = Vec<Block>, Error = ()> {
    futures::stream::unfold(0, move |block_index| {
        if block_index >= confirmed_block_hashes.len() {
            None
        } else {
            Some(
                witnet_client
                    .execute("getBlock", json!([confirmed_block_hashes[block_index]]))
                    .map_err(|e| log::error!("getBlock: {:?}", e))
                    .and_then(move |block| {
                        futures::future::result(
                            serde_json::from_value(block)
                                .map_err(|e| {
                                    log::error!("Error while retrieving signature bytes {:?}", e)
                                })
                                .map(|block: Block| (block, block_index + 1)),
                        )
                    }),
            )
        }
    })
    .collect()
}

/// Actor which receives Witnet superblocks, posts them to the block relay,
/// and sends Proofs of Inclusion to Ethereum
pub fn block_relay_and_poi(
    config: Arc<Config>,
    eth_state: Arc<EthState>,
    wait_for_witnet_block_tx: mpsc::UnboundedSender<(U256, oneshot::Sender<()>)>,
    witnet_client: Arc<TcpSocket>,
) -> (
    mpsc::Sender<WitnetSuperBlock>,
    impl Future<Item = (), Error = ()>,
) {
    let (tx, rx) = mpsc::channel(16);

    let fut = rx.map_err(|e| log::error!("Failed to receive WitnetSuperBlock message: {:?}", e))
        .for_each(move |msg| {
            log::debug!("Got ActorMessage: {:?}", msg);
            let eth_account = config.eth_account;
            let wait_for_witnet_block_tx2 = wait_for_witnet_block_tx.clone();
            let enable_claim_and_inclusion = config.enable_claim_and_inclusion;
            let enable_result_reporting = config.enable_result_reporting;
            let block_relay_contract = eth_state.block_relay_contract.clone();

            let witnet_client = Arc::clone(&witnet_client);
            let (superblock_notification, is_new_block) = match msg {
                WitnetSuperBlock::New(superblock_notification) => (superblock_notification, true),
                WitnetSuperBlock::Replay(superblock_notification) => (superblock_notification, false),
            };

            let superblock = superblock_notification.superblock;
            let confirmed_block_hashes = superblock_notification.consolidated_block_hashes;

            let superblock_hash: U256 = match superblock.hash() {
                Hash::SHA256(x) => x.into(),
            };
            let superblock_epoch: U256 = superblock.index.into();
            let dr_merkle_root: U256 =
                match superblock.data_request_root {
                    Hash::SHA256(x) => x.into(),
                };
            let tally_merkle_root: U256 =
                match superblock.tally_root {
                    Hash::SHA256(x) => x.into(),
                };
            get_blocks(confirmed_block_hashes, witnet_client)
                .and_then({
                    let config = Arc::clone(&config);
                    let eth_state = Arc::clone(&eth_state);
                    move |confirmed_blocks| {
                        eth_state.wrb_requests.read()
                            .and_then({
                                move |wrb_requests| {
                                    let block_hash: U256 = match superblock.hash() {
                                        Hash::SHA256(x) => x.into(),
                                    };
                                    let dr_txs: Vec<DRTransaction> = confirmed_blocks.iter().flat_map(|block| {
                                        block.txns.data_request_txns.clone()
                                    }).collect();
                                    let tally_txs: Vec<TallyTransaction> = confirmed_blocks.iter().flat_map(|block| {
                                        block.txns.tally_txns.clone()
                                    }).collect();

                                    let block_epoch: U256 = superblock.index.into();

                                    let mut including = vec![];
                                    let mut resolving = vec![];

                                    let claimed_drs = wrb_requests.claimed();
                                    let waiting_for_tally = wrb_requests.included();

                                    if enable_claim_and_inclusion {
                                        for dr in &dr_txs {
                                            for dr_id in claimed_drs.get_by_right(&dr.body.dr_output.hash())
                                            {
                                                let dr_inclusion_proof = match superblock.dr_proof_of_inclusion(&confirmed_blocks, &dr) {
                                                    Some(x) => x,
                                                    None => {
                                                        log::error!("Error creating data request proof of inclusion");
                                                        continue;
                                                    }
                                                };

                                                let poi: Vec<U256> = dr_inclusion_proof
                                                    .lemma
                                                    .iter()
                                                    .map(|x| match x {
                                                        Hash::SHA256(x) => x.into(),
                                                    })
                                                    .collect();
                                                let poi_index = U256::from(dr_inclusion_proof.index);

                                                log::debug!(
                                                    "Proof of inclusion for data request {}:\nPoi: {:x?}\nPoi index: {}",
                                                    dr.hash(),
                                                    poi,
                                                    poi_index,
                                                );
                                                log::info!("[{}] Claimed dr got included in witnet block!", dr_id);
                                                log::info!("[{}] Sending proof of inclusion to WRB wrb_contract", dr_id);
                                                including.push((*dr_id, poi.clone(), poi_index, block_hash, block_epoch));
                                            }
                                        }
                                    }

                                    if enable_result_reporting {
                                        for tally in &tally_txs {
                                            for dr_id in waiting_for_tally.get_by_right(&tally.dr_pointer)
                                            {
                                                let Hash::SHA256(dr_pointer_bytes) = tally.dr_pointer;
                                                log::info!("[{}] Found tally for data request, posting to WRB", dr_id);
                                                let tally_inclusion_proof = match superblock.tally_proof_of_inclusion(&confirmed_blocks, &tally) {
                                                    Some(x) => x,
                                                    None => {
                                                        log::error!("Error creating tally data proof of inclusion");
                                                        continue;
                                                    }
                                                };
                                                log::debug!(
                                                    "Proof of inclusion for tally        {}:\nData: {:?}\n{:?}",
                                                    tally.hash(),
                                                    [&dr_pointer_bytes[..], &tally.tally].concat(),
                                                    tally_inclusion_proof
                                                );

                                                // Call report_result
                                                let poi: Vec<U256> = tally_inclusion_proof
                                                    .lemma
                                                    .iter()
                                                    .map(|x| match x {
                                                        Hash::SHA256(x) => x.into(),
                                                    })
                                                    .collect();
                                                let poi_index = U256::from(tally_inclusion_proof.index);
                                                let result: Bytes = tally.tally.clone();

                                                resolving.push((*dr_id, poi.clone(), poi_index, block_hash, block_epoch, result.clone()));
                                            }
                                        }
                                    }
                                    // If including or resolving is non empty, needs_relaying should be set to true
                                    let needs_relaying = if config.relay_all_superblocks_even_the_empty_ones || !including.is_empty() || !resolving.is_empty() {
                                        true
                                    } else {
                                        dr_txs.iter().any(|dr_tx| {
                                            wrb_requests.posted().values().any(|(address, dr_hash)| {
                                                if *address == H160::default() {
                                                    false
                                                } else {
                                                    dr_tx.body.dr_output.hash() == *dr_hash
                                                }
                                            })
                                        }) || tally_txs.iter().any(|tally_tx| {
                                            !waiting_for_tally.get_by_right(&tally_tx.dr_pointer).is_empty()
                                        })
                                    };
                                    futures::future::finished((including, resolving, needs_relaying))
                                }
                            })
                    }
                })
                .and_then({
                    let config = Arc::clone(&config);
                    let eth_state = Arc::clone(&eth_state);
                    move |(including, resolving, needs_relaying)| {
                        if (is_new_block && config.enable_block_relay_new_blocks) || (!is_new_block && config.enable_block_relay_old_blocks) {
                            // Optimization: do not process blocks that do not contain requests coming from ethereum
                            if including.is_empty() && resolving.is_empty() && !needs_relaying {
                                log::debug!("Skipping empty superblock");
                                return futures::finished(());
                            }
                            let last_id =  if let Some(&(id, ..)) = including.last() {
                                Some(id)
                            }
                            else if let Some(&(id, ..)) = resolving.last() {
                                Some(id)
                            }
                            else {
                                // At this point we know including and resolving are empty, but we need to relay the superblock
                                None
                            };

                            let block_relay_contract2 = block_relay_contract.clone();
                            // Post witnet superblock to BlockRelay wrb_contract
                            tokio::spawn(
                                block_relay_contract
                                    .query(
                                        "readDrMerkleRoot",
                                        (superblock_hash, ),
                                        eth_account,
                                        contract::Options::default(),
                                        None,
                                    )
                                    .map(move |_: U256| {
                                        log::debug!("Superblock {:x} was already posted", superblock_hash);
                                    })
                                    .or_else({
                                        let config = Arc::clone(&config);
                                        let eth_state = Arc::clone(&eth_state);
                                        move |_| {
                                            log::debug!("Trying to relay superblock {:x}", superblock_hash);
                                            // Use the same gas price as the last request from this block.
                                            // If this block does not contain any requests coming from ethereum, default to "None" which will estimate the gas price using the ethereum client.
                                            if let Some(last_id) = last_id {
                                                Either::A(get_gas_price(last_id, &config, &eth_state)
                                                    .map_err(move |e| {
                                                        log::warn!(
                                                            "[{}] Error in params reception while retrieving gas price:  {}",
                                                            last_id, e);
                                                    })
                                                    .map(move |gas_price: U256| {
                                                        Some(gas_price)
                                                    })
                                                )
                                            }
                                            else {
                                                Either::B(futures::future::finished(None))
                                            }.and_then({
                                                    let config = Arc::clone(&config);
                                                    move |gas_price| {
                                                        block_relay_contract2
                                                            .call_with_confirmations(
                                                                "postNewBlock",
                                                                (superblock_hash, superblock_epoch, dr_merkle_root, tally_merkle_root),
                                                                eth_account,
                                                                contract::Options::with(|opt| {
                                                                    opt.gas = config.gas_limits.post_new_block.map(Into::into);
                                                                    opt.gas_price = gas_price
                                                                }),
                                                                1,
                                                            )
                                                            .map_err(|e| log::error!("postNewBlock: {:?}", e))
                                                            .and_then(move |tx| {
                                                                log::debug!("postNewBlock: {:?}", tx);

                                                                handle_receipt(tx).map_err(move |()| {
                                                                    log::warn!("Failed to post superblock {:x} to block relay, maybe it was already posted?", superblock_hash)
                                                                })
                                                            })
                                                            .map(move |()| {
                                                                log::info!("Posted superblock {:x} to block relay", superblock_hash);
                                                            })
                                                    }
                                                })
                                        }
                                    })
                            );
                        }

                        // Wait for someone else to publish the witnet block to ethereum
                        let (wbtx, wbrx) = oneshot::channel();
                        let fut = if !including.is_empty() || !resolving.is_empty() {
                            Either::A(wait_for_witnet_block_tx2.send((superblock_hash, wbtx))
                                .map_err(|e| log::error!("Failed to send message to block_ticker channel: {}", e))
                                .and_then(move |_| {
                                    // Receiving the new block notification can fail if the block_ticker got
                                    // a different subscription to the same block hash.
                                    // In that case, since there already is another future waiting for the
                                    // same block, we can exit this one
                                    wbrx.map_err(move |e| {
                                        log::debug!("Failed to receive message through oneshot channel while waiting for superblock {}: {:x}", e, superblock_hash)
                                    })
                                })
                                .and_then({
                                    let config = Arc::clone(&config);
                                    let eth_state = Arc::clone(&eth_state);
                                    move |()| {
                                        eth_state.wrb_requests.write().map(move |mut wrb_requests| {
                                            for (dr_id, poi, poi_index, block_hash, block_epoch) in including {
                                                if wrb_requests.claimed().contains_left(&dr_id) {
                                                    wrb_requests.set_including(dr_id, poi.clone(), poi_index, block_hash, block_epoch);
                                                    let wrb_requests = eth_state.wrb_requests.clone();
                                                    let params_str = format!("{:?}", (dr_id, poi.clone(), poi_index, block_hash, block_epoch));
                                                    tokio::spawn(
                                                        get_gas_price(dr_id, &config, &eth_state)
                                                            .map_err(move |e| {
                                                                log::warn!(
                                                                    "[{}] Error in params reception while retrieving gas price: {}",
                                                                    dr_id, e);
                                                            })
                                                            .map(move |gas_price: U256| {
                                                                gas_price
                                                            }).and_then({
                                                            let config = Arc::clone(&config);
                                                            let wrb_contract = eth_state.wrb_contract.clone();
                                                            move |gas_price| {
                                                                wrb_contract
                                                                    .call_with_confirmations(
                                                                        "reportDataRequestInclusion",
                                                                        (dr_id, poi, poi_index, block_hash, block_epoch),
                                                                        eth_account,
                                                                        contract::Options::with(|opt| {
                                                                            opt.gas = config.gas_limits.report_data_request_inclusion.map(Into::into);
                                                                            opt.gas_price = Some(gas_price)
                                                                        }),
                                                                        1,
                                                                    )
                                                                    .then(move |tx| {
                                                                        match tx {
                                                                            Ok(tx) => {
                                                                                log::debug!("reportDataRequestInclusion: {:?}", tx);
                                                                                Either::A(handle_receipt(tx).map_err(|()| log::error!("handle_receipt: transaction failed")))
                                                                            }
                                                                            Err(e) => {
                                                                                log::error!("reportDataRequestInclusion{}: {:?}", params_str, e);
                                                                                Either::B(wrb_requests.write().map(move |mut wrb_requests| wrb_requests.undo_including(dr_id)))
                                                                            }
                                                                        }
                                                                    })
                                                            }
                                                        })
                                                    );
                                                }
                                            }
                                            for (dr_id, poi, poi_index, block_hash, block_epoch, result) in resolving {
                                                if wrb_requests.included().contains_left(&dr_id) {
                                                    wrb_requests.set_resolving(dr_id, poi.clone(), poi_index, block_hash, block_epoch, result.clone());
                                                    let wrb_requests = eth_state.wrb_requests.clone();
                                                    let params_str = format!("{:?}", &(dr_id, poi.clone(), poi_index, block_hash, block_epoch, result.clone()));
                                                    tokio::spawn(
                                                        get_gas_price(dr_id, &config, &eth_state)
                                                            .map_err(move |e| {
                                                                log::warn!(
                                                                    "[{}] Error in params reception while retrieving gas price: {}",
                                                                    dr_id, e);
                                                            })
                                                            .map(move |gas_price: U256| {
                                                                gas_price
                                                            }).and_then({
                                                            let config = Arc::clone(&config);
                                                            let eth_state = Arc::clone(&eth_state);
                                                            let wrb_contract = eth_state.wrb_contract.clone();
                                                            move |gas_price| {
                                                                wrb_contract
                                                                    .call_with_confirmations(
                                                                        "reportResult",
                                                                        (dr_id, poi, poi_index, block_hash, block_epoch, result),
                                                                        eth_account,
                                                                        contract::Options::with(|opt| {
                                                                            opt.gas = config.gas_limits.report_result.map(Into::into);
                                                                            opt.gas_price = Some(gas_price);
                                                                        }),
                                                                        1,
                                                                    )
                                                                    .then(move |tx| {
                                                                        match tx {
                                                                            Ok(tx) => {
                                                                                log::debug!("reportResult: {:?}", tx);
                                                                                Either::A(handle_receipt(tx).map_err(|()| log::error!("handle_receipt: transaction failed")))
                                                                            }
                                                                            Err(e) => {
                                                                                log::error!("reportResult{}: {:?}", params_str, e);
                                                                                Either::B(wrb_requests.write().map(move |mut wrb_requests| wrb_requests.undo_resolving(dr_id)))
                                                                            }
                                                                        }
                                                                    })
                                                            }
                                                        })
                                                    );
                                                }
                                            }
                                        })
                                    }
                                })
                            )
                        } else {
                            Either::B(futures::finished(()))
                        }
                            // Without this line the actor will panic on the first failure
                            .then(|_| Result::<(), ()>::Ok(()));

                        // Process multiple blocks in parallel
                        tokio::spawn(fut);
                        futures::done(Result::<(), ()>::Ok(()))
                    }
                })
        })
        .map(|_| ());

    (tx, fut)
}
