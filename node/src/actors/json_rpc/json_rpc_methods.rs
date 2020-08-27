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
use jsonrpc_core::{futures, futures::Future, BoxFuture, MetaIoHandler, Params, Value};
use jsonrpc_pubsub::{PubSubHandler, Session, Subscriber, SubscriptionId};
use serde::{Deserialize, Serialize};

use witnet_crypto::key::KeyPath;
use witnet_data_structures::{
    chain::{Block, CheckpointBeacon, Hash, Hashable, PublicKeyHash},
    transaction::Transaction,
    vrf::VrfMessage,
};

//use std::str::FromStr;
use super::Subscriptions;

#[cfg(test)]
use self::mock_actix::SystemService;
use crate::{
    actors::{
        chain_manager::{ChainManager, ChainManagerError, StateMachine},
        epoch_manager::{EpochManager, EpochManagerError},
        inventory_manager::{InventoryManager, InventoryManagerError},
        messages::{
            AddCandidates, AddPeers, AddTransaction, BuildDrt, BuildVtt, GetBalance,
            GetBlocksEpochRange, GetConsolidatedPeers, GetDataRequestReport, GetEpoch,
            GetHighestCheckpointBeacon, GetItemBlock, GetItemTransaction, GetKnownPeers,
            GetMemoryTransaction, GetMempool, GetNodeStats, GetReputation, GetState, GetUtxoInfo,
        },
        peers_manager::PeersManager,
        sessions_manager::SessionsManager,
    },
    signature_mngr,
};

type JsonRpcResultAsync = Box<dyn Future<Item = Value, Error = jsonrpc_core::Error> + Send>;

