use std::{
    collections::HashMap,
    convert::TryFrom,
    net::SocketAddr,
    sync::atomic::{AtomicUsize, Ordering},
    sync::Arc,
};

use actix::MailboxError;
#[cfg(not(test))]
use actix::SystemService;
use itertools::Itertools;
use jsonrpc_core::{MetaIoHandler, Params, Value};
use jsonrpc_pubsub::{PubSubHandler, Session, Subscriber, SubscriptionId};
use serde::{Deserialize, Serialize};

use witnet_crypto::key::KeyPath;
use witnet_data_structures::{
    chain::{
        Block, DataRequestOutput, Epoch, Hash, Hashable, PublicKeyHash, RADType, StateMachine,
        SyncStatus,
    },
    mainnet_validations::ActiveWips,
    transaction::Transaction,
    vrf::VrfMessage,
};

use crate::{
    actors::{
        chain_manager::{run_dr_locally, ChainManager, ChainManagerError},
        epoch_manager::{EpochManager, EpochManagerError},
        inventory_manager::{InventoryManager, InventoryManagerError},
        messages::{
            AddCandidates, AddPeers, AddTransaction, BuildDrt, BuildVtt, ClearPeers, DropAllPeers,
            GetBalance, GetBlocksEpochRange, GetConsolidatedPeers, GetDataRequestInfo, GetEpoch,
            GetHighestCheckpointBeacon, GetItemBlock, GetItemSuperblock, GetItemTransaction,
            GetKnownPeers, GetMemoryTransaction, GetMempool, GetNodeStats, GetReputation,
            GetSignalingInfo, GetState, GetUtxoInfo, InitializePeers, IsConfirmedBlock, Rewind,
        },
        peers_manager::PeersManager,
        sessions_manager::SessionsManager,
    },
    config_mngr, signature_mngr,
};

use super::Subscriptions;

#[cfg(test)]
use self::mock_actix::SystemService;
use crate::actors::messages::GetSupplyInfo;
use futures::FutureExt;
use futures_util::compat::Compat;
use std::future::Future;

type JsonRpcResult = Result<Value, jsonrpc_core::Error>;

/// Define the JSON-RPC interface:
/// All the methods available through JSON-RPC
pub fn jsonrpc_io_handler(
    subscriptions: Subscriptions,
    enable_sensitive_methods: bool,
) -> PubSubHandler<Arc<Session>> {
    let mut io = PubSubHandler::new(MetaIoHandler::default());

    io.add_method("inventory", |params: Params| {
        Compat::new(Box::pin(inventory(params.parse())))
    });
    io.add_method("getBlockChain", |params: Params| {
        Compat::new(Box::pin(get_block_chain(params.parse())))
    });
    io.add_method("getBlock", |params: Params| {
        Compat::new(Box::pin(get_block(params)))
    });
    io.add_method("getTransaction", |params: Params| {
        Compat::new(Box::pin(get_transaction(params.parse())))
    });
    //io.add_method("getOutput", |params: Params| get_output(params.parse()));
    io.add_method("syncStatus", |_params: Params| {
        Compat::new(Box::pin(status()))
    });
    io.add_method("dataRequestReport", |params: Params| {
        Compat::new(Box::pin(data_request_report(params.parse())))
    });
    io.add_method("getBalance", |params: Params| {
        Compat::new(Box::pin(get_balance(params)))
    });
    io.add_method("getReputation", |params: Params| {
        Compat::new(Box::pin(get_reputation(params.parse(), false)))
    });
    io.add_method("getReputationAll", |_params: Params| {
        Compat::new(Box::pin(get_reputation(
            Ok((PublicKeyHash::default(),)),
            true,
        )))
    });
    io.add_method("getSupplyInfo", |_params: Params| {
        Compat::new(Box::pin(get_supply_info()))
    });
    io.add_method("peers", |_params: Params| Compat::new(Box::pin(peers())));
    io.add_method("knownPeers", |_params: Params| {
        Compat::new(Box::pin(known_peers()))
    });
    io.add_method("nodeStats", |_params: Params| {
        Compat::new(Box::pin(node_stats()))
    });
    io.add_method("getMempool", |params: Params| {
        Compat::new(Box::pin(get_mempool(params.parse())))
    });
    io.add_method("getConsensusConstants", |params: Params| {
        Compat::new(Box::pin(get_consensus_constants(params.parse())))
    });
    io.add_method("getSuperblock", |params: Params| {
        Compat::new(Box::pin(get_superblock(params.parse())))
    });
    io.add_method("signalingInfo", |params: Params| {
        Compat::new(Box::pin(signaling_info(params.parse())))
    });

    // Enable methods that assume that JSON-RPC is only accessible by the owner of the node.
    // A method is sensitive if it touches in some way the master key of the node.
    // For example: methods that can be used to create transactions (spending value from this node),
    // sign arbitrary messages with the node master key, and even export the master key.
    async fn call_if_authorized<F, Fut>(
        enable_sensitive_methods: bool,
        method_name: &str,
        params: Params,
        method: F,
    ) -> JsonRpcResult
    where
        F: FnOnce(Params) -> Fut,
        Fut: Future<Output = JsonRpcResult>,
    {
        if enable_sensitive_methods {
            method(params).await
        } else {
            Err(internal_error_s(unauthorized_message(method_name)))
        }
    }

    io.add_method("sendRequest", move |params| {
        Compat::new(Box::pin(call_if_authorized(
            enable_sensitive_methods,
            "sendRequest",
            params,
            |params| send_request(params.parse()),
        )))
    });
    io.add_method("tryRequest", move |params| {
        Compat::new(Box::pin(call_if_authorized(
            enable_sensitive_methods,
            "tryRequest",
            params,
            |params| try_request(params.parse()),
        )))
    });
    io.add_method("sendValue", move |params| {
        Compat::new(Box::pin(call_if_authorized(
            enable_sensitive_methods,
            "sendValue",
            params,
            |params| send_value(params.parse()),
        )))
    });
    io.add_method("getPublicKey", move |params| {
        Compat::new(Box::pin(call_if_authorized(
            enable_sensitive_methods,
            "getPublicKey",
            params,
            |_params| get_public_key(),
        )))
    });
    io.add_method("getPkh", move |params| {
        Compat::new(Box::pin(call_if_authorized(
            enable_sensitive_methods,
            "getPkh",
            params,
            |_params| get_pkh(),
        )))
    });
    io.add_method("getUtxoInfo", move |params| {
        Compat::new(Box::pin(call_if_authorized(
            enable_sensitive_methods,
            "getUtxoInfo",
            params,
            |params| get_utxo_info(params.parse()),
        )))
    });
    io.add_method("sign", move |params| {
        Compat::new(Box::pin(call_if_authorized(
            enable_sensitive_methods,
            "sign",
            params,
            |params| sign_data(params.parse()),
        )))
    });
    io.add_method("createVRF", move |params| {
        Compat::new(Box::pin(call_if_authorized(
            enable_sensitive_methods,
            "createVRF",
            params,
            |params| create_vrf(params.parse()),
        )))
    });
    io.add_method("masterKeyExport", move |params| {
        Compat::new(Box::pin(call_if_authorized(
            enable_sensitive_methods,
            "masterKeyExport",
            params,
            |_params| master_key_export(),
        )))
    });
    io.add_method("addPeers", move |params| {
        Compat::new(Box::pin(call_if_authorized(
            enable_sensitive_methods,
            "addPeers",
            params,
            |params| add_peers(params.parse()),
        )))
    });
    io.add_method("clearPeers", move |params| {
        Compat::new(Box::pin(call_if_authorized(
            enable_sensitive_methods,
            "clearPeers",
            params,
            |_params| clear_peers(),
        )))
    });
    io.add_method("initializePeers", move |params| {
        Compat::new(Box::pin(call_if_authorized(
            enable_sensitive_methods,
            "initializePeers",
            params,
            |_params| initialize_peers(),
        )))
    });
    io.add_method("rewind", move |params| {
        Compat::new(Box::pin(call_if_authorized(
            enable_sensitive_methods,
            "rewind",
            params,
            |params| rewind(params.parse()),
        )))
    });

    // Enable subscriptions
    // We need two Arcs, one for subscribe and one for unsuscribe
    let ss = subscriptions.clone();
    let ssu = subscriptions.clone();
    let atomic_counter = AtomicUsize::new(1);
    io.add_subscription(
        "witnet_subscription",
        (
            "witnet_subscribe",
            move |params: Params, _meta: Arc<Session>, subscriber: Subscriber| {
                log::debug!("Called witnet_subscribe");
                let (method_name, method_params) = match params {
                    Params::Array(v) => {
                        // [method_name, method_params] = v
                        // Use an iterator because pattern matching on vectors is not possible
                        let mut iter = v.into_iter();
                        match (iter.next(), iter.next(), iter.next()) {
                            // Only one element in vector: set params to Value::Null
                            (Some(method_name), None, None) => (method_name, Value::Null),
                            // Two elements in vector: method_name and params
                            (Some(method_name), Some(method_params), None) => {
                                (method_name, method_params)
                            }
                            // Otherwise, return an error
                            _ => {
                                // Ignore errors with `.ok()` because an error here means the connection was closed
                                subscriber
                                    .reject(jsonrpc_core::Error::invalid_params(
                                        "Expected array with 1 or 2 elements",
                                    ))
                                    .ok();
                                return;
                            }
                        }
                    }
                    _ => {
                        // Ignore errors with `.ok()` because an error here means the connection was closed
                        subscriber
                            .reject(jsonrpc_core::Error::invalid_params("Expected array"))
                            .ok();
                        return;
                    }
                };

                let method_name: String = match serde_json::from_value(method_name) {
                    Ok(s) => s,
                    Err(e) => {
                        // Ignore errors with `.ok()` because an error here means the connection was closed
                        subscriber
                            .reject(jsonrpc_core::Error::invalid_params(e.to_string()))
                            .ok();
                        return;
                    }
                };

                let add_subscription = |method_name, subscriber: Subscriber| {
                    if let Ok(mut s) = ss.lock() {
                        let id = SubscriptionId::String(
                            atomic_counter.fetch_add(1, Ordering::SeqCst).to_string(),
                        );
                        if let Ok(sink) = subscriber.assign_id(id.clone()) {
                            let v = s.entry(method_name).or_insert_with(HashMap::new);
                            v.insert(id, (sink, method_params));
                            log::debug!(
                                "Subscribed to {}. There are {} subscriptions to this method",
                                method_name,
                                v.len()
                            );
                        } else {
                            // Session closed before we got a chance to reply
                            log::debug!("Failed to assign id: session closed");
                        }
                    } else {
                        log::error!("Failed to acquire lock in add_subscription");
                        subscriber
                            .reject(internal_error_s("Failed to acquire lock"))
                            .ok();
                    }
                };

                match method_name.as_str() {
                    "blocks" => {
                        add_subscription("blocks", subscriber);
                    }
                    "superblocks" => {
                        add_subscription("superblocks", subscriber);
                    }
                    "status" => {
                        add_subscription("status", subscriber);
                    }
                    e => {
                        log::debug!("Unknown subscription method: {}", e);
                        // Ignore errors with `.ok()` because an error here means the connection was closed
                        subscriber
                            .reject(jsonrpc_core::Error::invalid_params(format!(
                                "Unknown subscription method: {}",
                                e
                            )))
                            .ok();
                    }
                }
            },
        ),
        (
            "witnet_unsubscribe",
            move |id: SubscriptionId,
                  _meta: Option<Arc<Session>>|
                  -> jsonrpc_core::BoxFuture<Value> {
                // If meta is None it means that the session is now closed
                // If meta is Some it means that the client called witnet_unsubscribe for this id,
                // but the session is still open
                log::debug!("Closing subscription {:?}", id);
                match ssu.lock() {
                    Ok(mut s) => {
                        let mut found = false;
                        for (_method, v) in s.iter_mut() {
                            if v.remove(&id).is_some() {
                                found = true;
                                // Each id can only appear once
                                break;
                            }
                        }

                        Box::new(jsonrpc_core::futures::future::ok(Value::Bool(found)))
                    }
                    Err(e) => {
                        log::error!("Failed to acquire lock in witnet_unsubscribe");
                        Box::new(jsonrpc_core::futures::future::err(internal_error(e)))
                    }
                }
            },
        ),
    );

    io
}

