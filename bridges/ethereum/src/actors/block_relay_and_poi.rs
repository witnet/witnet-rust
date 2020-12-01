//! Actor which receives Witnet superblocks, posts them to the block relay,
//! and sends proofs of inclusion to Ethereum

use crate::{actors::handle_receipt, actors::WitnetSuperBlock, config::Config, eth::EthState};

use async_jsonrpc_client::{transports::tcp::TcpSocket, Transport};
use ethabi::Bytes;
use futures::{future::Either, sink::Sink, stream::Stream, sync::oneshot};
use serde_json::json;
use std::sync::Arc;
use tokio::sync::mpsc;
use web3::{contract, futures::Future, types::U256};
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
            let wrb_contract = eth_state.wrb_contract.clone();
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
                                    futures::future::finished((including, resolving))
                                }
                            })
                    }
                })
                .and_then({
                    let config = Arc::clone(&config);
                    let eth_state = Arc::clone(&eth_state);
                    move |(including, resolving)| {

                        // Optimization: do not process blocks that do not contain requests coming from ethereum
                        if including.is_empty() && resolving.is_empty() {
                            log::debug!("Skipping empty superblock");
                            return futures::finished(());
                        }

                        if (is_new_block && config.enable_block_relay_new_blocks) || (!is_new_block && config.enable_block_relay_old_blocks) {

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

                                        move |_| {
                                            log::debug!("Trying to relay superblock {:x}", superblock_hash);
                                            block_relay_contract2
                                                .call_with_confirmations(
                                                    "postNewBlock",
                                                    (superblock_hash, superblock_epoch, dr_merkle_root, tally_merkle_root),
                                                    eth_account,
                                                    contract::Options::with(|opt| {
                                                        opt.gas = config.gas_limits.post_new_block.map(Into::into);
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
                            );
                        }

                        // Wait for someone else to publish the witnet block to ethereum
                        let (wbtx, wbrx) = oneshot::channel();
                        let fut = wait_for_witnet_block_tx2.send((superblock_hash, wbtx))
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
                                    // Check if we need to acquire a write lock
                                    if !including.is_empty() || !resolving.is_empty() {
                                        Either::A(eth_state.wrb_requests.write().map(move |mut wrb_requests| {
                                            for (dr_id, poi, poi_index, block_hash, block_epoch) in including {
                                                if wrb_requests.claimed().contains_left(&dr_id) {
                                                    wrb_requests.set_including(dr_id, poi.clone(), poi_index, block_hash, block_epoch);
                                                    let wrb_requests = eth_state.wrb_requests.clone();
                                                    let params_str = format!("{:?}", (dr_id, poi.clone(), poi_index, block_hash, block_epoch));
                                                    tokio::spawn(
                                                        wrb_contract
                                                            .call_with_confirmations(
                                                                "reportDataRequestInclusion",
                                                                (dr_id, poi, poi_index, block_hash, block_epoch),
                                                                eth_account,
                                                                contract::Options::with(|opt| {
                                                                    opt.gas = config.gas_limits.report_data_request_inclusion.map(Into::into);
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
                                                            }),
                                                    );
                                                }
                                            }
                                            for (dr_id, poi, poi_index, block_hash, block_epoch, result) in resolving {
                                                if wrb_requests.included().contains_left(&dr_id) {
                                                    wrb_requests.set_resolving(dr_id, poi.clone(), poi_index, block_hash, block_epoch, result.clone());
                                                    let wrb_requests = eth_state.wrb_requests.clone();
                                                    let params_str = format!("{:?}", &(dr_id, poi.clone(), poi_index, block_hash, block_epoch, result.clone()));
                                                    tokio::spawn(
                                                        wrb_contract
                                                            .call_with_confirmations(
                                                                "reportResult",
                                                                (dr_id, poi, poi_index, block_hash, block_epoch, result),
                                                                eth_account,
                                                                contract::Options::with(|opt| {
                                                                    opt.gas = config.gas_limits.report_result.map(Into::into);
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
                                                            }),
                                                    );
                                                }
                                            }
                                        }))
                                    } else {
                                        Either::B(futures::finished(()))
                                    }
                                }
                            })
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