/// Define the JSON-RPC interface:
/// All the methods available through JSON-RPC
pub fn jsonrpc_io_handler(
    subscriptions: Subscriptions,
    enable_sensitive_methods: bool,
) -> PubSubHandler<Arc<Session>> {
    let mut io = PubSubHandler::new(MetaIoHandler::default());

    io.add_method("inventory", |params: Params| inventory(params.parse()));
    io.add_method("getBlockChain", |params: Params| {
        get_block_chain(params.parse())
    });
    io.add_method("getBlock", |params: Params| get_block(params.parse()));
    io.add_method("getTransaction", |params: Params| {
        get_transaction(params.parse())
    });
    //io.add_method("getOutput", |params: Params| get_output(params.parse()));
    io.add_method("syncStatus", |_params: Params| status());
    io.add_method("dataRequestReport", |params: Params| {
        data_request_report(params.parse())
    });
    io.add_method("getBalance", |params: Params| get_balance(params.parse()));
    io.add_method("getReputation", |params: Params| {
        get_reputation(params.parse(), false)
    });
    io.add_method("getReputationAll", |_params: Params| {
        get_reputation(Ok((PublicKeyHash::default(),)), true)
    });
    io.add_method("peers", |_params: Params| peers());
    io.add_method("knownPeers", |_params: Params| known_peers());
    io.add_method("nodeStats", |_params: Params| node_stats());
    io.add_method("getMempool", |params: Params| get_mempool(params.parse()));

    // Enable methods that assume that JSON-RPC is only accessible by the owner of the node.
    // A method is sensitive if it touches in some way the master key of the node.
    // For example: methods that can be used to create transactions (spending value from this node),
    // sign arbitrary messages with the node master key, and even export the master key.
    let unauthorized_method = |method_name| {
        Box::new(futures::failed(internal_error_s(unauthorized_message(
            method_name,
        ))))
    };
    io.add_method("sendRequest", move |params: Params| {
        if enable_sensitive_methods {
            send_request(params.parse())
        } else {
            unauthorized_method("sendRequest")
        }
    });
    io.add_method("sendValue", move |params: Params| {
        if enable_sensitive_methods {
            send_value(params.parse())
        } else {
            unauthorized_method("sendValue")
        }
    });
    io.add_method("getPublicKey", move |_params: Params| {
        if enable_sensitive_methods {
            get_public_key()
        } else {
            unauthorized_method("getPublicKey")
        }
    });
    io.add_method("getPkh", move |_params: Params| {
        if enable_sensitive_methods {
            get_pkh()
        } else {
            unauthorized_method("getPkh")
        }
    });
    io.add_method("getUtxoInfo", move |params: Params| {
        if enable_sensitive_methods {
            get_utxo_info(params.parse())
        } else {
            unauthorized_method("getUtxoInfo")
        }
    });
    io.add_method("sign", move |params: Params| {
        if enable_sensitive_methods {
            sign_data(params.parse())
        } else {
            unauthorized_method("sign")
        }
    });
    io.add_method("createVRF", move |params: Params| {
        if enable_sensitive_methods {
            create_vrf(params.parse())
        } else {
            unauthorized_method("createVRF")
        }
    });
    io.add_method("masterKeyExport", move |_params: Params| {
        if enable_sensitive_methods {
            master_key_export()
        } else {
            unauthorized_method("masterKeyExport")
        }
    });
    io.add_method("addPeers", move |params: Params| {
        if enable_sensitive_methods {
            add_peers(params.parse())
        } else {
            unauthorized_method("addPeers")
        }
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
                    "newBlocks" => {
                        add_subscription("newBlocks", subscriber);
                    }
                    "consolidatedBlocks" => {
                        add_subscription("consolidatedBlocks", subscriber);
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
            move |id: SubscriptionId, _meta: Option<Arc<Session>>| -> BoxFuture<Value> {
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

                        Box::new(futures::future::ok(Value::Bool(found)))
                    }
                    Err(e) => {
                        log::error!("Failed to acquire lock in witnet_unsubscribe");
                        Box::new(futures::future::err(internal_error(e)))
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
pub fn inventory(params: Result<InventoryItem, jsonrpc_core::Error>) -> JsonRpcResultAsync {
    let inv_elem = match params {
        Ok(x) => x,
        Err(e) => return Box::new(futures::failed(e)),
    };

    match inv_elem {
        InventoryItem::Block(block) => {
            log::debug!("Got block from JSON-RPC. Sending AnnounceItems message.");

            let chain_manager_addr = ChainManager::from_registry();
            let fut = chain_manager_addr
                .send(AddCandidates {
                    blocks: vec![block],
                })
                .map_err(internal_error)
                .map(|()| Value::Bool(true));

            Box::new(fut)
        }

        InventoryItem::Transaction(transaction) => {
            log::debug!("Got transaction from JSON-RPC. Sending AnnounceItems message.");

            let chain_manager_addr = ChainManager::from_registry();
            let fut = chain_manager_addr
                .send(AddTransaction { transaction })
                .map_err(internal_error)
                .and_then(|res| match res {
                    Ok(()) => futures::finished(Value::Bool(true)),
                    Err(e) => futures::failed(internal_error_s(e)),
                });

            Box::new(fut)
        }

        inv_elem => {
            log::debug!(
                "Invalid type of inventory item from JSON-RPC: {:?}",
                inv_elem
            );
            let fut = futures::failed(jsonrpc_core::Error::invalid_params(
                "Item type not implemented",
            ));

            Box::new(fut)
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
pub fn get_block_chain(
    params: Result<Option<GetBlockChainParams>, jsonrpc_core::Error>,
) -> JsonRpcResultAsync {
    // Helper function to convert the result of GetBlockEpochRange to a JSON value, or a JSON-RPC error
    fn process_get_block_chain(
        res: Result<Result<Vec<(u32, Hash)>, ChainManagerError>, MailboxError>,
    ) -> impl Future<Item = Value, Error = jsonrpc_core::Error> {
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
                        return futures::failed(err);
                    }
                };
                futures::finished(value)
            }
            Ok(Err(e)) => {
                let err = internal_error(e);
                futures::failed(err)
            }
            Err(e) => {
                let err = internal_error(e);
                futures::failed(err)
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
        let positive_x = u32::try_from(x.abs()).map_err(|_e| {
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
        Err(e) => return Box::new(futures::failed(e)),
    };

    let (epoch, epoch_negative) = match convert_negative_to_positive_with_negative_flag(epoch) {
        Ok(x) => x,
        Err(mut err_str) => {
            err_str.insert_str(0, "Epoch ");
            return Box::new(futures::failed(internal_error_s(err_str)));
        }
    };

    let (limit, limit_negative) = match convert_negative_to_positive_with_negative_flag(limit) {
        Ok(x) => x,
        Err(mut err_str) => {
            err_str.insert_str(0, "Limit ");
            return Box::new(futures::failed(internal_error_s(err_str)));
        }
    };

    let chain_manager_addr = ChainManager::from_registry();
    if epoch_negative {
        // On negative epoch, get blocks from last n epochs
        // But, what is the current epoch?
        let fut = EpochManager::from_registry()
            .send(GetEpoch)
            .then(move |res| match res {
                Ok(Ok(current_epoch)) => {
                    let epoch = current_epoch.saturating_sub(epoch);

                    futures::finished(epoch)
                }
                Ok(Err(e)) => {
                    let err = internal_error(e);
                    futures::failed(err)
                }
                Err(e) => {
                    let err = internal_error(e);
                    futures::failed(err)
                }
            })
            .and_then(move |epoch| {
                chain_manager_addr
                    .send(epoch_range(epoch, limit, limit_negative))
                    .then(process_get_block_chain)
            });
        Box::new(fut)
    } else {
        let fut = chain_manager_addr
            .send(epoch_range(epoch, limit, limit_negative))
            .then(process_get_block_chain);
        Box::new(fut)
    }
}

/// Get block by hash
/* test
{"jsonrpc":"2.0","id":1,"method":"getBlock","params":["c0002c6b25615c0f71069f159dffddf8a0b3e529efb054402f0649e969715bdb"]}
{"jsonrpc":"2.0","id":1,"method":"getBlock","params":[{"SHA256":[255,198,135,145,253,40,66,175,226,220,119,243,233,210,25,119,171,217,215,188,185,190,93,116,164,234,217,67,30,102,205,46]}]}
*/
pub fn get_block(hash: Result<(Hash,), jsonrpc_core::Error>) -> JsonRpcResultAsync {
    let hash = match hash {
        Ok(x) => x.0,
        Err(e) => return Box::new(futures::failed(e)),
    };

    let inventory_manager = InventoryManager::from_registry();
    Box::new(
        inventory_manager
            .send(GetItemBlock { hash })
            .then(move |res| match res {
                Ok(Ok(output)) => {
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

                    let txns_hashes = serde_json::json!({
                        "mint" : output.txns.mint.hash(),
                        "value_transfer" : vtt_hashes,
                        "data_request" : drt_hashes,
                        "commit" : ct_hashes,
                        "reveal" : rt_hashes,
                        "tally" : tt_hashes
                    });

                    let mut value = match serde_json::to_value(output) {
                        Ok(x) => x,
                        Err(e) => {
                            let err = internal_error(e);
                            return futures::failed(err);
                        }
                    };

                    value
                        .as_object_mut()
                        .expect("The result of getBlock should be an object")
                        .insert("txns_hashes".to_string(), txns_hashes);

                    futures::finished(value)
                }
                Ok(Err(e)) => {
                    let err = internal_error(e);
                    futures::failed(err)
                }
                Err(e) => {
                    let err = internal_error(e);
                    futures::failed(err)
                }
            }),
    )
}

/// Format of the output of getTransaction
#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GetTransactionOutput {
    /// Transaction
    pub transaction: Transaction,
    /// Hash of the block that contains this transaction in hex format,
    /// or "pending" if the transaction has not been included in any block yet
    pub block_hash: String,
}

/// Get transaction by hash
pub fn get_transaction(hash: Result<(Hash,), jsonrpc_core::Error>) -> JsonRpcResultAsync {
    let hash = match hash {
        Ok(x) => x.0,
        Err(e) => return Box::new(futures::failed(e)),
    };

    let inventory_manager = InventoryManager::from_registry();
    Box::new(
        inventory_manager
            .send(GetItemTransaction { hash })
            .then(move |res| match res {
                Ok(Ok((transaction, pointer_to_block))) => {
                    let output = GetTransactionOutput {
                        transaction,
                        block_hash: pointer_to_block.block_hash.to_string(),
                    };
                    let value = match serde_json::to_value(output) {
                        Ok(x) => x,
                        Err(e) => {
                            let err = internal_error(e);
                            let fut: JsonRpcResultAsync = Box::new(futures::failed(err));
                            return fut;
                        }
                    };
                    let fut: JsonRpcResultAsync = Box::new(futures::finished(value));
                    fut
                }
                Ok(Err(InventoryManagerError::ItemNotFound)) => {
                    let chain_manager = ChainManager::from_registry();
                    Box::new(chain_manager.send(GetMemoryTransaction { hash }).then(
                        |res| match res {
                            Ok(Ok(transaction)) => {
                                let output = GetTransactionOutput {
                                    transaction,
                                    block_hash: "pending".to_string(),
                                };
                                let value = match serde_json::to_value(output) {
                                    Ok(x) => x,
                                    Err(e) => {
                                        let err = internal_error(e);
                                        return futures::failed(err);
                                    }
                                };
                                futures::finished(value)
                            }
                            Ok(Err(())) => {
                                let err = internal_error(InventoryManagerError::ItemNotFound);
                                futures::failed(err)
                            }
                            Err(e) => {
                                let err = internal_error(e);
                                futures::failed(err)
                            }
                        },
                    ))
                }
                Ok(Err(e)) => {
                    let err = internal_error(e);
                    Box::new(futures::failed(err))
                }
                Err(e) => {
                    let err = internal_error(e);
                    Box::new(futures::failed(err))
                }
            }),
    )
}

/*
/// get output
pub fn get_output(output_pointer: Result<(String,), jsonrpc_core::Error>) -> JsonRpcResultAsync {
    let output_pointer = match output_pointer {
        Ok(x) => match OutputPointer::from_str(&x.0) {
            Ok(x) => x,
            Err(e) => {
                let err = internal_error(e);
                return Box::new(futures::failed(err));
            }
        },
        Err(e) => {
            return Box::new(futures::failed(e));
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
                            return futures::failed(err);
                        }
                    };
                    futures::finished(value)
                }
                Ok(Err(e)) => {
                    let err = internal_error(e);
                    futures::failed(err)
                }
                Err(e) => {
                    let err = internal_error(e);
                    futures::failed(err)
                }
            }),
    )
}
*/
/// Build data request transaction
pub fn send_request(params: Result<BuildDrt, jsonrpc_core::Error>) -> JsonRpcResultAsync {
    log::debug!("Creating data request from JSON-RPC.");

    match params {
        Ok(msg) => Box::new(
            ChainManager::from_registry()
                .send(msg)
                .then(|res| match res {
                    Ok(Ok(hash)) => match serde_json::to_value(hash) {
                        Ok(x) => Box::new(futures::finished(x)),
                        Err(e) => {
                            let err = internal_error_s(e);
                            Box::new(futures::failed(err))
                        }
                    },
                    Ok(Err(e)) => {
                        let err = internal_error_s(e);
                        Box::new(futures::failed(err))
                    }
                    Err(e) => {
                        let err = internal_error_s(e);
                        Box::new(futures::failed(err))
                    }
                }),
        ),
        Err(err) => Box::new(futures::failed(err)),
    }
}

/// Build value transfer transaction
pub fn send_value(params: Result<BuildVtt, jsonrpc_core::Error>) -> JsonRpcResultAsync {
    log::debug!("Creating value transfer from JSON-RPC.");

    match params {
        Ok(msg) => Box::new(
            ChainManager::from_registry()
                .send(msg)
                .then(|res| match res {
                    Ok(Ok(hash)) => match serde_json::to_value(hash) {
                        Ok(x) => Box::new(futures::finished(x)),
                        Err(e) => {
                            let err = internal_error_s(e);
                            Box::new(futures::failed(err))
                        }
                    },
                    Ok(Err(e)) => {
                        let err = internal_error_s(e);
                        Box::new(futures::failed(err))
                    }
                    Err(e) => {
                        let err = internal_error_s(e);
                        Box::new(futures::failed(err))
                    }
                }),
        ),
        Err(err) => Box::new(futures::failed(err)),
    }
}

/// Node synchronization status
#[derive(Debug, Default, Deserialize, Serialize)]
pub struct SyncStatus {
    /// The hash of the top consolidated block and the epoch of that block
    pub chain_beacon: CheckpointBeacon,
    /// The current epoch, or None if the epoch 0 is in the future
    pub current_epoch: Option<u32>,
    /// Is the node synchronized?
    pub synchronized: bool,
}

/// Get node status
pub fn status() -> JsonRpcResultAsync {
    let chain_manager = ChainManager::from_registry();
    let epoch_manager = EpochManager::from_registry();

    let synchronized_fut = chain_manager
        .send(GetState)
        .map_err(internal_error_s)
        .then(|res| match res {
            Ok(Ok(StateMachine::Synced)) => Ok(true),
            Ok(Ok(..)) => Ok(false),
            Ok(Err(())) => Err(internal_error(())),
            Err(e) => Err(internal_error(e)),
        });
    let chain_beacon_fut = chain_manager
        .send(GetHighestCheckpointBeacon)
        .then(|res| match res {
            Ok(Ok(x)) => Ok(x),
            Ok(Err(e)) => Err(internal_error_s(e)),
            Err(e) => Err(internal_error_s(e)),
        });

    let current_epoch_fut = epoch_manager.send(GetEpoch).then(|res| match res {
        Ok(Ok(x)) => Ok(Some(x)),
        Ok(Err(EpochManagerError::CheckpointZeroInTheFuture(_))) => Ok(None),
        Ok(Err(e)) => Err(internal_error(e)),
        Err(e) => Err(internal_error_s(e)),
    });

    let j = Future::join3(synchronized_fut, chain_beacon_fut, current_epoch_fut)
        .map(|(synchronized, chain_beacon, current_epoch)| SyncStatus {
            chain_beacon,
            current_epoch,
            synchronized,
        })
        .and_then(|res| match serde_json::to_value(res) {
            Ok(x) => futures::finished(x),
            Err(e) => {
                let err = internal_error_s(e);
                futures::failed(err)
            }
        });

    Box::new(j)
}

/// Get public key
pub fn get_public_key() -> JsonRpcResultAsync {
    let fut = signature_mngr::public_key()
        .map_err(internal_error)
        .map(|pk| {
            log::debug!("{:?}", pk);
            pk.to_bytes().to_vec().into()
        });

    Box::new(fut)
}

/// Get public key hash
pub fn get_pkh() -> JsonRpcResultAsync {
    let fut = signature_mngr::pkh()
        .map_err(internal_error)
        .map(|pkh| Value::String(pkh.to_string()));

    Box::new(fut)
}

/// Sign Data
pub fn sign_data(params: Result<[u8; 32], jsonrpc_core::Error>) -> JsonRpcResultAsync {
    let data = match params {
        Ok(x) => x,
        Err(e) => return Box::new(futures::failed(e)),
    };

    let fut = signature_mngr::sign_data(data)
        .map_err(internal_error)
        .and_then(|ks| match serde_json::to_value(ks) {
            Ok(value) => futures::finished(value),
            Err(e) => {
                let err = internal_error_s(e);
                futures::failed(err)
            }
        });

    Box::new(fut)
}

/// Create VRF
pub fn create_vrf(params: Result<Vec<u8>, jsonrpc_core::Error>) -> JsonRpcResultAsync {
    let data = match params {
        Ok(x) => x,
        Err(e) => return Box::new(futures::failed(e)),
    };

    let fut = signature_mngr::vrf_prove(VrfMessage::set_data(data))
        .map_err(internal_error)
        .map(|(proof, _hash)| {
            log::debug!("{:?}", proof);
            proof.get_proof().into()
        });

    Box::new(fut)
}

/// Data request report
pub fn data_request_report(params: Result<(Hash,), jsonrpc_core::Error>) -> JsonRpcResultAsync {
    let dr_pointer = match params {
        Ok(x) => x.0,
        Err(e) => return Box::new(futures::failed(e)),
    };

    let chain_manager_addr = ChainManager::from_registry();

    let fut = chain_manager_addr
        .send(GetDataRequestReport { dr_pointer })
        .map_err(internal_error)
        .and_then(|dr_info| match dr_info {
            Ok(x) => match serde_json::to_value(&x) {
                Ok(x) => futures::finished(x),
                Err(e) => {
                    let err = internal_error_s(e);
                    futures::failed(err)
                }
            },
            Err(e) => futures::failed(internal_error_s(e)),
        });

    Box::new(fut)
}

/// Get balance
pub fn get_balance(params: Result<(PublicKeyHash,), jsonrpc_core::Error>) -> JsonRpcResultAsync {
    let pkh = match params {
        Ok(x) => x.0,
        Err(e) => return Box::new(futures::failed(e)),
    };

    let chain_manager_addr = ChainManager::from_registry();

    let fut = chain_manager_addr
        .send(GetBalance { pkh })
        .map_err(internal_error)
        .and_then(|dr_info| match dr_info {
            Ok(x) => match serde_json::to_value(&x) {
                Ok(x) => futures::finished(x),
                Err(e) => {
                    let err = internal_error_s(e);
                    futures::failed(err)
                }
            },
            Err(e) => futures::failed(internal_error_s(e)),
        });

    Box::new(fut)
}

/// Get utxos
pub fn get_utxo_info(params: Result<(PublicKeyHash,), jsonrpc_core::Error>) -> JsonRpcResultAsync {
    let chain_manager_addr = ChainManager::from_registry();
    let pkh = match params {
        Ok(x) => x.0,
        Err(e) => return Box::new(futures::failed(e)),
    };

    let fut = chain_manager_addr
        .send(GetUtxoInfo { pkh })
        .map_err(internal_error)
        .and_then(|dr_info| match dr_info {
            Ok(x) => match serde_json::to_value(&x) {
                Ok(x) => futures::finished(x),
                Err(e) => {
                    let err = internal_error_s(e);
                    futures::failed(err)
                }
            },
            Err(e) => futures::failed(internal_error_s(e)),
        });

    Box::new(fut)
}

/// Get Reputation of one pkh
pub fn get_reputation(
    params: Result<(PublicKeyHash,), jsonrpc_core::Error>,
    all: bool,
) -> JsonRpcResultAsync {
    let pkh = match params {
        Ok(x) => x.0,
        Err(e) => return Box::new(futures::failed(e)),
    };

    let chain_manager_addr = ChainManager::from_registry();

    let fut = chain_manager_addr
        .send(GetReputation { pkh, all })
        .map_err(internal_error)
        .and_then(|dr_info| match dr_info {
            Ok(x) => match serde_json::to_value(&x) {
                Ok(x) => futures::finished(x),
                Err(e) => {
                    let err = internal_error_s(e);
                    futures::failed(err)
                }
            },
            Err(e) => futures::failed(internal_error_s(e)),
        });

    Box::new(fut)
}

/// Export private key associated with the node identity
pub fn master_key_export() -> JsonRpcResultAsync {
    let fut = signature_mngr::key_pair().map_err(internal_error).and_then(
        move |(_extended_pk, extended_sk)| {
            let master_path = KeyPath::default();
            let secret_key_hex = extended_sk.to_slip32(&master_path);
            let secret_key_hex = match secret_key_hex {
                Ok(x) => x,
                Err(e) => return futures::failed(internal_error_s(e)),
            };
            match serde_json::to_value(secret_key_hex) {
                Ok(x) => futures::finished(x),
                Err(e) => {
                    let err = internal_error_s(e);
                    futures::failed(err)
                }
            }
        },
    );
    Box::new(fut)
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
pub fn peers() -> JsonRpcResultAsync {
    let sessions_manager_addr = SessionsManager::from_registry();

    let fut = sessions_manager_addr
        .send(GetConsolidatedPeers)
        .map_err(internal_error)
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
                    Ok(x) => futures::finished(x),
                    Err(e) => {
                        let err = internal_error_s(e);
                        futures::failed(err)
                    }
                }
            }
            Err(()) => futures::failed(internal_error(())),
        });

    Box::new(fut)
}

/// Get list of known peers
pub fn known_peers() -> JsonRpcResultAsync {
    let peers_manager_addr = PeersManager::from_registry();

    let fut = peers_manager_addr
        .send(GetKnownPeers)
        .map_err(internal_error)
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
                    Ok(x) => futures::finished(x),
                    Err(e) => {
                        let err = internal_error_s(e);
                        futures::failed(err)
                    }
                }
            }
            Err(e) => futures::failed(internal_error_s(e)),
        });

    Box::new(fut)
}