fn internal_error<T: std::fmt::Debug>(e: T) -> jsonrpc_core::Error {
    jsonrpc_core::Error {
        code: jsonrpc_core::ErrorCode::InternalError,
        message: format!("{:?}", e),
        data: None,
    }
}

fn internal_error_s<T: std::fmt::Display>(e: T) -> jsonrpc_core::Error {
    jsonrpc_core::Error {
        code: jsonrpc_core::ErrorCode::InternalError,
        message: format!("{}", e),
        data: None,
    }
}

/// Message that appears when calling a sensitive method when sensitive methods are disabled
fn unauthorized_message(method_name: &str) -> String {
    format!("Method {} not allowed while node setting json_rpc.enable_sensitive_methods is set to false", method_name)
}

/// Inventory element: block, transaction, etc
#[derive(Debug, Eq, PartialEq, Clone, Serialize, Deserialize)]
// TODO Remove Clippy allow
#[allow(clippy::large_enum_variant)]
pub enum InventoryItem {
    /// Error
    #[serde(rename = "error")]
    Error,
    /// Transaction
    #[serde(rename = "transaction")]
    Transaction(Transaction),
    /// Block
    #[serde(rename = "block")]
    Block(Block),
}

/// Make the node process, validate and potentially broadcast a new inventory entry.
///
/// Input: the JSON serialization of a well-formed inventory entry
///
/// Returns a boolean indicating success.
/* Test string:
{"jsonrpc": "2.0","method": "inventory","params": {"block": {"block_header":{"version":1,"beacon":{"checkpoint":2,"hash_prev_block": {"SHA256": [4,4,4,4,4,4,4,4,4,4,4,4,4,4,4,4,4,4,4,4,4,4,4,4,4,4,4,4,4,4,4,4]}},"hash_merkle_root":{"SHA256":[3,3,3,3,3,3,3,3,3,3,3,3,3,3,3,3,3,3,3,3,3,3,3,3,3,3,3,3,3,3,3,3]}},"proof":{"block_sig": null}"txns":[null]}},"id": 1}
*/
pub async fn inventory(params: Result<InventoryItem, jsonrpc_core::Error>) -> JsonRpcResult {
    let inv_elem = match params {
        Ok(x) => x,
        Err(e) => return Err(e),
    };

    match inv_elem {
        InventoryItem::Block(block) => {
            log::debug!("Got block from JSON-RPC. Sending AnnounceItems message.");

            let chain_manager_addr = ChainManager::from_registry();
            let res = chain_manager_addr
                .send(AddCandidates {
                    blocks: vec![block],
                })
                .await;

            res.map_err(internal_error).map(|()| Value::Bool(true))
        }

        InventoryItem::Transaction(transaction) => {
            log::debug!("Got transaction from JSON-RPC. Sending AnnounceItems message.");

            let chain_manager_addr = ChainManager::from_registry();
            chain_manager_addr
                .send(AddTransaction {
                    transaction,
                    broadcast_flag: true,
                })
                .await
                .map_err(internal_error)
                .and_then(|res| match res {
                    Ok(()) => Ok(Value::Bool(true)),
                    Err(e) => Err(internal_error_s(e)),
                })
        }

        inv_elem => {
            log::debug!(
                "Invalid type of inventory item from JSON-RPC: {:?}",
                inv_elem
            );
            Err(jsonrpc_core::Error::invalid_params(
                "Item type not implemented",
            ))
        }
    }
}

/// Params of getBlockChain method
#[derive(Debug, Default, Deserialize, Serialize)]
pub struct GetBlockChainParams {
    /// First epoch for which to return block hashes.
    /// If negative, return block hashes from the last n epochs.
    #[serde(default)] // default to 0
    pub epoch: i64,
    /// Number of block hashes to return.
    /// If negative, return the last n block hashes from this epoch range.
    /// If zero, unlimited.
    #[serde(default)] // default to 0
    pub limit: i64,
}

/// Get the list of all the known block hashes.
///
/// Returns a list of `(epoch, block_hash)` pairs.
/* test
{"jsonrpc": "2.0","method": "getBlockChain", "id": 1}
*/
pub async fn get_block_chain(
    params: Result<Option<GetBlockChainParams>, jsonrpc_core::Error>,
) -> JsonRpcResult {
    // Helper function to convert the result of GetBlockEpochRange to a JSON value, or a JSON-RPC error
    async fn process_get_block_chain(
        res: Result<Result<Vec<(u32, Hash)>, ChainManagerError>, MailboxError>,
    ) -> JsonRpcResult {
        match res {
            Ok(Ok(vec_inv_entry)) => {
                let epoch_and_hash: Vec<_> = vec_inv_entry
                    .into_iter()
                    .map(|(epoch, hash)| {
                        let hash_string = format!("{}", hash);
                        (epoch, hash_string)
                    })
                    .collect();
                let value = match serde_json::to_value(epoch_and_hash) {
                    Ok(x) => x,
                    Err(e) => {
                        let err = internal_error(e);
                        return Err(err);
                    }
                };
                Ok(value)
            }
            Ok(Err(e)) => {
                let err = internal_error(e);
                Err(err)
            }
            Err(e) => {
                let err = internal_error(e);
                Err(err)
            }
        }
    }

    fn epoch_range(start_epoch: u32, limit: u32, limit_negative: bool) -> GetBlocksEpochRange {
        if limit_negative {
            GetBlocksEpochRange::new_with_limit_from_end(start_epoch.., limit as usize)
        } else {
            GetBlocksEpochRange::new_with_limit(start_epoch.., limit as usize)
        }
    }

    fn convert_negative_to_positive_with_negative_flag(x: i64) -> Result<(u32, bool), String> {
        let positive_x = u32::try_from(x.unsigned_abs()).map_err(|_e| {
            format!(
                "out of bounds: {} must be between -{} and {} inclusive",
                x,
                u32::max_value(),
                u32::max_value()
            )
        })?;

        Ok((positive_x, x.is_negative()))
    }

    let GetBlockChainParams { epoch, limit } = match params {
        Ok(x) => x.unwrap_or_default(),
        Err(e) => return Err(e),
    };

    let (epoch, epoch_negative) = match convert_negative_to_positive_with_negative_flag(epoch) {
        Ok(x) => x,
        Err(mut err_str) => {
            err_str.insert_str(0, "Epoch ");
            return Err(internal_error_s(err_str));
        }
    };

    let (limit, limit_negative) = match convert_negative_to_positive_with_negative_flag(limit) {
        Ok(x) => x,
        Err(mut err_str) => {
            err_str.insert_str(0, "Limit ");
            return Err(internal_error_s(err_str));
        }
    };

    let chain_manager_addr = ChainManager::from_registry();
    if epoch_negative {
        // On negative epoch, get blocks from last n epochs
        // But, what is the current epoch?
        let res = EpochManager::from_registry().send(GetEpoch).await;

        match res {
            Ok(Ok(current_epoch)) => {
                let epoch = current_epoch.saturating_sub(epoch);

                let res = chain_manager_addr
                    .send(epoch_range(epoch, limit, limit_negative))
                    .await;

                process_get_block_chain(res).await
            }
            Ok(Err(e)) => {
                let err = internal_error(e);
                Err(err)
            }
            Err(e) => {
                let err = internal_error(e);
                Err(err)
            }
        }
    } else {
        let res = chain_manager_addr
            .send(epoch_range(epoch, limit, limit_negative))
            .await;
        process_get_block_chain(res).await
    }
}

/// Get block by hash
///
/// - First argument is the hash of the block that we are querying.
/// - Second argument is whether we want the response to contain a list of hashes of the
///   transactions found in the block.
/* test
{"jsonrpc":"2.0","id":1,"method":"getBlock","params":["c0002c6b25615c0f71069f159dffddf8a0b3e529efb054402f0649e969715bdb", false]}
*/
pub async fn get_block(params: Params) -> Result<Value, jsonrpc_core::Error> {
    let (block_hash, include_txns_hashes): (Hash, bool);

    // Handle parameters as an array with a first obligatory hash field plus an optional bool field
    if let Params::Array(params) = params {
        if let Some(Value::String(hash)) = params.get(0) {
            match hash.parse() {
                Ok(hash) => block_hash = hash,
                Err(e) => {
                    return Err(internal_error(e));
                }
            }
        } else {
            return Err(jsonrpc_core::Error::invalid_params(
                "First argument of `get_block` must have type `Hash`",
            ));
        };

        match params.get(1) {
            None => include_txns_hashes = true,
            Some(Value::Bool(ith)) => include_txns_hashes = *ith,
            Some(_) => {
                return Err(jsonrpc_core::Error::invalid_params(
                    "Second argument of `get_block` must have type `Bool`",
                ))
            }
        };
    } else {
        return Err(jsonrpc_core::Error::invalid_params(
            "Params of `get_block` method must have type `Array`",
        ));
    };

    let inventory_manager = InventoryManager::from_registry();

    let res = inventory_manager
        .send(GetItemBlock { hash: block_hash })
        .await;

    match res {
        Ok(Ok(mut output)) => {
            let block_epoch = output.block_header.beacon.checkpoint;
            let block_hash = output.hash();

            // Only include the `txns_hashes` field if explicitly requested, as hash
            // operations are quite expensive, and transactions read from storage cannot
            // benefit from hash memoization
            let txns_hashes = if include_txns_hashes {
                let vtt_hashes: Vec<_> = output
                    .txns
                    .value_transfer_txns
                    .iter()
                    .map(|txn| txn.hash())
                    .collect();
                let drt_hashes: Vec<_> = output
                    .txns
                    .data_request_txns
                    .iter()
                    .map(|txn| txn.hash())
                    .collect();
                let ct_hashes: Vec<_> = output
                    .txns
                    .commit_txns
                    .iter()
                    .map(|txn| txn.hash())
                    .collect();
                let rt_hashes: Vec<_> = output
                    .txns
                    .reveal_txns
                    .iter()
                    .map(|txn| txn.hash())
                    .collect();
                let tt_hashes: Vec<_> = output
                    .txns
                    .tally_txns
                    .iter()
                    .map(|txn| txn.hash())
                    .collect();

                Some(serde_json::json!({
                    "mint" : output.txns.mint.hash(),
                    "value_transfer" : vtt_hashes,
                    "data_request" : drt_hashes,
                    "commit" : ct_hashes,
                    "reveal" : rt_hashes,
                    "tally" : tt_hashes
                }))
            } else {
                None
            };

            // Check if there were data request transactions included in the block which require RADType replacement
            // It is imperative to apply this after the transaction hash calculation to conserve the correct hashes
            if !output.txns.data_request_txns.is_empty() {
                // Create Active WIPs
                let signaling_info = ChainManager::from_registry()
                    .send(GetSignalingInfo {})
                    .await;
                let active_wips = match signaling_info {
                    Ok(Ok(wips)) => ActiveWips {
                        active_wips: wips.active_upgrades,
                        block_epoch,
                    },
                    Ok(Err(e)) => {
                        let err = internal_error(e);
                        return Err(err);
                    }
                    Err(e) => {
                        let err = internal_error(e);
                        return Err(err);
                    }
                };

                if !active_wips.wip0019() {
                    // Replace RADType::Unknown with RADType::HttpGet for all epochs before the activation of WIP0019
                    for dr_txn in &mut output.txns.data_request_txns {
                        for retrieve in &mut dr_txn.body.dr_output.data_request.retrieve {
                            retrieve.kind = RADType::HttpGet
                        }
                    }
                }
            }

            let mut value = match serde_json::to_value(output) {
                Ok(x) => x,
                Err(e) => {
                    let err = internal_error(e);
                    return Err(err);
                }
            };

            // See explanation above about optional `txns_hashes` field
            if let Some(txns_hashes) = txns_hashes {
                value
                    .as_object_mut()
                    .expect("The result of getBlock should be an object")
                    .insert("txns_hashes".to_string(), txns_hashes);
            }

            // Check if this block is confirmed by a majority of superblock votes
            let chain_manager = ChainManager::from_registry();
            let res = chain_manager
                .send(IsConfirmedBlock {
                    block_hash,
                    block_epoch,
                })
                .await;

            match res {
                Ok(Ok(confirmed)) => {
                    // Append {"confirmed":true} to the result
                    value
                        .as_object_mut()
                        .expect("The result of getBlock should be an object")
                        .insert("confirmed".to_string(), Value::Bool(confirmed));

                    Ok(value)
                }
                Ok(Err(e)) => {
                    let err = internal_error(e);
                    Err(err)
                }
                Err(e) => {
                    let err = internal_error(e);
                    Err(err)
                }
            }
        }
        Ok(Err(e)) => {
            let err = internal_error(e);
            Err(err)
        }
        Err(e) => {
            let err = internal_error(e);
            Err(err)
        }
    }
}

/// Format of the output of getTransaction
#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GetTransactionOutput {
    /// Transaction
    pub transaction: Transaction,
    /// Weight of the transaction
    pub weight: u32,
    /// Hash of the block that contains this transaction in hex format,
    /// or "pending" if the transaction has not been included in any block yet
    pub block_hash: String,
    /// Epoch of the block that contains this transaction, or None if the transaction has not been
    /// included in any block yet
    pub block_epoch: Option<Epoch>,
    /// True if the block that includes this transaction has been confirmed by a superblock
    pub confirmed: bool,
}