/// Get the node stats
pub fn node_stats() -> JsonRpcResultAsync {
    let chain_manager_addr = ChainManager::from_registry();

    let fut = chain_manager_addr
        .send(GetNodeStats)
        .map_err(internal_error)
        .and_then(|node_stats| match node_stats {
            Ok(x) => match serde_json::to_value(&x) {
                Ok(x) => futures::finished(x),
                Err(e) => {
                    let err = internal_error_s(e);
                    futures::failed(err)
                }
            },
            Err(e) => futures::failed(internal_error_s(e)),
        });

    Box::new(fut)
}

/// Get all the pending transactions
pub fn get_mempool(params: Result<(), jsonrpc_core::Error>) -> JsonRpcResultAsync {
    match params {
        Ok(()) => (),
        Err(e) => return Box::new(futures::failed(e)),
    };

    let chain_manager_addr = ChainManager::from_registry();

    let fut = chain_manager_addr
        .send(GetMempool)
        .map_err(internal_error)
        .and_then(|dr_info| match dr_info {
            Ok(x) => match serde_json::to_value(&x) {
                Ok(x) => futures::finished(x),
                Err(e) => {
                    let err = internal_error_s(e);
                    futures::failed(err)
                }
            },
            Err(e) => futures::failed(internal_error_s(e)),
        });

    Box::new(fut)
}