/// Get transaction by hash
pub async fn get_transaction(hash: Result<(Hash,), jsonrpc_core::Error>) -> JsonRpcResult {
    let hash = match hash {
        Ok(x) => x.0,
        Err(e) => return Err(e),
    };

    let inventory_manager = InventoryManager::from_registry();

    let res = inventory_manager.send(GetItemTransaction { hash }).await;

    match res {
        Ok(Ok((transaction, pointer_to_block, block_epoch))) => {
            let weight = transaction.weight();
            let block_hash = pointer_to_block.block_hash;
            // Check if this block is confirmed by a majority of superblock votes
            let chain_manager = ChainManager::from_registry();
            let confirmed = match chain_manager
                .send(IsConfirmedBlock {
                    block_hash,
                    block_epoch,
                })
                .await
            {
                Ok(Ok(x)) => x,
                Ok(Err(e)) => {
                    return Err(internal_error(e));
                }
                Err(e) => {
                    return Err(internal_error(e));
                }
            };

            let new_transaction = match transaction {
                Transaction::DataRequest(mut dr_txn) => {
                    // Create Active WIPs
                    let signaling_info = ChainManager::from_registry()
                        .send(GetSignalingInfo {})
                        .await;
                    let active_wips = match signaling_info {
                        Ok(Ok(wips)) => ActiveWips {
                            active_wips: wips.active_upgrades,
                            block_epoch,
                        },
                        Ok(Err(e)) => {
                            let err = internal_error(e);
                            return Err(err);
                        }
                        Err(e) => {
                            let err = internal_error(e);
                            return Err(err);
                        }
                    };

                    if !active_wips.wip0019() {
                        // Replace RADType::Unknown with RADType::HttpGet for all epochs before the activation of WIP0019
                        for retrieve in &mut dr_txn.body.dr_output.data_request.retrieve {
                            retrieve.kind = RADType::HttpGet
                        }
                    }

                    Transaction::DataRequest(dr_txn)
                }
                _ => transaction,
            };

            let output = GetTransactionOutput {
                transaction: new_transaction,
                weight,
                block_hash: block_hash.to_string(),
                block_epoch: Some(block_epoch),
                confirmed,
            };
            let value = match serde_json::to_value(output) {
                Ok(x) => x,
                Err(e) => {
                    let err = internal_error(e);
                    let fut: JsonRpcResult = Err(err);
                    return fut;
                }
            };
            let fut: JsonRpcResult = Ok(value);
            fut
        }
        Ok(Err(InventoryManagerError::ItemNotFound)) => {
            let chain_manager = ChainManager::from_registry();
            let res = chain_manager.send(GetMemoryTransaction { hash }).await;

            match res {
                Ok(Ok(transaction)) => {
                    let weight = transaction.weight();
                    let output = GetTransactionOutput {
                        transaction,
                        weight,
                        block_hash: "pending".to_string(),
                        block_epoch: None,
                        confirmed: false,
                    };
                    let value = match serde_json::to_value(output) {
                        Ok(x) => x,
                        Err(e) => {
                            let err = internal_error(e);
                            return Err(err);
                        }
                    };
                    Ok(value)
                }
                Ok(Err(())) => {
                    let err = internal_error(InventoryManagerError::ItemNotFound);
                    Err(err)
                }
                Err(e) => {
                    let err = internal_error(e);
                    Err(err)
                }
            }
        }
        Ok(Err(e)) => {
            let err = internal_error(e);
            Err(err)
        }
        Err(e) => {
            let err = internal_error(e);
            Err(err)
        }
    }
}

/*
/// get output
pub fn get_output(output_pointer: Result<(String,), jsonrpc_core::Error>) -> JsonRpcResult {
    let output_pointer = match output_pointer {
        Ok(x) => match OutputPointer::from_str(&x.0) {
            Ok(x) => x,
            Err(e) => {
                let err = internal_error(e);
                return Box::new(Err(err));
            }
        },
        Err(e) => {
            return Box::new(Err(e));
        }
    };
    let chain_manager_addr = ChainManager::from_registry();
    Box::new(
        chain_manager_addr
            .send(GetOutput { output_pointer })
            .then(|res| match res {
                Ok(Ok(output)) => {
                    let value = match serde_json::to_value(output) {
                        Ok(x) => x,
                        Err(e) => {
                            let err = internal_error(e);
                            return Err(err);
                        }
                    };
                    Ok(value)
                }
                Ok(Err(e)) => {
                    let err = internal_error(e);
                    Err(err)
                }
                Err(e) => {
                    let err = internal_error(e);
                    Err(err)
                }
            }),
    )
}
*/
/// Build data request transaction
pub async fn send_request(params: Result<BuildDrt, jsonrpc_core::Error>) -> JsonRpcResult {
    log::debug!("Creating data request from JSON-RPC.");

    match params {
        Ok(msg) => {
            ChainManager::from_registry()
                .send(msg)
                .map(|res| match res {
                    Ok(Ok(hash)) => match serde_json::to_value(hash) {
                        Ok(x) => Ok(x),
                        Err(e) => {
                            let err = internal_error_s(e);
                            Err(err)
                        }
                    },
                    Ok(Err(e)) => {
                        let err = internal_error_s(e);
                        Err(err)
                    }
                    Err(e) => {
                        let err = internal_error_s(e);
                        Err(err)
                    }
                })
                .await
        }
        Err(err) => Err(err),
    }
}

/// Try a data request locally
pub async fn try_request(params: Result<DataRequestOutput, jsonrpc_core::Error>) -> JsonRpcResult {
    log::debug!("Trying a data request from JSON-RPC.");

    match params {
        Ok(dr_output) => match run_dr_locally(&dr_output) {
            Ok(result) => Ok(Value::String(result.to_string())),
            Err(e) => Err(internal_error_s(e)),
        },
        Err(err) => Err(err),
    }
}

/// Build value transfer transaction
pub async fn send_value(params: Result<BuildVtt, jsonrpc_core::Error>) -> JsonRpcResult {
    log::debug!("Creating value transfer from JSON-RPC.");

    match params {
        Ok(msg) => {
            ChainManager::from_registry()
                .send(msg)
                .map(|res| match res {
                    Ok(Ok(hash)) => match serde_json::to_value(hash) {
                        Ok(x) => Ok(x),
                        Err(e) => {
                            let err = internal_error_s(e);
                            Err(err)
                        }
                    },
                    Ok(Err(e)) => {
                        let err = internal_error_s(e);
                        Err(err)
                    }
                    Err(e) => {
                        let err = internal_error_s(e);
                        Err(err)
                    }
                })
                .await
        }
        Err(err) => Err(err),
    }
}

/// Get node status
pub async fn status() -> JsonRpcResult {
    let chain_manager = ChainManager::from_registry();
    let epoch_manager = EpochManager::from_registry();
    let node_state_fut = async {
        let res = chain_manager.send(GetState).await.map_err(internal_error_s);

        match res {
            Ok(Ok(StateMachine::Synced)) => Ok(StateMachine::Synced),
            Ok(Ok(StateMachine::AlmostSynced)) => Ok(StateMachine::AlmostSynced),
            Ok(Ok(StateMachine::WaitingConsensus)) => Ok(StateMachine::WaitingConsensus),
            Ok(Ok(StateMachine::Synchronizing)) => Ok(StateMachine::Synchronizing),
            Ok(Err(())) => Err(internal_error(())),
            Err(e) => Err(internal_error(e)),
        }
    };

    let chain_beacon_fut = async {
        let res = chain_manager.send(GetHighestCheckpointBeacon).await;

        match res {
            Ok(Ok(x)) => Ok(x),
            Ok(Err(e)) => Err(internal_error_s(e)),
            Err(e) => Err(internal_error_s(e)),
        }
    };

    let current_epoch_fut = async {
        let res = epoch_manager.send(GetEpoch).await;

        match res {
            Ok(Ok(x)) => Ok(Some(x)),
            Ok(Err(EpochManagerError::CheckpointZeroInTheFuture(_))) => Ok(None),
            Ok(Err(e)) => Err(internal_error(e)),
            Err(e) => Err(internal_error_s(e)),
        }
    };

    futures_util::future::try_join3(chain_beacon_fut, current_epoch_fut, node_state_fut)
        .map(|res| {
            res.map(|(chain_beacon, current_epoch, node_state)| SyncStatus {
                chain_beacon,
                current_epoch,
                node_state,
            })
        })
        .map(|res| {
            res.and_then(|res| match serde_json::to_value(res) {
                Ok(x) => Ok(x),
                Err(e) => {
                    let err = internal_error_s(e);
                    Err(err)
                }
            })
        })
        .await
}

/// Get public key
pub async fn get_public_key() -> JsonRpcResult {
    signature_mngr::public_key()
        .map(|res| {
            res.map_err(internal_error).map(|pk| {
                log::debug!("{:?}", pk);
                pk.to_bytes().to_vec().into()
            })
        })
        .await
}

/// Get public key hash
pub async fn get_pkh() -> JsonRpcResult {
    signature_mngr::pkh()
        .map(|res| {
            res.map_err(internal_error)
                .map(|pkh| Value::String(pkh.to_string()))
        })
        .await
}

/// Sign Data
pub async fn sign_data(params: Result<[u8; 32], jsonrpc_core::Error>) -> JsonRpcResult {
    let data = match params {
        Ok(x) => x,
        Err(e) => return Err(e),
    };

    signature_mngr::sign_data(data)
        .map(|res| {
            res.map_err(internal_error)
                .and_then(|ks| match serde_json::to_value(ks) {
                    Ok(value) => Ok(value),
                    Err(e) => {
                        let err = internal_error_s(e);
                        Err(err)
                    }
                })
        })
        .await
}

/// Create VRF
pub async fn create_vrf(params: Result<Vec<u8>, jsonrpc_core::Error>) -> JsonRpcResult {
    let data = match params {
        Ok(x) => x,
        Err(e) => return Err(e),
    };

    signature_mngr::vrf_prove(VrfMessage::set_data(data))
        .map(|res| {
            res.map_err(internal_error).map(|(proof, _hash)| {
                log::debug!("{:?}", proof);
                proof.get_proof().into()
            })
        })
        .await
}

/// Data request info
pub async fn data_request_report(params: Result<(Hash,), jsonrpc_core::Error>) -> JsonRpcResult {
    let dr_pointer = match params {
        Ok(x) => x.0,
        Err(e) => return Err(e),
    };

    let chain_manager_addr = ChainManager::from_registry();

    chain_manager_addr
        .send(GetDataRequestInfo { dr_pointer })
        .map(|res| {
            res.map_err(internal_error)
                .and_then(|dr_info| match dr_info {
                    Ok(x) => match serde_json::to_value(&x) {
                        Ok(x) => Ok(x),
                        Err(e) => {
                            let err = internal_error_s(e);
                            Err(err)
                        }
                    },
                    Err(e) => Err(internal_error_s(e)),
                })
        })
        .await
}

/// Params of getBlockChain method
#[derive(Debug, Default, Deserialize, Serialize)]
pub struct GetBalanceParams {
    /// Public key hash
    pub pkh: PublicKeyHash,
    /// Distinguish between fetching a simple balance or fetching confirmed and unconfirmed balance
    #[serde(default)] // default to false
    pub simple: bool,
}

/// Get balance
pub async fn get_balance(params: Params) -> JsonRpcResult {
    let (pkh, simple): (PublicKeyHash, bool);

    // Handle parameters as an array with a first obligatory PublicKeyHash field plus an optional bool field
    if let Params::Array(params) = params {
        if let Some(Value::String(public_key)) = params.get(0) {
            match public_key.parse() {
                Ok(public_key) => pkh = public_key,
                Err(e) => {
                    return Err(internal_error(e));
                }
            }
        } else {
            return Err(jsonrpc_core::Error::invalid_params(
                "First argument of `get_balance` must have type `PublicKeyHash`",
            ));
        };

        simple = false;
    } else {
        let params: GetBalanceParams = params.parse()?;
        pkh = params.pkh;
        simple = params.simple;
    };

    let chain_manager_addr = ChainManager::from_registry();

    chain_manager_addr
        .send(GetBalance { pkh, simple })
        .map(|res| {
            res.map_err(internal_error)
                .and_then(|dr_info| match dr_info {
                    Ok(x) => match serde_json::to_value(&x) {
                        Ok(x) => Ok(x),
                        Err(e) => {
                            let err = internal_error_s(e);
                            Err(err)
                        }
                    },
                    Err(e) => Err(internal_error_s(e)),
                })
        })
        .await
}

/// Get supply info
pub async fn get_supply_info() -> JsonRpcResult {
    let chain_manager_addr = ChainManager::from_registry();

    chain_manager_addr
        .send(GetSupplyInfo)
        .map(|res| {
            res.map_err(internal_error)
                .and_then(|supply_info| match supply_info {
                    Ok(x) => match serde_json::to_value(&x) {
                        Ok(x) => Ok(x),
                        Err(e) => {
                            let err = internal_error_s(e);
                            Err(err)
                        }
                    },
                    Err(e) => Err(internal_error_s(e)),
                })
        })
        .await
}

/// Get utxos
pub async fn get_utxo_info(params: Result<(PublicKeyHash,), jsonrpc_core::Error>) -> JsonRpcResult {
    let chain_manager_addr = ChainManager::from_registry();
    let pkh = match params {
        Ok(x) => x.0,
        Err(e) => return Err(e),
    };

    chain_manager_addr
        .send(GetUtxoInfo { pkh })
        .map(|res| {
            res.map_err(internal_error)
                .and_then(|dr_info| match dr_info {
                    Ok(x) => match serde_json::to_value(&x) {
                        Ok(x) => Ok(x),
                        Err(e) => {
                            let err = internal_error_s(e);
                            Err(err)
                        }
                    },
                    Err(e) => Err(internal_error_s(e)),
                })
        })
        .await
}

/// Get Reputation of one pkh
pub async fn get_reputation(
    params: Result<(PublicKeyHash,), jsonrpc_core::Error>,
    all: bool,
) -> JsonRpcResult {
    let pkh = match params {
        Ok(x) => x.0,
        Err(e) => return Err(e),
    };

    let chain_manager_addr = ChainManager::from_registry();

    chain_manager_addr
        .send(GetReputation { pkh, all })
        .map(|res| {
            res.map_err(internal_error)
                .and_then(|dr_info| match dr_info {
                    Ok(x) => match serde_json::to_value(&x) {
                        Ok(x) => Ok(x),
                        Err(e) => {
                            let err = internal_error_s(e);
                            Err(err)
                        }
                    },
                    Err(e) => Err(internal_error_s(e)),
                })
        })
        .await
}

/// Export private key associated with the node identity
pub async fn master_key_export() -> JsonRpcResult {
    signature_mngr::key_pair()
        .map(|res| {
            res.map_err(internal_error)
                .and_then(move |(_extended_pk, extended_sk)| {
                    let master_path = KeyPath::default();
                    let secret_key_hex = extended_sk.to_slip32(&master_path);
                    let secret_key_hex = match secret_key_hex {
                        Ok(x) => x,
                        Err(e) => return Err(internal_error_s(e)),
                    };
                    match serde_json::to_value(secret_key_hex) {
                        Ok(x) => Ok(x),
                        Err(e) => {
                            let err = internal_error_s(e);
                            Err(err)
                        }
                    }
                })
        })
        .await
}

/// Named tuple of `(address, type)`
#[derive(Debug, Default, Clone, Deserialize, Serialize)]
pub struct AddrType {
    /// Socket address of the peer
    pub address: String,
    /// "inbound" | "outbound" when asking for connected peers, or
    /// "new" | "tried" when asking for all the known peers
    #[serde(rename = "type")]
    pub type_: String,
}

/// Result of the `peers` method
pub type PeersResult = Vec<AddrType>;

/// Get list of consolidated peers
pub async fn peers() -> JsonRpcResult {
    let sessions_manager_addr = SessionsManager::from_registry();

    sessions_manager_addr
        .send(GetConsolidatedPeers)
        .map(|res| {
            res.map_err(internal_error)
                .and_then(|consolidated_peers| match consolidated_peers {
                    Ok(x) => {
                        let peers: Vec<_> = x
                            .inbound
                            .into_iter()
                            .sorted_by_key(|p| (p.is_ipv6(), p.ip(), p.port()))
                            .map(|p| AddrType {
                                address: p.to_string(),
                                type_: "inbound".to_string(),
                            })
                            .chain(
                                x.outbound
                                    .into_iter()
                                    .sorted_by_key(|p| (p.is_ipv6(), p.ip(), p.port()))
                                    .map(|p| AddrType {
                                        address: p.to_string(),
                                        type_: "outbound".to_string(),
                                    }),
                            )
                            .collect();

                        match serde_json::to_value(&peers) {
                            Ok(x) => Ok(x),
                            Err(e) => {
                                let err = internal_error_s(e);
                                Err(err)
                            }
                        }
                    }
                    Err(()) => Err(internal_error(())),
                })
        })
        .await
}

/// Get list of known peers
pub async fn known_peers() -> JsonRpcResult {
    let peers_manager_addr = PeersManager::from_registry();

    peers_manager_addr
        .send(GetKnownPeers)
        .map(|res| {
            res.map_err(internal_error)
                .and_then(|known_peers| match known_peers {
                    Ok(x) => {
                        let peers: Vec<_> = x
                            .new
                            .into_iter()
                            .sorted_by_key(|p| (p.is_ipv6(), p.ip(), p.port()))
                            .map(|p| AddrType {
                                address: p.to_string(),
                                type_: "new".to_string(),
                            })
                            .chain(
                                x.tried
                                    .into_iter()
                                    .sorted_by_key(|p| (p.is_ipv6(), p.ip(), p.port()))
                                    .map(|p| AddrType {
                                        address: p.to_string(),
                                        type_: "tried".to_string(),
                                    }),
                            )
                            .collect();

                        match serde_json::to_value(&peers) {
                            Ok(x) => Ok(x),
                            Err(e) => {
                                let err = internal_error_s(e);
                                Err(err)
                            }
                        }
                    }
                    Err(e) => Err(internal_error_s(e)),
                })
        })
        .await
}