/// Add peers
pub fn add_peers(params: Result<Vec<SocketAddr>, jsonrpc_core::Error>) -> JsonRpcResultAsync {
    let addresses = match params {
        Ok(x) => x,
        Err(e) => return Box::new(futures::failed(e)),
    };
    // Use None as the source address: this will make adding peers using this method be exactly the
    // same as adding peers using the configuration file
    let src_address = None;
    let peers_manager_addr = PeersManager::from_registry();

    let fut = peers_manager_addr
        .send(AddPeers {
            addresses,
            src_address,
        })
        .map_err(internal_error)
        .and_then(|res| match res {
            Ok(_overwritten_peers) => {
                // Ignore overwritten peers, just return true on success
                futures::finished(Value::Bool(true))
            }
            Err(e) => futures::failed(internal_error_s(e)),
        });

    Box::new(fut)
}

#[cfg(test)]
mod mock_actix {
    use actix::{MailboxError, Message};
    use futures::Future;

    pub struct Addr;

    impl Addr {
        pub fn send<T: Message>(
            &self,
            _msg: T,
        ) -> impl Future<Item = T::Result, Error = MailboxError> {
            // We cannot test methods which use `send`, so return an error
            futures::failed(MailboxError::Closed)
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
    use futures::sync::mpsc;

    use super::*;
    use std::collections::BTreeSet;
    use witnet_data_structures::{chain::RADRequest, transaction::*};

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
        let response = io.handle_request_sync(&msg, meta);
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
        let response = io.handle_request_sync(&msg, meta);
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
        let response = io.handle_request_sync(&msg, meta);
        assert_eq!(response, Some(expected));
    }