/// Get the node stats
pub async fn node_stats() -> JsonRpcResult {
    let chain_manager_addr = ChainManager::from_registry();

    chain_manager_addr
        .send(GetNodeStats)
        .map(|res| {
            res.map_err(internal_error)
                .and_then(|node_stats| match node_stats {
                    Ok(x) => match serde_json::to_value(&x) {
                        Ok(x) => Ok(x),
                        Err(e) => {
                            let err = internal_error_s(e);
                            Err(err)
                        }
                    },
                    Err(e) => Err(internal_error_s(e)),
                })
        })
        .await
}

/// Get all the pending transactions
pub async fn get_mempool(params: Result<(), jsonrpc_core::Error>) -> JsonRpcResult {
    match params {
        Ok(()) => (),
        Err(e) => return Err(e),
    };

    let chain_manager_addr = ChainManager::from_registry();

    chain_manager_addr
        .send(GetMempool)
        .map(|res| {
            res.map_err(internal_error)
                .and_then(|dr_info| match dr_info {
                    Ok(x) => match serde_json::to_value(&x) {
                        Ok(x) => Ok(x),
                        Err(e) => {
                            let err = internal_error_s(e);
                            Err(err)
                        }
                    },
                    Err(e) => Err(internal_error_s(e)),
                })
        })
        .await
}

/// Add peers
pub async fn add_peers(params: Result<Vec<SocketAddr>, jsonrpc_core::Error>) -> JsonRpcResult {
    let addresses = match params {
        Ok(x) => x,
        Err(e) => return Err(e),
    };
    // Use None as the source address: this will make adding peers using this method be exactly the
    // same as adding peers using the configuration file
    let src_address = None;
    let peers_manager_addr = PeersManager::from_registry();

    peers_manager_addr
        .send(AddPeers {
            addresses,
            src_address,
        })
        .map(|res| {
            res.map_err(internal_error).and_then(|res| match res {
                Ok(_overwritten_peers) => {
                    // Ignore overwritten peers, just return true on success
                    Ok(Value::Bool(true))
                }
                Err(e) => Err(internal_error_s(e)),
            })
        })
        .await
}

/// Clear peers
pub async fn clear_peers() -> JsonRpcResult {
    let peers_manager_addr = PeersManager::from_registry();

    peers_manager_addr
        .send(ClearPeers)
        .map(|res| {
            res.map_err(internal_error).and_then(|res| match res {
                Ok(_overwritten_peers) => {
                    // Ignore overwritten peers, just return true on success
                    Ok(Value::Bool(true))
                }
                Err(e) => Err(internal_error_s(e)),
            })
        })
        .await
}

/// Initialize peers
pub async fn initialize_peers() -> JsonRpcResult {
    config_mngr::get()
        .map(|res| res.map_err(internal_error))
        .then(|res| async {
            match res {
                Ok(config) => {
                    // Clear all peers
                    let known_peers: Vec<_> =
                        config.connections.known_peers.iter().cloned().collect();
                    let peers_manager_addr = PeersManager::from_registry();
                    peers_manager_addr
                        .send(InitializePeers { known_peers })
                        .map(|res| {
                            res.map_err(internal_error).and_then(|res| match res {
                                Ok(_overwritten_peers) => {
                                    // Drop all peers from session manager
                                    let sessions_manager_addr = SessionsManager::from_registry();
                                    sessions_manager_addr.do_send(DropAllPeers);

                                    // Ignore overwritten peers, just return true on success
                                    Ok(Value::Bool(true))
                                }
                                Err(e) => Err(internal_error_s(e)),
                            })
                        })
                        .await
                }
                Err(e) => Err(e),
            }
        })
        .await
}

/// Get consensus constants used by the node
pub async fn get_consensus_constants(params: Result<(), jsonrpc_core::Error>) -> JsonRpcResult {
    match params {
        Ok(()) => (),
        Err(e) => return Err(e),
    };

    config_mngr::get()
        .map(|res| {
            res.map_err(internal_error).and_then(|config| {
                match serde_json::to_value(&config.consensus_constants) {
                    Ok(x) => Ok(x),
                    Err(e) => {
                        let err = internal_error_s(e);
                        Err(err)
                    }
                }
            })
        })
        .await
}

/// Rewind
pub async fn rewind(params: Result<(Epoch,), jsonrpc_core::Error>) -> JsonRpcResult {
    let epoch = match params {
        Ok((epoch,)) => epoch,
        Err(e) => return Err(e),
    };

    let chain_manager_addr = ChainManager::from_registry();
    chain_manager_addr
        .send(Rewind { epoch })
        .map(|res| {
            res.map_err(internal_error)
                .and_then(|success| match success {
                    Ok(x) => match serde_json::to_value(&x) {
                        Ok(x) => Ok(x),
                        Err(e) => {
                            let err = internal_error_s(e);
                            Err(err)
                        }
                    },
                    Err(e) => Err(internal_error_s(e)),
                })
        })
        .await
}

/// Parameter of getSuperblock: can be either block epoch or superblock index
#[derive(Deserialize)]
pub enum GetSuperblockBlocksParams {
    /// Get superblock by block epoch
    #[serde(rename = "block_epoch")]
    BlockEpoch(u32),
    /// Get superblock by superblock index
    #[serde(rename = "superblock_index")]
    SuperblockIndex(u32),
}

/// Get the blocks that pertain to the superblock index
pub async fn get_superblock(
    params: Result<GetSuperblockBlocksParams, jsonrpc_core::Error>,
) -> JsonRpcResult {
    let params = match params {
        Ok(x) => x,
        Err(e) => return Err(e),
    };

    let superblock_index = match params {
        GetSuperblockBlocksParams::SuperblockIndex(superblock_index) => Ok(superblock_index),
        GetSuperblockBlocksParams::BlockEpoch(block_epoch) => {
            config_mngr::get()
                .map(|res| {
                    res.map_err(internal_error).map(move |config| {
                        let superblock_period =
                            u32::from(config.consensus_constants.superblock_period);
                        // Calculate the superblock_index that contains the block_epoch.
                        // The +1 is needed because block_epoch/superblock_period will be the current
                        // top superblock, and we want the next one because that's the one that
                        // consolidates this block.
                        (block_epoch / superblock_period) + 1
                    })
                })
                .await
        }
    }?;

    let inventory_manager_addr = InventoryManager::from_registry();

    let dr_info = inventory_manager_addr
        .send(GetItemSuperblock { superblock_index })
        .await
        .map_err(internal_error)?;

    match dr_info {
        Ok(x) => match serde_json::to_value(&x) {
            Ok(x) => Ok(x),
            Err(e) => {
                let err = internal_error_s(e);
                Err(err)
            }
        },
        Err(e) => Err(internal_error_s(e)),
    }
}

/// Get the blocks that pertain to the superblock index
pub async fn signaling_info(params: Result<(), jsonrpc_core::Error>) -> JsonRpcResult {
    let _params = match params {
        Ok(x) => x,
        Err(e) => return Err(e),
    };

    let chain_manager_addr = ChainManager::from_registry();

    let info = chain_manager_addr
        .send(GetSignalingInfo {})
        .await
        .map_err(internal_error)?;

    match info {
        Ok(x) => match serde_json::to_value(&x) {
            Ok(x) => Ok(x),
            Err(e) => {
                let err = internal_error_s(e);
                Err(err)
            }
        },
        Err(e) => Err(internal_error_s(e)),
    }
}

#[cfg(test)]
mod mock_actix {
    use actix::{MailboxError, Message};

    pub struct Addr;

    impl Addr {
        pub async fn send<T: Message>(&self, _msg: T) -> Result<T::Result, MailboxError> {
            // We cannot test methods which use `send`, so return an error
            Err(MailboxError::Closed)
        }

        pub fn do_send<T: Message>(&self, _msg: T) {
            // We cannot test methods which use `do_send`, so return an error
            panic!("We cannot test actix `do_send` methods")
        }
    }

    pub trait SystemService {
        fn from_registry() -> Addr {
            Addr
        }
    }

    impl<T> SystemService for T {}
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use jsonrpc_core::futures::sync::mpsc;

    use witnet_data_structures::{chain::RADRequest, transaction::*};

    use super::*;

    #[test]
    fn empty_string_parse_error() {
        // An empty message should return a parse error
        let empty_string = "";
        let parse_error =
            r#"{"jsonrpc":"2.0","error":{"code":-32700,"message":"Parse error"},"id":null}"#
                .to_string();
        let subscriptions = Subscriptions::default();
        let (transport_sender, _transport_receiver) = mpsc::channel(0);
        let meta = Arc::new(Session::new(transport_sender));
        let io = jsonrpc_io_handler(subscriptions, true);
        let response = io.handle_request_sync(empty_string, meta);
        assert_eq!(response, Some(parse_error));
    }