    #[test]
    fn subscribe_new_blocks() {
        // Subscribe to new blocks gives us a SubscriptionId
        let msg = r#"{"jsonrpc":"2.0","method":"witnet_subscribe","params":["newBlocks"],"id":1}"#;
        let expected = r#"{"jsonrpc":"2.0","result":"1","id":1}"#.to_string();
        let subscriptions = Subscriptions::default();
        let (transport_sender, _transport_receiver) = mpsc::channel(0);
        let meta = Arc::new(Session::new(transport_sender));
        let io = jsonrpc_io_handler(subscriptions, true);
        let response = io.handle_request_sync(&msg, meta);
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
        // But first, subscribe to newBlocks
        let msg1 = r#"{"jsonrpc":"2.0","method":"witnet_subscribe","params":["newBlocks"],"id":1}"#;
        let _response1 = io.handle_request_sync(&msg1, meta.clone());
        let response2 = io.handle_request_sync(&msg2, meta);
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
        let response = io.handle_request_sync(&msg, meta);
        assert_eq!(response, Some(expected));
    }

    #[test]
    fn serialize_block() {
        // Check that the serialization of `Block` doesn't change
        use witnet_data_structures::chain::*;
        let block = block_example();
        let inv_elem = InventoryItem::Block(block);
        let s = serde_json::to_string(&inv_elem).unwrap();
        let expected = r#"{"block":{"block_header":{"version":0,"beacon":{"checkpoint":0,"hashPrevBlock":"0000000000000000000000000000000000000000000000000000000000000000"},"merkle_roots":{"mint_hash":"0000000000000000000000000000000000000000000000000000000000000000","vt_hash_merkle_root":"0000000000000000000000000000000000000000000000000000000000000000","dr_hash_merkle_root":"0000000000000000000000000000000000000000000000000000000000000000","commit_hash_merkle_root":"0000000000000000000000000000000000000000000000000000000000000000","reveal_hash_merkle_root":"0000000000000000000000000000000000000000000000000000000000000000","tally_hash_merkle_root":"0000000000000000000000000000000000000000000000000000000000000000"},"proof":{"proof":{"proof":[],"public_key":{"compressed":0,"bytes":[0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0]}}},"bn256_public_key":null},"block_sig":{"signature":{"Secp256k1":{"der":[]}},"public_key":{"compressed":0,"bytes":[0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0]}},"txns":{"mint":{"epoch":0,"outputs":[]},"value_transfer_txns":[],"data_request_txns":[{"body":{"inputs":[{"output_pointer":"0000000000000000000000000000000000000000000000000000000000000000:0"}],"outputs":[{"pkh":"wit1qqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqwrt3a4","value":0,"time_lock":0}],"dr_output":{"data_request":{"time_lock":0,"retrieve":[{"kind":"HTTP-GET","url":"https://openweathermap.org/data/2.5/weather?id=2950159&appid=b6907d289e10d714a6e88b30761fae22","script":[]},{"kind":"HTTP-GET","url":"https://openweathermap.org/data/2.5/weather?id=2950159&appid=b6907d289e10d714a6e88b30761fae22","script":[]}],"aggregate":{"filters":[],"reducer":0},"tally":{"filters":[],"reducer":0}},"witness_reward":0,"witnesses":0,"commit_and_reveal_fee":0,"min_consensus_percentage":0,"collateral":0}},"signatures":[{"signature":{"Secp256k1":{"der":[]}},"public_key":{"compressed":0,"bytes":[0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0]}}]}],"commit_txns":[],"reveal_txns":[],"tally_txns":[]}}}"#;
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
        };

        let rad_retrieve_2 = RADRetrieve {
            kind: RADType::HttpGet,
            url: "https://openweathermap.org/data/2.5/weather?id=2950159&appid=b6907d289e10d714a6e88b30761fae22".to_string(),
            script: vec![0],
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
                "createVRF",
                "dataRequestReport",
                "getBalance",
                "getBlock",
                "getBlockChain",
                "getMempool",
                "getPkh",
                "getPublicKey",
                "getReputation",
                "getReputationAll",
                "getTransaction",
                "getUtxoInfo",
                "inventory",
                "knownPeers",
                "masterKeyExport",
                "nodeStats",
                "peers",
                "sendRequest",
                "sendValue",
                "sign",
                "syncStatus",
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
            "createVRF",
            "getPkh",
            "getPublicKey",
            "getUtxoInfo",
            "masterKeyExport",
            "sendRequest",
            "sendValue",
            "sign",
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