    #[test]
    fn inventory_method() {
        // The expected behaviour of the inventory method
        use witnet_data_structures::chain::*;
        let block = block_example();

        let inv_elem = InventoryItem::Block(block);
        let s = serde_json::to_string(&inv_elem).unwrap();
        let msg = format!(
            r#"{{"jsonrpc":"2.0","method":"inventory","params":{},"id":1}}"#,
            s
        );

        // Expected result: true
        let expected = r#"{"jsonrpc":"2.0","error":{"code":-32603,"message":"MailboxError(Mailbox has closed)"},"id":1}"#.to_string();
        let subscriptions = Subscriptions::default();
        let (transport_sender, _transport_receiver) = mpsc::channel(0);
        let meta = Arc::new(Session::new(transport_sender));
        let io = jsonrpc_io_handler(subscriptions, true);
        let response = io.handle_request_sync(&msg, meta);
        assert_eq!(response, Some(expected));
    }

    #[test]
    fn inventory_invalid_params() {
        // What happens when the inventory method is called with an invalid parameter?
        let msg = r#"{"jsonrpc":"2.0","method":"inventory","params":{ "header": 0 },"id":1}"#;
        let expected = r#"{"jsonrpc":"2.0","error":{"code":-32602,"message":"Invalid params: unknown variant `header`, expected one of"#.to_string();
        let subscriptions = Subscriptions::default();
        let (transport_sender, _transport_receiver) = mpsc::channel(0);
        let meta = Arc::new(Session::new(transport_sender));
        let io = jsonrpc_io_handler(subscriptions, true);
        let response = io.handle_request_sync(msg, meta);
        // Compare only the first N characters
        let response =
            response.map(|s| s.chars().take(expected.chars().count()).collect::<String>());
        assert_eq!(response, Some(expected));
    }

    #[test]
    fn inventory_unimplemented_type() {
        // What happens when the inventory method is called with an unimplemented type?
        let msg = r#"{"jsonrpc":"2.0","method":"inventory","params":{ "error": null },"id":1}"#;
        let expected =
            r#"{"jsonrpc":"2.0","error":{"code":-32602,"message":"Item type not implemented"},"id":1}"#
                .to_string();
        let subscriptions = Subscriptions::default();
        let (transport_sender, _transport_receiver) = mpsc::channel(0);
        let meta = Arc::new(Session::new(transport_sender));
        let io = jsonrpc_io_handler(subscriptions, true);
        let response = io.handle_request_sync(msg, meta);
        assert_eq!(response, Some(expected));
    }

    #[test]
    fn inventory_block() {
        // Check that the inventory method accepts blocks
        use witnet_data_structures::chain::*;
        let block = block_example();
        let inv_elem = InventoryItem::Block(block);
        let msg = format!(
            r#"{{"jsonrpc":"2.0","method":"inventory","params":{},"id":1}}"#,
            serde_json::to_string(&inv_elem).unwrap()
        );
        let expected = r#"{"jsonrpc":"2.0","error":{"code":-32603,"message":"MailboxError(Mailbox has closed)"},"id":1}"#.to_string();
        let subscriptions = Subscriptions::default();
        let (transport_sender, _transport_receiver) = mpsc::channel(0);
        let meta = Arc::new(Session::new(transport_sender));
        let io = jsonrpc_io_handler(subscriptions, true);
        let response = io.handle_request_sync(&msg, meta);
        assert_eq!(response, Some(expected));
    }

    #[test]
    fn get_block_chain_abs_overflow() {
        // Ensure that the get_block_chain method does not panic when passed i64::MIN as argument
        let params = GetBlockChainParams {
            epoch: i64::MIN,
            limit: 1,
        };
        let msg = format!(
            r#"{{"jsonrpc":"2.0","method":"getBlockChain","params":{},"id":1}}"#,
            serde_json::to_string(&params).unwrap()
        );
        let expected = r#"{"jsonrpc":"2.0","error":{"code":-32603,"message":"Epoch out of bounds: -9223372036854775808 must be between -4294967295 and 4294967295 inclusive"},"id":1}"#.to_string();
        let subscriptions = Subscriptions::default();
        let (transport_sender, _transport_receiver) = mpsc::channel(0);
        let meta = Arc::new(Session::new(transport_sender));
        let io = jsonrpc_io_handler(subscriptions, true);
        let response = io.handle_request_sync(&msg, meta.clone());
        assert_eq!(response, Some(expected));

        let params = GetBlockChainParams {
            epoch: 1,
            limit: i64::MIN,
        };
        let msg = format!(
            r#"{{"jsonrpc":"2.0","method":"getBlockChain","params":{},"id":1}}"#,
            serde_json::to_string(&params).unwrap()
        );
        let expected = r#"{"jsonrpc":"2.0","error":{"code":-32603,"message":"Limit out of bounds: -9223372036854775808 must be between -4294967295 and 4294967295 inclusive"},"id":1}"#.to_string();
        let response = io.handle_request_sync(&msg, meta);
        assert_eq!(response, Some(expected));
    }

    #[test]
    fn subscribe_invalid_method() {
        // Try to subscribe to a non-existent subscription?
        let msg = r#"{"jsonrpc":"2.0","method":"witnet_subscribe","params":["asdf"],"id":1}"#;
        let expected =
            r#"{"jsonrpc":"2.0","error":{"code":-32602,"message":"Unknown subscription method: asdf"},"id":1}"#
                .to_string();
        let subscriptions = Subscriptions::default();
        let (transport_sender, _transport_receiver) = mpsc::channel(0);
        let meta = Arc::new(Session::new(transport_sender));
        let io = jsonrpc_io_handler(subscriptions, true);
        let response = io.handle_request_sync(msg, meta);
        assert_eq!(response, Some(expected));
    }

    #[test]
    fn subscribe_new_blocks() {
        // Subscribe to new blocks gives us a SubscriptionId
        let msg = r#"{"jsonrpc":"2.0","method":"witnet_subscribe","params":["blocks"],"id":1}"#;
        let expected = r#"{"jsonrpc":"2.0","result":"1","id":1}"#.to_string();
        let subscriptions = Subscriptions::default();
        let (transport_sender, _transport_receiver) = mpsc::channel(0);
        let meta = Arc::new(Session::new(transport_sender));
        let io = jsonrpc_io_handler(subscriptions, true);
        let response = io.handle_request_sync(msg, meta);
        assert_eq!(response, Some(expected));
    }

    #[test]
    fn unsubscribe_returns_true() {
        // Check that unsubscribe returns true
        let msg2 = r#"{"jsonrpc":"2.0","method":"witnet_unsubscribe","params":["1"],"id":1}"#;
        let expected2 = r#"{"jsonrpc":"2.0","result":true,"id":1}"#.to_string();
        let subscriptions = Subscriptions::default();
        let (transport_sender, _transport_receiver) = mpsc::channel(0);
        let meta = Arc::new(Session::new(transport_sender));
        let io = jsonrpc_io_handler(subscriptions, true);
        // But first, subscribe to blocks
        let msg1 = r#"{"jsonrpc":"2.0","method":"witnet_subscribe","params":["blocks"],"id":1}"#;
        let _response1 = io.handle_request_sync(msg1, meta.clone());
        let response2 = io.handle_request_sync(msg2, meta);
        assert_eq!(response2, Some(expected2));
    }

    #[test]
    fn unsubscribe_can_fail() {
        // Check that unsubscribe returns false when unsubscribing to a non-existent subscription
        let msg = r#"{"jsonrpc":"2.0","method":"witnet_unsubscribe","params":["999"],"id":1}"#;
        let expected = r#"{"jsonrpc":"2.0","result":false,"id":1}"#.to_string();
        let subscriptions = Subscriptions::default();
        let (transport_sender, _transport_receiver) = mpsc::channel(0);
        let meta = Arc::new(Session::new(transport_sender));
        let io = jsonrpc_io_handler(subscriptions, true);
        let response = io.handle_request_sync(msg, meta);
        assert_eq!(response, Some(expected));
    }

    #[test]
    fn serialize_block() {
        // Check that the serialization of `Block` doesn't change
        use witnet_data_structures::chain::*;
        let block = block_example();
        let inv_elem = InventoryItem::Block(block);
        let s = serde_json::to_string(&inv_elem).unwrap();
        let expected = r#"{"block":{"block_header":{"signals":0,"beacon":{"checkpoint":0,"hashPrevBlock":"0000000000000000000000000000000000000000000000000000000000000000"},"merkle_roots":{"mint_hash":"0000000000000000000000000000000000000000000000000000000000000000","vt_hash_merkle_root":"0000000000000000000000000000000000000000000000000000000000000000","dr_hash_merkle_root":"0000000000000000000000000000000000000000000000000000000000000000","commit_hash_merkle_root":"0000000000000000000000000000000000000000000000000000000000000000","reveal_hash_merkle_root":"0000000000000000000000000000000000000000000000000000000000000000","tally_hash_merkle_root":"0000000000000000000000000000000000000000000000000000000000000000"},"proof":{"proof":{"proof":[],"public_key":{"compressed":0,"bytes":[0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0]}}},"bn256_public_key":null},"block_sig":{"signature":{"Secp256k1":{"der":[]}},"public_key":{"compressed":0,"bytes":[0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0]}},"txns":{"mint":{"epoch":0,"outputs":[]},"value_transfer_txns":[],"data_request_txns":[{"body":{"inputs":[{"output_pointer":"0000000000000000000000000000000000000000000000000000000000000000:0"}],"outputs":[{"pkh":"wit1qqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqwrt3a4","value":0,"time_lock":0}],"dr_output":{"data_request":{"time_lock":0,"retrieve":[{"kind":"HTTP-GET","url":"https://openweathermap.org/data/2.5/weather?id=2950159&appid=b6907d289e10d714a6e88b30761fae22"},{"kind":"HTTP-GET","url":"https://openweathermap.org/data/2.5/weather?id=2950159&appid=b6907d289e10d714a6e88b30761fae22"}],"aggregate":{"filters":[],"reducer":0},"tally":{"filters":[],"reducer":0}},"witness_reward":0,"witnesses":0,"commit_and_reveal_fee":0,"min_consensus_percentage":0,"collateral":0}},"signatures":[{"signature":{"Secp256k1":{"der":[]}},"public_key":{"compressed":0,"bytes":[0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0]}}]}],"commit_txns":[],"reveal_txns":[],"tally_txns":[]}}}"#;
        assert_eq!(s, expected, "\n{}\n", s);
    }

    #[test]
    fn serialize_transaction() {
        use witnet_data_structures::chain::*;
        let rad_aggregate = RADAggregate::default();

        let rad_retrieve_1 = RADRetrieve {
            kind: RADType::HttpGet,
            url: "https://openweathermap.org/data/2.5/weather?id=2950159&appid=b6907d289e10d714a6e88b30761fae22".to_string(),
            script: vec![0],
            body: vec![],
            headers: vec![],
        };

        let rad_retrieve_2 = RADRetrieve {
            kind: RADType::HttpGet,
            url: "https://openweathermap.org/data/2.5/weather?id=2950159&appid=b6907d289e10d714a6e88b30761fae22".to_string(),
            script: vec![0],
            body: vec![],
            headers: vec![],
        };

        let rad_consensus = RADTally::default();

        let rad_request = RADRequest {
            aggregate: rad_aggregate,
            time_lock: 0,
            retrieve: vec![rad_retrieve_1, rad_retrieve_2],
            tally: rad_consensus,
        };
        // Check that the serialization of `Transaction` doesn't change
        let transaction = build_hardcoded_transaction(rad_request);

        let inv_elem = InventoryItem::Transaction(transaction);
        let s = serde_json::to_string(&inv_elem).unwrap();
        let expected = r#"{"transaction":{"DataRequest":{"body":{"inputs":[{"output_pointer":"0909090909090909090909090909090909090909090909090909090909090909:0"}],"outputs":[],"dr_output":{"data_request":{"time_lock":0,"retrieve":[{"kind":"HTTP-GET","url":"https://openweathermap.org/data/2.5/weather?id=2950159&appid=b6907d289e10d714a6e88b30761fae22","script":[0]},{"kind":"HTTP-GET","url":"https://openweathermap.org/data/2.5/weather?id=2950159&appid=b6907d289e10d714a6e88b30761fae22","script":[0]}],"aggregate":{"filters":[],"reducer":0},"tally":{"filters":[],"reducer":0}},"witness_reward":0,"witnesses":0,"commit_and_reveal_fee":0,"min_consensus_percentage":0,"collateral":0}},"signatures":[{"signature":{"Secp256k1":{"der":[]}},"public_key":{"compressed":0,"bytes":[0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0]}}]}}}"#;
        assert_eq!(s, expected, "\n{}\n", s);
    }

    fn build_hardcoded_transaction(data_request: RADRequest) -> Transaction {
        use witnet_data_structures::chain::*;
        let signature = Signature::Secp256k1(Secp256k1Signature::default());
        let keyed_signatures = vec![KeyedSignature {
            public_key: PublicKey::default(),
            signature,
        }];

        let value_transfer_input = Input::new(OutputPointer {
            transaction_id: Hash::SHA256([9; 32]),
            output_index: 0,
        });

        let data_request_output = DataRequestOutput {
            data_request,
            ..DataRequestOutput::default()
        };

        let inputs = vec![value_transfer_input];

        Transaction::DataRequest(DRTransaction::new(
            DRTransactionBody::new(inputs, vec![], data_request_output),
            keyed_signatures,
        ))
    }

    #[test]
    fn hash_str_format() {
        use witnet_data_structures::chain::Hash;
        let h = Hash::default();
        let hash_array = serde_json::to_string(&h).unwrap();
        let h2 = serde_json::from_str(&hash_array).unwrap();
        assert_eq!(h, h2);
        let hash_str = r#""0000000000000000000000000000000000000000000000000000000000000000""#;
        let h3 = serde_json::from_str(hash_str).unwrap();
        assert_eq!(h, h3);
    }

    #[test]
    fn build_drt_example() {
        let build_drt = BuildDrt::default();
        let s = serde_json::to_string(&build_drt).unwrap();
        let expected = r#"{"dro":{"data_request":{"time_lock":0,"retrieve":[],"aggregate":{"filters":[],"reducer":0},"tally":{"filters":[],"reducer":0}},"witness_reward":0,"witnesses":0,"commit_and_reveal_fee":0,"min_consensus_percentage":0,"collateral":0},"fee":0}"#;
        assert_eq!(s, expected, "\n{}\n", s);
    }

    #[test]
    fn list_jsonrpc_methods() {
        // This test will break when adding or removing JSON-RPC methods.
        // When adding a new method, please make sure to mark it as sensitive if that's the case.
        // Removing a method means breaking the API and should be avoided.
        let subscriptions = Subscriptions::default();
        let io = jsonrpc_io_handler(subscriptions, true);
        let all_methods: BTreeSet<_> = io
            .iter()
            .map(|(method_name, _method)| method_name)
            .collect();

        let all_methods_vec: Vec<_> = all_methods.iter().copied().collect();
        assert_eq!(
            all_methods_vec,
            vec![
                "addPeers",
                "clearPeers",
                "createVRF",
                "dataRequestReport",
                "getBalance",
                "getBlock",
                "getBlockChain",
                "getConsensusConstants",
                "getMempool",
                "getPkh",
                "getPublicKey",
                "getReputation",
                "getReputationAll",
                "getSuperblock",
                "getSupplyInfo",
                "getTransaction",
                "getUtxoInfo",
                "initializePeers",
                "inventory",
                "knownPeers",
                "masterKeyExport",
                "nodeStats",
                "peers",
                "rewind",
                "sendRequest",
                "sendValue",
                "sign",
                "signalingInfo",
                "syncStatus",
                "tryRequest",
                "witnet_subscribe",
                "witnet_unsubscribe",
            ]
        );

        let subscriptions = Subscriptions::default();
        let (transport_sender, _transport_receiver) = mpsc::channel(0);
        let meta = Arc::new(Session::new(transport_sender));
        let io = jsonrpc_io_handler(subscriptions, false);
        let non_sensitive_methods: BTreeSet<_> = io
            .iter()
            .map(|(method_name, _method)| method_name)
            .collect();

        // Disabling sensistive methods does not unregister them, the methods still exist but
        // they return a custom error message
        assert_eq!(all_methods.difference(&non_sensitive_methods).count(), 0);

        let expected_sensitive_methods = vec![
            "addPeers",
            "clearPeers",
            "createVRF",
            "getPkh",
            "getPublicKey",
            "getUtxoInfo",
            "initializePeers",
            "masterKeyExport",
            "rewind",
            "sendRequest",
            "sendValue",
            "sign",
            "tryRequest",
        ];

        for method_name in expected_sensitive_methods {
            let msg = format!(r#"{{"jsonrpc":"2.0","method":"{}","id":1}}"#, method_name);
            let error_msg = format!(
                r#"{{"jsonrpc":"2.0","error":{{"code":-32603,"message":{:?}}},"id":1}}"#,
                unauthorized_message(method_name)
            );

            let response = io.handle_request_sync(&msg, meta.clone());

            assert_eq!(response.unwrap(), error_msg);
        }
    }
}
