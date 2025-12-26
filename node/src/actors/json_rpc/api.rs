use actix::MailboxError;
#[cfg(not(test))]
use actix::SystemService;
use futures::FutureExt;
use itertools::Itertools;
use jsonrpc_core::{BoxFuture, Error, Params, Value};
use jsonrpc_pubsub::{Subscriber, SubscriptionId};
use serde::{Deserialize, Serialize};
use std::{
    collections::{HashMap, HashSet, hash_map::Entry},
    convert::TryFrom,
    fmt::Debug,
    net::SocketAddr,
    path::PathBuf,
    str::FromStr,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
};

#[cfg(test)]
use self::mock_actix::SystemService;
use crate::{
    actors::{
        chain_manager::{ChainManager, ChainManagerError, run_dr_locally},
        epoch_manager::{EpochManager, EpochManagerError},
        inventory_manager::{InventoryManager, InventoryManagerError},
        json_rpc::Subscriptions,
        messages::{
            AddCandidates, AddPeers, AddTransaction, AuthorizeStake, BuildDrt, BuildStake,
            BuildStakeParams, BuildStakeResponse, BuildUnstake, BuildUnstakeParams, BuildVtt,
            ClearPeers, DropAllPeers, EstimatePriority, GetBalance, GetBalance2, GetBalance2Limits,
            GetBalanceTarget, GetBlocksEpochRange, GetConsolidatedPeers, GetDataRequestInfo,
            GetEpoch, GetEpochConstants, GetHighestCheckpointBeacon, GetItemBlock,
            GetItemSuperblock, GetItemTransaction, GetKnownPeers, GetMemoryTransaction, GetMempool,
            GetNodeStats, GetProtocolInfo, GetReputation, GetSignalingInfo, GetState,
            GetSupplyInfo, GetSupplyInfo2, GetUtxoInfo, InitializePeers, IsConfirmedBlock,
            MagicEither, QueryStakes, QueryStakingPowers, Rewind, SearchDataRequests,
            SnapshotExport, SnapshotImport, StakeAuthorization,
        },
        peers_manager::PeersManager,
        sessions_manager::SessionsManager,
    },
    config_mngr, signature_mngr,
    utils::Force,
};
use witnet_crypto::{hash::calculate_sha256, key::KeyPath};
use witnet_data_structures::{
    chain::{
        Block, DataRequestInfo, DataRequestOutput, DataRequestStage, Epoch, EpochConstants, Hash,
        Hashable, KeyedSignature, PublicKeyHash, RADType, StakeOutput, StateMachine, SyncStatus,
        ValueTransferOutput, tapi::ActiveWips,
    },
    get_environment, get_protocol_version,
    proto::{
        ProtobufConvert,
        versioning::{ProtocolVersion, VersionedHashable},
    },
    serialization_helpers::number_from_string,
    staking::prelude::*,
    transaction::{Transaction, VTTransaction},
    vrf::VrfMessage,
};

type JsonRpcResult = jsonrpc_core::Result<jsonrpc_core::Value>;

/// Guard methods that assume that JSON-RPC is only accessible by the owner of the node.
///
/// A method is sensitive if it touches in some way the master key of the node or leaks addresses.
/// For example: methods that can be used to create transactions (spending value from this node),
/// sign arbitrary messages with the node master key, or even export the master key.
async fn if_authorized<F, Fut>(
    enable_sensitive_methods: bool,
    method_name: &str,
    params: Params,
    method: F,
) -> JsonRpcResult
where
    F: FnOnce(Params) -> Fut,
    Fut: futures::Future<Output = JsonRpcResult>,
{
    if enable_sensitive_methods {
        method(params).await
    } else {
        Err(internal_error_s(unauthorized_message(method_name)))
    }
}

/// Attach the regular JSON-RPC methods to a multi-transport server.
pub fn attach_regular_methods<H>(
    server: &mut impl witty_jsonrpc::server::ActixServer<H>,
    sensitive_methods_enabled: bool,
    system: &Option<actix::System>,
) where
    H: witty_jsonrpc::handler::Handler,
{
    server.add_actix_method(system, "inventory", |params: Params| {
        Box::pin(inventory(params.parse()))
    });
    server.add_actix_method(system, "getBlockChain", |params: Params| {
        Box::pin(get_block_chain(params.parse()))
    });
    server.add_actix_method(system, "getBlock", |params: Params| {
        Box::pin(get_block(params))
    });
    server.add_actix_method(system, "getDataRequest", |params: Params| {
        Box::pin(get_data_request(params.parse()))
    });
    server.add_actix_method(system, "getTransaction", |params: Params| {
        Box::pin(get_transaction(params.parse()))
    });
    server.add_actix_method(system, "getValueTransfer", |params: Params| {
        Box::pin(get_value_transfer(params.parse()))
    });
    server.add_actix_method(system, "syncStatus", |_params: Params| Box::pin(status()));
    server.add_actix_method(system, "dataRequestReport", |params: Params| {
        Box::pin(data_request_report(params.parse()))
    });
    server.add_actix_method(system, "getBalance", move |params: Params| {
        Box::pin(get_balance(params, sensitive_methods_enabled))
    });
    server.add_actix_method(system, "getBalance2", |params: Params| {
        Box::pin(get_balance_2(params.parse()))
    });
    server.add_actix_method(system, "getReputation", |params: Params| {
        Box::pin(get_reputation(params.parse(), false))
    });
    server.add_actix_method(system, "getReputationAll", |_params: Params| {
        Box::pin(get_reputation(Ok((PublicKeyHash::default(),)), true))
    });
    server.add_actix_method(system, "getSupplyInfo", |_params: Params| {
        Box::pin(get_supply_info())
    });
    server.add_actix_method(system, "peers", |_params: Params| Box::pin(peers()));
    server.add_actix_method(system, "knownPeers", |_params: Params| {
        Box::pin(known_peers())
    });
    server.add_actix_method(
        system,
        "nodeStats",
        |_params: Params| Box::pin(node_stats()),
    );
    server.add_actix_method(system, "getMempool", |params: Params| {
        Box::pin(get_mempool(params.parse()))
    });
    server.add_actix_method(system, "getConsensusConstants", |params: Params| {
        Box::pin(get_consensus_constants(params.parse()))
    });
    server.add_actix_method(system, "getSuperblock", |params: Params| {
        Box::pin(get_superblock(params.parse()))
    });
    server.add_actix_method(system, "signalingInfo", |_params: Params| {
        Box::pin(signaling_info())
    });
    server.add_actix_method(system, "priority", |_params: Params| Box::pin(priority()));
    server.add_actix_method(system, "protocol", |_params: Params| Box::pin(protocol()));
    server.add_actix_method(system, "queryStakes", |params: Params| {
        Box::pin(query_stakes(params.parse()))
    });
    server.add_actix_method(system, "queryPowers", |params: Params| {
        Box::pin(query_powers(params.parse()))
    });
    server.add_actix_method(system, "getUtxoInfo", move |params: Params| {
        Box::pin(get_utxo_info(params.parse()))
    });
    server.add_actix_method(system, "searchDataRequests", |params: Params| {
        Box::pin(search_data_requests(params.parse()))
    });
}

/// Attach the sensitive JSON-RPC methods to a multi-transport server.
pub fn attach_sensitive_methods<H>(
    server: &mut impl witty_jsonrpc::server::ActixServer<H>,
    enable_sensitive_methods: bool,
    system: &Option<actix::System>,
) where
    H: witty_jsonrpc::handler::Handler,
{
    server.add_actix_method(system, "sendRequest", move |params| {
        Box::pin(if_authorized(
            enable_sensitive_methods,
            "sendRequest",
            params,
            |params| send_request(params.parse()),
        ))
    });
    server.add_actix_method(system, "tryRequest", move |params| {
        Box::pin(if_authorized(
            enable_sensitive_methods,
            "tryRequest",
            params,
            |params| try_request(params.parse()),
        ))
    });
    server.add_actix_method(system, "sendValue", move |params| {
        Box::pin(if_authorized(
            enable_sensitive_methods,
            "sendValue",
            params,
            |params| send_value(params.parse()),
        ))
    });
    server.add_actix_method(system, "getPublicKey", move |params| {
        Box::pin(if_authorized(
            enable_sensitive_methods,
            "getPublicKey",
            params,
            |_params| get_public_key(),
        ))
    });
    server.add_actix_method(system, "getPkh", move |params| {
        Box::pin(if_authorized(
            enable_sensitive_methods,
            "getPkh",
            params,
            |_params| get_pkh(),
        ))
    });
    server.add_actix_method(system, "sign", move |params| {
        Box::pin(if_authorized(
            enable_sensitive_methods,
            "sign",
            params,
            |params| sign_data(params.parse()),
        ))
    });
    server.add_actix_method(system, "createVRF", move |params| {
        Box::pin(if_authorized(
            enable_sensitive_methods,
            "createVRF",
            params,
            |params| create_vrf(params.parse()),
        ))
    });
    server.add_actix_method(system, "masterKeyExport", move |params| {
        Box::pin(if_authorized(
            enable_sensitive_methods,
            "masterKeyExport",
            params,
            |_params| master_key_export(),
        ))
    });
    server.add_actix_method(system, "addPeers", move |params| {
        Box::pin(if_authorized(
            enable_sensitive_methods,
            "addPeers",
            params,
            |params| add_peers(params.parse()),
        ))
    });
    server.add_actix_method(system, "clearPeers", move |params| {
        Box::pin(if_authorized(
            enable_sensitive_methods,
            "clearPeers",
            params,
            |_params| clear_peers(),
        ))
    });
    server.add_actix_method(system, "initializePeers", move |params| {
        Box::pin(if_authorized(
            enable_sensitive_methods,
            "initializePeers",
            params,
            |_params| initialize_peers(),
        ))
    });
    server.add_actix_method(system, "rewind", move |params| {
        Box::pin(if_authorized(
            enable_sensitive_methods,
            "rewind",
            params,
            |params| rewind(params.parse()),
        ))
    });
    server.add_actix_method(system, "chainExport", move |params| {
        Box::pin(if_authorized(
            enable_sensitive_methods,
            "chainExport",
            params,
            |params| snapshot_export(params.parse()),
        ))
    });
    server.add_actix_method(system, "chainImport", move |params| {
        Box::pin(if_authorized(
            enable_sensitive_methods,
            "chainImport",
            params,
            |params| snapshot_import(params.parse()),
        ))
    });
    server.add_actix_method(system, "stake", move |params| {
        Box::pin(if_authorized(
            enable_sensitive_methods,
            "stake",
            params,
            |params| stake(params.parse()),
        ))
    });
    server.add_actix_method(system, "authorizeStake", move |params: Params| {
        Box::pin(if_authorized(
            enable_sensitive_methods,
            "authorizeStake",
            params,
            |params| authorize_stake(params.parse()),
        ))
    });
    server.add_actix_method(system, "unstake", move |params| {
        Box::pin(if_authorized(
            enable_sensitive_methods,
            "unstake",
            params,
            |params| unstake(params.parse()),
        ))
    });
    server.add_actix_method(system, "getSupplyInfo2", move |params| {
        Box::pin(if_authorized(
            enable_sensitive_methods,
            "getSupplyInfo2",
            params,
            |_params| get_supply_info_2(),
        ))
    });
}

fn extract_topic_and_params(params: Params) -> Result<(String, Value), Error> {
    if let Params::Array(values) = params {
        let mut values_iter = values.into_iter();
        // Try to read a first param that must be a String representing the topic
        let topic = String::from(values_iter
            .next()
            .ok_or(Error::invalid_params(
            "At least one subscription param is expected, representing the topic to subscribe to",
        ))?.as_str().ok_or(Error::invalid_params("The first subscription param is expected to be a String representing the topic to subscribe to"))?);
        // Try to read a second param that will be used as the topic param (defaults to Null)
        let params = values_iter.next().unwrap_or(Value::Null);

        Ok((topic, params))
    } else {
        Err(Error::invalid_params(
            "Subscription params must be an Array",
        ))
    }
}

/// Attach the JSON-RPC subscriptions to a multi-transport server.
pub fn attach_subscriptions<H>(
    server: &mut impl witty_jsonrpc::server::ActixServer<H>,
    subscriptions: Subscriptions,
    system: &Option<actix::System>,
) where
    H: witty_jsonrpc::handler::Handler,
{
    // Atomic counter that keeps track of the next subscriber ID
    let atomic_counter = AtomicUsize::new(1);

    // Cloned the subscriptions for reuse in subscribe / unsubscribe closures
    let cloned_subscriptions = subscriptions.clone();

    // Wrapped in `Arc` for spawning the subscriptions in other threads safely
    let subscribe_arc = Arc::new(
        move |params: Params, meta: H::Metadata, subscriber: Subscriber| {
            log::debug!("Called subscribe method with params {params:?} and meta {meta:?}");

            // Main business logic for registering a new subscription
            let register = |topic: String, params: Value, subscriber: Subscriber| {
                if let Ok(mut all_subscriptions) = cloned_subscriptions.lock() {
                    // Generate a subscription ID and assign it to the subscriber, getting a sink
                    let id = SubscriptionId::String(
                        atomic_counter.fetch_add(1, Ordering::SeqCst).to_string(),
                    );
                    let sink = subscriber.assign_id(id.clone());

                    // If we got a sink, we can register the subscription
                    if let Ok(sink) = sink {
                        let topic_subscriptions = all_subscriptions
                            .entry(topic.clone())
                            .or_insert_with(HashMap::new);
                        topic_subscriptions.insert(id, (sink, params));

                        log::debug!(
                            "Subscribed to topic '{}'. There are {} subscriptions to this topic",
                            topic,
                            topic_subscriptions.len()
                        );
                    } else {
                        // If we couldn't get a sync, it's probably because the session closed before we
                        // got a chance to reply
                        log::debug!("Failed to assign id: session closed");
                    }
                } else {
                    log::error!("Failed to acquire lock on subscriptions Arc");
                    subscriber
                        .reject(internal_error_s(
                            "failed to acquire lock on subscriptions Arc",
                        ))
                        .ok();
                };
            };

            // Try to extract the method name and true params from raw JSON-RPC params
            match extract_topic_and_params(params) {
                Ok((topic, params)) => {
                    // Deal with supported / unsupported subscription methods
                    match topic.as_str() {
                        "blocks" | "superblocks" | "status" => {
                            // If using a supported topic, register the subscription
                            register(topic, params, subscriber);
                        }
                        other => {
                            // If the topic is unknown, reject the subscription
                            log::error!(
                                "got a subscription request for an unsupported topic: {other}"
                            );

                            subscriber
                                .reject(Error::invalid_params_with_details(
                                    "unknown subscription topic",
                                    other,
                                ))
                                .ok();
                        }
                    }
                }
                Err(error) => {
                    // Upon any error, reject the subscriber and exit
                    subscriber.reject(error).ok();
                }
            }
        },
    );

    // Wrapped in `Arc` for spawning the unsubscriptions in other threads safely
    let unsubscribe_arc = Arc::new(
        move |id: SubscriptionId, meta: Option<H::Metadata>| -> BoxFuture<JsonRpcResult> {
            log::debug!("called unsubscribe method for id {id:?} with meta {meta:?}");

            match subscriptions.lock() {
                Ok(mut subscriptions) => {
                    let mut found = false;
                    for (_topic, subscribers) in subscriptions.iter_mut() {
                        if subscribers.remove(&id).is_some() {
                            found = true;
                            // Each id can only appear once because subscriptions to multiple topics
                            // from the same client will have different IDs.
                            break;
                        }
                    }

                    Box::pin(futures::future::ok(Value::from(found)))
                }
                Err(error) => {
                    log::error!("Failed to acquire lock on subscriptions Arc");
                    Box::pin(futures::future::err(internal_error(error)))
                }
            }
        },
    );

    server.add_actix_subscription(
        system,
        "witnet_subscription",
        ("witnet_subscribe", subscribe_arc),
        ("witnet_unsubscribe", unsubscribe_arc),
    );
}

/// Attach the whole Node API to a multi-transport server, including regular and sensistive methods,
/// as well as subscriptions.
pub fn attach_api<H>(
    server: &mut impl witty_jsonrpc::server::ActixServer<H>,
    enable_sensitive_methods: bool,
    subscriptions: Subscriptions,
    system: &Option<actix::System>,
) where
    H: witty_jsonrpc::handler::Handler,
{
    attach_regular_methods(server, enable_sensitive_methods, system);
    attach_sensitive_methods(server, enable_sensitive_methods, system);
    attach_subscriptions(server, subscriptions, system);
}

fn internal_error<T: std::fmt::Debug>(e: T) -> jsonrpc_core::Error {
    jsonrpc_core::Error {
        code: jsonrpc_core::ErrorCode::InternalError,
        message: format!("{e:?}"),
        data: None,
    }
}

fn internal_error_s<T: std::fmt::Display>(e: T) -> jsonrpc_core::Error {
    jsonrpc_core::Error {
        code: jsonrpc_core::ErrorCode::InternalError,
        message: format!("{e}"),
        data: None,
    }
}

/// Message that appears when calling a sensitive method when sensitive methods are disabled
fn unauthorized_message(method_name: &str) -> String {
    format!(
        "Method {method_name} not allowed while node setting json_rpc.enable_sensitive_methods is set to false"
    )
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
pub async fn inventory(params: Result<InventoryItem, Error>) -> JsonRpcResult {
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
            log::debug!("Invalid type of inventory item from JSON-RPC: {inv_elem:?}");
            Err(Error::invalid_params("Item type not implemented"))
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
pub async fn get_block_chain(params: Result<Option<GetBlockChainParams>, Error>) -> JsonRpcResult {
    // Helper function to convert the result of GetBlockEpochRange to a JSON value, or a JSON-RPC error
    async fn process_get_block_chain(
        res: Result<Result<Vec<(u32, Hash)>, ChainManagerError>, MailboxError>,
    ) -> JsonRpcResult {
        match res {
            Ok(Ok(vec_inv_entry)) => {
                let epoch_and_hash: Vec<_> = vec_inv_entry
                    .into_iter()
                    .map(|(epoch, hash)| {
                        let hash_string = format!("{hash}");
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
                u32::MAX,
                u32::MAX
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
pub async fn get_block(params: Params) -> Result<Value, Error> {
    let (block_hash, include_txns_metadata): (Hash, bool);

    // Handle parameters as an array with a first obligatory hash field plus an optional bool field
    if let Params::Array(params) = params {
        if let Some(Value::String(hash)) = params.first() {
            match hash.parse() {
                Ok(hash) => block_hash = hash,
                Err(e) => {
                    return Err(internal_error(e));
                }
            }
        } else {
            return Err(Error::invalid_params(
                "First argument of `get_block` must have type `Hash`",
            ));
        };

        match params.get(1) {
            None => include_txns_metadata = true,
            Some(Value::Bool(ith)) => include_txns_metadata = *ith,
            Some(_) => {
                return Err(Error::invalid_params(
                    "Second argument of `get_block` must have type `Bool`",
                ));
            }
        };
    } else {
        return Err(Error::invalid_params(
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
            let block_hash = output.versioned_hash(get_protocol_version(Some(block_epoch)));

            let dr_weight = match serde_json::to_value(output.dr_weight()) {
                Ok(x) => x,
                Err(e) => {
                    let err = internal_error(e);
                    return Err(err);
                }
            };

            let vt_weight = match serde_json::to_value(output.vt_weight()) {
                Ok(x) => x,
                Err(e) => {
                    let err = internal_error(e);
                    return Err(err);
                }
            };

            let block_weight = match serde_json::to_value(output.weight()) {
                Ok(x) => x,
                Err(e) => {
                    let err = internal_error(e);
                    return Err(err);
                }
            };

            // Only include the `txns_hashes` field if explicitly requested, as hash
            // operations are quite expensive, and transactions read from storage cannot
            // benefit from hash memoization
            let txns_hashes = if include_txns_metadata {
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

                let mut hashes = Some(serde_json::json!({
                    "mint" : output.txns.mint.hash(),
                    "value_transfer" : vtt_hashes,
                    "data_request" : drt_hashes,
                    "commit" : ct_hashes,
                    "reveal" : rt_hashes,
                    "tally" : tt_hashes
                }));

                if ProtocolVersion::from_epoch(block_epoch) >= ProtocolVersion::V1_8 {
                    let st_hashes: Vec<_> = output
                        .txns
                        .stake_txns
                        .iter()
                        .map(|txn| txn.hash())
                        .collect();
                    if let Some(ref mut hashes) = hashes {
                        hashes
                            .as_object_mut()
                            .expect("The result of getBlock should be an object")
                            .insert("stake".to_string(), serde_json::json!(st_hashes));
                    }
                }

                if ProtocolVersion::from_epoch(block_epoch) == ProtocolVersion::V2_0 {
                    let ut_hashes: Vec<_> = output
                        .txns
                        .unstake_txns
                        .iter()
                        .map(|txn| txn.hash())
                        .collect();
                    if let Some(ref mut hashes) = hashes {
                        hashes
                            .as_object_mut()
                            .expect("The result of getBlock should be an object")
                            .insert("unstake".to_string(), serde_json::json!(ut_hashes));
                    }
                }

                hashes
            } else {
                None
            };

            // Only include the `txns_weights` field if explicitly requested
            let txns_weights = if include_txns_metadata {
                let vtt_weights: Vec<_> = output
                    .txns
                    .value_transfer_txns
                    .iter()
                    .map(|txn| txn.weight())
                    .collect();
                let drt_weights: Vec<_> = output
                    .txns
                    .data_request_txns
                    .iter()
                    .map(|txn| txn.weight())
                    .collect();

                let mut weights = Some(serde_json::json!({
                    "value_transfer": vtt_weights,
                    "data_request": drt_weights,
                }));

                if ProtocolVersion::from_epoch(block_epoch) >= ProtocolVersion::V1_8 {
                    let st_weights: Vec<_> = output
                        .txns
                        .stake_txns
                        .iter()
                        .map(|txn| txn.weight())
                        .collect();
                    if let Some(ref mut weights) = weights {
                        weights
                            .as_object_mut()
                            .expect("The result of getBlock should be an object")
                            .insert("stake".to_string(), st_weights.into());
                    }
                }

                if ProtocolVersion::from_epoch(block_epoch) >= ProtocolVersion::V2_0 {
                    let ut_weights: Vec<_> = output
                        .txns
                        .unstake_txns
                        .iter()
                        .map(|txn| txn.weight())
                        .collect();
                    if let Some(ref mut weights) = weights {
                        weights
                            .as_object_mut()
                            .expect("The result of getBlock should be an object")
                            .insert("unstake".to_string(), ut_weights.into());
                    }
                }
                weights
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
            // See explanation above about optional `txns_weights` field
            if let Some(txns_weights) = txns_weights {
                value
                    .as_object_mut()
                    .expect("The result of getBlock should be an object")
                    .insert("txns_weights".to_string(), txns_weights);
            }

            value
                .as_object_mut()
                .expect("The result of getBlock should be an object")
                .insert("dr_weight".to_string(), dr_weight);

            value
                .as_object_mut()
                .expect("The result of getBlock should be an object")
                .insert("vt_weight".to_string(), vt_weight);

            value
                .as_object_mut()
                .expect("The result of getBlock should be an object")
                .insert("block_weight".to_string(), block_weight);

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
    /// Epoch of the block that contains this transaction, or None if the transaction has not been
    /// included in any block yet
    pub block_epoch: Option<Epoch>,
    /// Hash of the block that contains this transaction in hex format,
    /// or "pending" if the transaction has not been included in any block yet
    pub block_hash: String,
    /// Timestamp of the block that contains this transaction, or None not included yet
    pub block_timestamp: Option<i64>,
    /// True if the block that includes this transaction has been confirmed by a superblock
    pub confirmed: bool,
    /// Number of epochs since this transaction got included in a block
    pub confirmations: Option<u32>,
}

/// Get transaction by hash
pub async fn get_transaction(hash: Result<(Hash,), Error>) -> JsonRpcResult {
    let hash = match hash {
        Ok(x) => x.0,
        Err(e) => return Err(e),
    };

    // Get current epoch
    let current_epoch = match EpochManager::from_registry().send(GetEpoch).await {
        Ok(Ok(current_epoch)) => current_epoch,
        Ok(Err(e)) => return Err(internal_error(e)),
        Err(e) => return Err(internal_error(e)),
    };

    // Get epoch constants
    let epoch_constants = match EpochManager::from_registry().send(GetEpochConstants).await {
        Ok(Some(epoch_constats)) => epoch_constats,
        Err(e) => return Err(internal_error(e)),
        _ => return Err(internal_error("")),
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

            let block_timestamp = match epoch_constants.epoch_timestamp(block_epoch) {
                Err(err) => return Err(internal_error(err)),
                Ok((timestamp, _)) => timestamp,
            };

            let output = GetTransactionOutput {
                transaction: new_transaction,
                weight,
                block_hash: block_hash.to_string(),
                block_epoch: Some(block_epoch),
                block_timestamp: Some(block_timestamp),
                confirmed,
                confirmations: Some(current_epoch.saturating_sub(block_epoch)),
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
                        block_timestamp: None,
                        confirmed: false,
                        confirmations: None,
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
pub fn get_output(output_pointer: Result<(String,), Error>) -> JsonRpcResult {
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
pub async fn send_request(params: Result<BuildDrt, Error>) -> JsonRpcResult {
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
pub async fn try_request(params: Result<DataRequestOutput, Error>) -> JsonRpcResult {
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
pub async fn send_value(params: Result<BuildVtt, Error>) -> JsonRpcResult {
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
                log::debug!("{pk:?}");
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
pub async fn sign_data(params: Result<[u8; 32], Error>) -> JsonRpcResult {
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
pub async fn create_vrf(params: Result<Vec<u8>, Error>) -> JsonRpcResult {
    let data = match params {
        Ok(x) => x,
        Err(e) => return Err(e),
    };

    signature_mngr::vrf_prove(VrfMessage::set_data(data))
        .map(|res| {
            res.map_err(internal_error).map(|(proof, _hash)| {
                log::debug!("{proof:?}");
                proof.get_proof().into()
            })
        })
        .await
}

/// Data request info
pub async fn data_request_report(params: Result<(Hash,), Error>) -> JsonRpcResult {
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

/// Params of getBalance method
#[derive(Debug, Default, Deserialize, Serialize)]
pub struct GetBalanceParams {
    /// Query the balance of a specific address. If not provided, will default to our own address,
    /// unless `all` is set.
    #[serde(default)] // default to None
    pub pkh: Option<PublicKeyHash>,
    /// Distinguish between fetching a simple balance or fetching confirmed and unconfirmed balance.
    #[serde(default)] // default to false
    pub simple: bool,
    /// Query the balance of all the existing addresses.
    #[serde(default)] // default to false
    pub all: bool,
}

/// Easily derive the target of the getBalance command from its arguments.
impl From<GetBalanceParams> for GetBalanceTarget {
    fn from(params: GetBalanceParams) -> Self {
        if params.all {
            GetBalanceTarget::All
        } else {
            params.pkh.into()
        }
    }
}

/// Get balance
pub async fn get_balance(params: Params, sensitive_methods_enabled: bool) -> JsonRpcResult {
    let (target, simple): (GetBalanceTarget, bool);

    // Handle parameters as an array with a first obligatory PublicKeyHash field plus an optional bool field
    if let Params::Array(params) = params {
        if let Some(Value::String(target_param)) = params.first() {
            target = GetBalanceTarget::from_str(target_param).map_err(internal_error)?;
        } else {
            return Err(Error::invalid_params(
                "First argument of `get_balance` must have type `PublicKeyHash`",
            ));
        };

        simple = params.get(1).and_then(Value::as_bool).unwrap_or(false);
    } else {
        let params: GetBalanceParams = params.parse()?;
        simple = params.simple;
        target = params.into();
    };

    if target == GetBalanceTarget::Own && !sensitive_methods_enabled {
        return Err(Error::invalid_params(
            "Providing own node's balance is not allowed when sensitive methods are disabled",
        ));
    };

    let chain_manager_addr = ChainManager::from_registry();
    chain_manager_addr
        .send(GetBalance { target, simple })
        .map(|res| {
            res.map_err(internal_error)
                .and_then(|dr_info| match dr_info {
                    Ok(x) => match serde_json::to_value(x) {
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
                    Ok(x) => match serde_json::to_value(x) {
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

/// Get relevant supply info after V1_8 activation
pub async fn get_supply_info_2() -> JsonRpcResult {
    let chain_manager_addr = ChainManager::from_registry();

    chain_manager_addr
        .send(GetSupplyInfo2)
        .map(|res| {
            res.map_err(internal_error)
                .and_then(|supply_info| match supply_info {
                    Ok(x) => match serde_json::to_value(x) {
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

/// Optional having-filter when getting utxo info for specified address
#[derive(Clone, Debug, Default, Hash, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UtxoFilter {
    /// Minimum value
    #[serde(deserialize_with = "number_from_string")]
    pub min_value: u64,

    /// Signer's public key hash
    pub from_signer: Option<PublicKeyHash>,
}

/// Get utxos
pub async fn get_utxo_info(params: Result<Params, Error>) -> JsonRpcResult {
    if let Params::Array(values) = params? {
        // Try to read a first param that must be a String representing a bech32 address
        let pkh = values.first().ok_or(Error::invalid_params(
            "First argument must refer to a valid Bech32 address",
        ))?;
        let pkh = PublicKeyHash::deserialize(pkh).map_err(internal_error)?;

        // Try to read a second param. If present, we try to deserialize it as a UtxoFilter
        let filter = if let Some(value) = values.get(1) {
            Some(UtxoFilter::deserialize(value).map_err(internal_error)?)
        } else {
            None
        };

        let chain_manager_addr = ChainManager::from_registry();
        let mut utxos_info = chain_manager_addr
            .send(GetUtxoInfo { pkh })
            .await
            .map_err(internal_error)?
            .map_err(internal_error)?;

        // If a value filter is applied, get rid of utxos smaller than the required value
        if let Some(UtxoFilter {
            min_value,
            from_signer: _,
        }) = filter
        {
            utxos_info.utxos.retain(|utxo| utxo.value >= min_value);
        }

        // If a signer filter is applied, get rid of utxos not created by the specified signer address
        if let Some(UtxoFilter {
            min_value: _,
            from_signer: Some(from_signer),
        }) = filter
        {
            let inventory_manager = InventoryManager::from_registry();
            let futures = utxos_info.utxos.iter().map(|utxo| {
                inventory_manager.send(GetItemTransaction {
                    hash: utxo.output_pointer.transaction_id,
                })
            });
            // The strategy here is to first create a set containing only the hashes of the transactions
            // that were signed by the specified signer address
            let filtered_hashes = futures::future::join_all(futures)
                .await
                .into_iter()
                .flatten()
                .flatten()
                .filter_map(|(transaction, _, _)| {
                    if transaction
                        .signatures()
                        .iter()
                        .any(|signature| signature.public_key.pkh().eq(&from_signer))
                    {
                        Some(transaction.hash())
                    } else {
                        None
                    }
                })
                .collect::<HashSet<Hash>>();

            // And then we get rid of the utxos that don't point to any of those transactions
            utxos_info
                .utxos
                .retain(|utxo| filtered_hashes.contains(&utxo.output_pointer.transaction_id));
        }

        serde_json::to_value(utxos_info).map_err(internal_error_s)
    } else {
        Err(Error::invalid_params("Expected an array of arguments"))
    }
}

/// Get Reputation of one pkh
pub async fn get_reputation(params: Result<(PublicKeyHash,), Error>, all: bool) -> JsonRpcResult {
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
                    Ok(x) => match serde_json::to_value(x) {
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

                        match serde_json::to_value(peers) {
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

                        match serde_json::to_value(peers) {
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
                    Ok(x) => match serde_json::to_value(x) {
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
pub async fn get_mempool(params: Result<(), Error>) -> JsonRpcResult {
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
                    Ok(x) => match serde_json::to_value(x) {
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
pub async fn add_peers(params: Result<Vec<SocketAddr>, Error>) -> JsonRpcResult {
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
pub async fn get_consensus_constants(params: Result<(), Error>) -> JsonRpcResult {
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
pub async fn rewind(params: Result<(Epoch,), Error>) -> JsonRpcResult {
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
                    Ok(x) => match serde_json::to_value(x) {
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
pub async fn get_superblock(params: Result<GetSuperblockBlocksParams, Error>) -> JsonRpcResult {
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
/// Get the list of protocol upgrades that are already active and the ones
/// that are currently being polled for activation signaling
pub async fn signaling_info() -> JsonRpcResult {
    let chain_manager_addr = ChainManager::from_registry();

    let info = chain_manager_addr
        .send(GetSignalingInfo {})
        .await
        .map_err(internal_error)?;

    match info {
        Ok(x) => match serde_json::to_value(x) {
            Ok(x) => Ok(x),
            Err(e) => {
                let err = internal_error_s(e);
                Err(err)
            }
        },
        Err(e) => Err(internal_error_s(e)),
    }
}

/// Get priority and time-to-block estimations for different priority tiers.
pub async fn priority() -> JsonRpcResult {
    let chain_manager_addr = ChainManager::from_registry();
    let response = chain_manager_addr.send(EstimatePriority {}).await;
    let estimate = response
        .map_err(internal_error_s)?
        .map_err(internal_error_s)?;

    serde_json::to_value(estimate).map_err(internal_error_s)
}

/// Get information about protocol versions and which version is currently being enforced.
pub async fn protocol() -> JsonRpcResult {
    let chain_manager_addr = ChainManager::from_registry();
    let response = chain_manager_addr.send(GetProtocolInfo {}).await;
    let protocol_info = response
        .map_err(internal_error_s)?
        .map_err(internal_error_s)?;

    serde_json::to_value(protocol_info).map_err(internal_error_s)
}

/// Parameters of snapshot_export
#[derive(Debug, Deserialize)]
pub struct SnapshotExportParams {
    /// The path where the chain state snapshot should be written to.
    pub path: Option<PathBuf>,
    /// Whether to force the export regardless of the synchronization status.
    pub force: Option<bool>,
}

impl From<SnapshotExportParams> for Force<PathBuf> {
    fn from(params: SnapshotExportParams) -> Self {
        match (params.path, params.force) {
            (Some(path), Some(true)) => Force::All(path),
            (Some(path), _) => Force::Some(path),
            (_, Some(true)) => Force::All(witnet_config::dirs::data_dir()),
            _ => Force::Some(witnet_config::dirs::data_dir()),
        }
    }
}

/// Export a snapshot of the current chain state.
///
/// This method is intended for fast syncing nodes from snapshot files that can downloaded over
/// HTTP, FTP, Torrent or IPFS.
pub async fn snapshot_export(params: Result<SnapshotExportParams, Error>) -> JsonRpcResult {
    // Use path from parameters if provided, otherwise try to guess path from configuration
    let path = Force::from(params?);

    // Tell the chain manager to create and export the snapshot
    let chain_manager = ChainManager::from_registry();
    let actor_response = chain_manager.send(SnapshotExport { path }).await;
    let response = actor_response
        .map_err(internal_error_s)?
        .map_err(internal_error_s)?;

    // Write the response back (the path to the snapshot file)
    serde_json::to_value(response).map_err(internal_error_s)
}

/// Parameters of snapshot_import
#[derive(Debug, Deserialize)]
pub struct SnapshotImportParams {
    /// The path to the chain state snapshot file.
    pub path: Option<PathBuf>,
    /// Whether to force the import regardless of the synchronization status.
    pub force: Option<bool>,
}

impl From<SnapshotImportParams> for Force<PathBuf> {
    fn from(SnapshotImportParams { path, force }: SnapshotImportParams) -> Self {
        Self::from(SnapshotExportParams { path, force })
    }
}

/// Import a snapshot of the chain state from the file system.
///
/// This method is intended for fast syncing nodes from snapshot files that can downloaded over
/// HTTP, FTP, Torrent or IPFS.
pub async fn snapshot_import(params: Result<SnapshotImportParams, Error>) -> JsonRpcResult {
    // Use path from parameters if provided, otherwise try to guess path from configuration
    let path = Force::from(params?);

    // Tell the chain manager to create and export the snapshot
    let chain_manager = ChainManager::from_registry();
    let actor_response = chain_manager.send(SnapshotImport { path }).await;
    let response = actor_response
        .map_err(internal_error_s)?
        .map_err(internal_error_s)?;

    // Write the response back (the path to the snapshot file)
    serde_json::to_value(response).map_err(internal_error_s)
}
/// Build a stake transaction
pub async fn stake(params: Result<BuildStakeParams, Error>) -> JsonRpcResult {
    // Short-circuit if parameters are wrong
    let params = params?;

    let withdrawer = params
        .withdrawer
        .clone()
        .try_do_magic(|hex_str| PublicKeyHash::from_bech32(get_environment(), &hex_str))
        .map_err(internal_error)?;
    log::debug!("[STAKE] Creating stake transaction with withdrawer address: {withdrawer}");

    // This is the actual message that gets signed as part of the authorization
    let msg = withdrawer.as_secp256k1_msg();

    // Perform some sanity checks on the authorization string
    match params.authorization {
        MagicEither::Left(ref hex_str) => {
            // Authorization string is not a hexadecimal string
            if !hex_str.chars().all(|c| c.is_ascii_hexdigit()) {
                return Err(Error::invalid_params(
                    "The authorization string is not a hexadecimal string",
                ));
            }

            // Invalid authorization length
            if hex_str.len() != 170 {
                return Err(Error::invalid_params(format!(
                    "Authorization string has an unexpected length: {} != {}",
                    hex_str.len(),
                    170
                )));
            }
        }
        MagicEither::Right(_) => (),
    };

    let authorization = params
        .authorization
        .try_do_magic(|hex_str| {
            KeyedSignature::from_recoverable_hex(
                &hex_str[hex_str.char_indices().nth_back(129).unwrap().0..],
                &msg,
            )
        })
        .map_err(internal_error)?;
    let validator = PublicKeyHash::from_public_key(&authorization.public_key);
    log::debug!(
        "[STAKE] A stake authorization was provided, and it was signed by validator {validator}"
    );

    let key = StakeKey {
        validator,
        withdrawer,
    };

    // Construct a BuildStake message that we can relay to the ChainManager for creation of the Stake transaction
    let build_stake = BuildStake {
        dry_run: params.dry_run,
        fee: params.fee,
        utxo_strategy: params.utxo_strategy,
        stake_output: StakeOutput {
            authorization,
            key,
            value: params.value,
        },
    };

    ChainManager::from_registry()
        .send(build_stake)
        .map(|res| match res {
            Ok(Ok(transaction)) => {
                // In the event that this is a dry run, we want to inject some additional information into the
                // response, so that the user can confirm the facts surrounding the stake transaction before
                // submitting it
                if params.dry_run {
                    let staker = transaction
                        .signatures
                        .iter()
                        .map(|signature| signature.public_key.pkh())
                        .collect();

                    let bsr = BuildStakeResponse {
                        transaction,
                        staker,
                        validator,
                        withdrawer,
                    };

                    serde_json::to_value(bsr).map_err(internal_error)
                } else {
                    serde_json::to_value(transaction).map_err(internal_error)
                }
            }
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

/// Build an unstake transaction
pub async fn unstake(params: Result<BuildUnstakeParams, Error>) -> JsonRpcResult {
    // Short-circuit if parameters are wrong
    let params = params?;

    let operator = params
        .operator
        .try_do_magic(|hex_str| PublicKeyHash::from_bech32(get_environment(), &hex_str))
        .map_err(internal_error)?;

    // Construct a BuildUnstake message that we can relay to the ChainManager for creation of the Unstake transaction
    let build_unstake = BuildUnstake {
        operator,
        value: params.value,
        fee: params.fee,
        dry_run: params.dry_run,
    };

    ChainManager::from_registry()
        .send(build_unstake)
        .map(|res| match res {
            Ok(Ok(transaction)) => serde_json::to_value(transaction).map_err(internal_error),
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

/// Create a stake authorization for the given address.
///
/// The output of this method is a required argument to call the Stake method.
/* test
{"jsonrpc": "2.0","method": "authorizeStake", "params": {"withdrawer":"wit1lkzl4a365fvrr604pwqzykxugpglkrp5ekj0k0"}, "id": "1"}
*/
pub async fn authorize_stake(params: Result<AuthorizeStake, Error>) -> JsonRpcResult {
    // Short-circuit if parameters are wrong
    let params = params?;

    // If a withdrawer address is not specified, default to local node address
    let withdrawer = if let Some(address) = params.withdrawer {
        PublicKeyHash::from_bech32(get_environment(), &address).map_err(internal_error)?
    } else {
        let pk = signature_mngr::public_key().await.unwrap();

        PublicKeyHash::from_public_key(&pk)
    };

    // This is the actual message that gets signed as part of the authorization
    let msg = withdrawer.as_secp256k1_msg();

    signature_mngr::sign_data(msg)
        .map(|res| {
            res.map_err(internal_error).and_then(|signature| {
                let authorization = StakeAuthorization {
                    withdrawer,
                    signature,
                };

                serde_json::to_value(authorization).map_err(internal_error)
            })
        })
        .await
}

/// Query the amount of nanowits staked by an address.
pub async fn query_stakes(params: Result<Option<QueryStakes>, Error>) -> JsonRpcResult {
    // Short-circuit if parameters are wrong
    let params = params?;
    // Parse params or defaults:
    let msg = params.unwrap_or(QueryStakes::default());
    ChainManager::from_registry()
        .send(msg)
        .map(|res| match res {
            Ok(Ok(stakes)) => serde_json::to_value(stakes).map_err(internal_error),
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

/// Query data requests transaction hashes by providing a RAD hash value
pub async fn search_data_requests(
    params: Result<Option<SearchDataRequests>, Error>,
) -> JsonRpcResult {
    // short-circuit if parameters are wrong
    let params = params?;
    // parse params or defaults
    let msg = params.ok_or(Error::invalid_params("A 'radHash' must be specified"))?;
    ChainManager::from_registry()
        .send(msg)
        .map(|res| match res {
            Ok(Ok(result)) => serde_json::to_value(result).map_err(internal_error),
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

/// Format of the output of query_powers
#[derive(Clone, Debug, PartialEq, Eq, serde::Deserialize, serde::Serialize)]
pub struct QueryStakingPowersRecord {
    /// Staking power
    pub power: u64,
    /// Ranking index (1 .. N)
    pub ranking: usize,
    /// Validator's stringified pkh
    pub validator: String,
    /// Withdrawer's stringified pkh
    pub withdrawer: String,
}

/// Query the amount of nanowits staked by an address.
pub async fn query_powers(params: Result<QueryStakingPowers, Error>) -> JsonRpcResult {
    // Short-circuit if parameters are wrong
    let msg = params?;

    ChainManager::from_registry()
        .send(msg)
        .map(|res| match res {
            Ok(candidates) => {
                let candidates: Vec<QueryStakingPowersRecord> = candidates
                    .iter()
                    .map(|(ranking, key, power)| QueryStakingPowersRecord {
                        power: *power,
                        ranking: *ranking,
                        validator: key.validator.to_string(),
                        withdrawer: key.withdrawer.to_string(),
                    })
                    .collect();
                let candidates = serde_json::to_value(candidates);
                candidates.map_err(internal_error)
            }
            Err(e) => {
                let err = internal_error_s(e);
                Err(err)
            }
        })
        .await
}

/// Params for get_balance_2
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum GetBalance2Params {
    /// sum up balances of all specified comma-separated addresses
    #[serde(alias = "address")]
    #[serde(alias = "pkh")]
    #[serde(alias = "pkhs")]
    Addresses(String),
    /// get balances for all holders within specified limits
    All(GetBalance2Limits),
}

/// Query the amount of nanowits staked by an address.
///
/// When the `Addressess` parameter is specified, this can take as many addresses as needed in a
/// single CSV-style string of colon-separated values, e.g.:
/// `wit1ggyjwgx7rcec079ne5xvjm83keythmtncryrhj;wit1xvud2lcxzwjzrup07dq36w7xdj3graljtlpefg`
///
/// Alternatively, the `All` parameter returns all balances, with the ability to filter by minimum
/// and maximum balances (see `GetBalance2Limits`).
pub async fn get_balance_2(params: Result<Option<GetBalance2Params>, Error>) -> JsonRpcResult {
    // Short-circuit if parameters are wrong
    let params = params?;
    let msg: GetBalance2 = if let Some(params) = params {
        match params {
            GetBalance2Params::Addresses(string) => {
                let addresses: Vec<String> = string.split(';').map(Into::into).collect();
                if addresses.len() > 1 {
                    GetBalance2::Sum(addresses.into_iter().map(MagicEither::Left).collect())
                } else {
                    GetBalance2::Address(MagicEither::Left(string))
                }
            }
            GetBalance2Params::All(limits) => GetBalance2::All(limits),
        }
    } else {
        GetBalance2::All(GetBalance2Limits::default())
    };
    ChainManager::from_registry()
        .send(msg)
        .map(|res| match res {
            Ok(Ok(stakes)) => serde_json::to_value(stakes).map_err(internal_error),
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

/// Different modes for the `get_value_transfer` method.
#[derive(Debug, Default, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum GetValueTransferMode {
    /// Ethereal mode: similar to simple mode, but serializes stuff with EVM-friendly data types.
    Ethereal,
    /// Full mode: returns the same information as the `get_transaction` method.
    #[default]
    Full,
    /// Simple mode: returns only basic data in a more easily consumable form.
    Simple,
}

/// Parameters for the `get_value_transfer` method.
#[derive(Debug, Deserialize, Serialize)]
pub struct GetValueTransferParams {
    hash: MagicEither<String, Hash>,
    #[serde(default)]
    mode: GetValueTransferMode,
    #[serde(default)]
    force: bool,
}

/// Enumerates all the states in which a value transfer transaction can be.
#[derive(Debug, Default, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ValueTransferStatus {
    /// A transaction that never made it into a block (e.g. double spent, replaced by fee, etc.)
    Cancelled,
    /// A transaction that is written into the block chain forever.
    /// Includes block epoch and number of confirmations.
    Final(Epoch, Epoch),
    /// A transaction that is written into a block but is waiting for finality (could still be
    /// reverted).
    /// Includes block epoch and number of confirmations.
    InBlock(Epoch, Epoch),
    /// A transaction that is still in the mempool, waiting to join a block.
    #[default]
    Pending,
    /// A transaction that was included into a block that was eventually reverted.
    Reverted,
}

impl ValueTransferStatus {
    /// Obtain a small number telling which is the status.
    pub fn discriminant(&self) -> u8 {
        use ValueTransferStatus::*;

        match self {
            Cancelled => 0,
            Final(_, _) => 1,
            InBlock(_, _) => 2,
            Pending => 3,
            Reverted => 4,
        }
    }
}

/// Extremely abridged output for the `get_value_transfer` method.
///
/// This is used with the `ethereal` mode.
#[derive(Debug, Deserialize, Serialize)]
pub struct GetValueTransferEtherealOutput {
    finalized: u8,
    metadata: String,
    recipient: String,
    sender: String,
    timestamp: Option<i64>,
    value: u64,
}

/// Full output for the `get_value_transfer` method.
///
/// This is used with the `full` mode.
#[derive(Debug, Deserialize, Serialize)]
pub struct GetValueTransferFullOutput {
    status: ValueTransferStatus,
    transaction: VTTransaction,
}

/// Abridged output for the `get_value_transfer` method.
///
/// This is used with the `simple` mode.
#[derive(Debug, Deserialize, Serialize)]
pub struct GetValueTransferSimpleOutput {
    fee: u64,
    metadata: Vec<String>,
    recipient: String,
    sender: String,
    status: ValueTransferStatus,
    timestamp: Option<i64>,
    value: u64,
}

/// Joins both potential outputs for the `get_value_transfer` method.
#[derive(Debug, Deserialize, Serialize)]
#[serde(untagged)]
pub enum GetValueTransfersOutput {
    /// Ethereal mode, extremely abridged.
    Ethereal(GetValueTransferEtherealOutput),
    /// Full mode, resembling the response of `get_transaction`.
    Full(GetValueTransferFullOutput),
    /// Simple mode, more easily consumable from 3rd party (d)apps.
    Simple(GetValueTransferSimpleOutput),
}

/// Query transaction status data for value transfer transactions
pub async fn get_value_transfer(params: Result<GetValueTransferParams, Error>) -> JsonRpcResult {
    let params = params?;
    let hash = params
        .hash
        .try_do_magic(|string| Hash::from_str(&string))
        .map_err(internal_error)?;

    // Unless forced, we need to make sure that the node is fully synchronized, otherwise the
    // transaction statuses might be off
    if !params.force {
        let sync_status = status().await?;
        let sync_status: SyncStatus =
            serde_json::from_value(sync_status).map_err(internal_error)?;

        if sync_status.node_state != StateMachine::Synced {
            return Err(internal_error_s(
                "The node is not synced yet. But you can use the `force: true` flag if you know what you are doing.",
            ));
        }
    }
    // This method piggybacks on the `get_transaction` method for fetching all the transaction facts
    let transaction = get_transaction(Ok((hash,))).await?;
    let transaction: GetTransactionOutput =
        serde_json::from_value(transaction).map_err(internal_error)?;
    // Assess the status based on inclusion in a block, confirmations, etc.
    let status = if let Some(block_epoch) = transaction.block_epoch {
        let confirmations = transaction.confirmations.unwrap_or_default();
        if transaction.confirmed {
            ValueTransferStatus::Final(block_epoch, confirmations)
        } else {
            // We can safely assume that a transaction that has enough confirmations (epochs ever
            // since) but is not really confirmed by the superblock protocol has been reverted
            if confirmations > 20 {
                ValueTransferStatus::Reverted
            } else {
                ValueTransferStatus::InBlock(block_epoch, confirmations)
            }
        }
    } else {
        ValueTransferStatus::Pending
    };

    // Only makes sense to continue if the provided hash belongs to a value transfer transaction
    if let Transaction::ValueTransfer(vtt) = transaction.transaction {
        let output = if params.mode == GetValueTransferMode::Full {
            // For the "full" mode, the response resembles that of `get_transaction`
            GetValueTransfersOutput::Full(GetValueTransferFullOutput {
                status,
                transaction: vtt,
            })
        } else {
            // For all the other modes, we need to pick all the different bits of info and assemble
            // them into a single output data structure

            // Only adds up the value of the outputs that point to the same recipient as the first
            // output found in the transaction
            let value = vtt.body.first_recipient_value();
            // If there's no outputs, there's no recipient
            let recipient = vtt
                .body
                .outputs
                .first()
                .map(|output| output.pkh.to_string())
                .unwrap_or_default();
            let sender = vtt.signatures[0].public_key.pkh().to_string();
            let metadata = vtt
                .body
                .metadata()
                .iter()
                .map(hex::encode)
                .collect::<Vec<_>>();

            // The timestamp can be only derived from the block epoch once the epoch constants are
            // known (this is specially relevant after the V2_0+ fork)
            let epoch_constants: EpochConstants =
                match EpochManager::from_registry().send(GetEpochConstants).await {
                    Ok(Some(epoch_constats)) => epoch_constats,
                    Err(e) => Err(internal_error(e))?,
                    _ => Err(internal_error(""))?,
                };
            let timestamp = match transaction.block_epoch {
                Some(block_epoch) => epoch_constants
                    .epoch_timestamp(block_epoch)
                    .ok()
                    .map(|(timestamp, _)| timestamp),
                None => None,
            };

            if params.mode == GetValueTransferMode::Ethereal {
                // Ethereal mode needs some extra processing of the output to simplify data types
                let finalized = status.discriminant();
                let metadata = metadata.join("");

                GetValueTransfersOutput::Ethereal(GetValueTransferEtherealOutput {
                    finalized,
                    metadata,
                    recipient,
                    sender,
                    timestamp,
                    value,
                })
            } else {
                // Simple mode requires fetching input transactions as to compute the fee
                let mut input_value = 0u64;
                let mut input_transactions: HashMap<Hash, Transaction> = HashMap::new();
                // Fetch each different input transaction just once, even when referred multiples times
                for input in &vtt.body.inputs {
                    let hash = input.output_pointer().transaction_id;

                    // Sum up the value of every single input
                    input_value += match input_transactions.entry(hash) {
                        Entry::Occupied(entry) => {
                            let transaction = entry.get();

                            get_transaction_output_value(
                                transaction,
                                input.output_pointer().output_index as usize,
                            )
                        }
                        Entry::Vacant(entry) => {
                            let transaction = get_transaction(Ok((hash,))).await?;
                            let transaction =
                                serde_json::from_value::<GetTransactionOutput>(transaction)
                                    .map_err(internal_error)?
                                    .transaction;
                            let transaction_value = get_transaction_output_value(
                                &transaction,
                                input.output_pointer().output_index as usize,
                            );

                            entry.insert(transaction);

                            transaction_value
                        }
                    };
                }
                let fee = input_value.saturating_sub(vtt.body.value());

                GetValueTransfersOutput::Simple(GetValueTransferSimpleOutput {
                    fee,
                    metadata,
                    recipient,
                    sender,
                    status,
                    timestamp,
                    value,
                })
            }
        };

        serde_json::to_value(output).map_err(internal_error)
    } else {
        Err(internal_error_s("wrong transaction type"))
    }
}

fn get_transaction_output_value(transaction: &Transaction, output_index: usize) -> u64 {
    match transaction {
        // Value transfers, data requests, stake and unstake transactions provide their own methods
        // for calculating their value
        Transaction::ValueTransfer(vt) => vt
            .body
            .outputs
            .get(output_index)
            .map(ValueTransferOutput::value)
            .unwrap_or_default(),
        Transaction::DataRequest(dr) => dr
            .body
            .outputs
            .get(output_index)
            .map(ValueTransferOutput::value)
            .unwrap_or_default(),
        Transaction::Unstake(un) => un.body.value(),
        Transaction::Stake(st) => {
            if st.body.change.is_some() {
                st.body.change.clone().unwrap().value
            } else {
                0u64
            }
        }

        // Commits, reveals, tallies and mints don't have inputs nor outputs anymore
        _ => 0u64,
    }
}

/// Joins potential outputs for the `get_data_request` method.
#[derive(Debug, Deserialize, Serialize)]
#[serde(untagged)]
pub enum GetDataRequestOutput {
    /// Ethereal mode, extremely abridged.
    Ethereal(GetDataRequestEtherealOutput),
    /// Full mode, resemble the response of `data_request_report`.
    Full(GetDataRequestFullOutput),
}

/// Different modes for the `get_value_transfer` method.
#[derive(Debug, Default, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum GetDataRequestMode {
    /// Ethereal mode: returns only basic data with EVM-friendly data types.
    Ethereal,
    /// Full mode: returns the same information as the `get_transaction` method.
    #[default]
    Full,
}

/// Parameters for the `get_data_request` method.
#[derive(Debug, Deserialize, Serialize)]
pub struct GetDataRequestParams {
    hash: MagicEither<String, Hash>,
    #[serde(default)]
    mode: GetDataRequestMode,
    #[serde(default)]
    force: bool,
}

/// Enumerates the states in which a data request transaction can be, from the point-of-view of the data requester.
#[derive(Debug, Default, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DataRequestStatus {
    /// A transaction that never made it into a block (e.g. double spent, replaced by fee, etc.)
    Cancelled,
    /// A data request transaction that is still in the mempool, waiting to join a block.
    #[default]
    Pending,
    /// A data request transaction that got included in a block and is currently in COMMIT phase.
    Committing,
    /// A data request transaction that got included in a block and is currently in REVEAL phase.
    Revealing,
    /// A data request transaction that got solved.
    Solved,
    /// A data request transaction whose tally result got included in a block that was eventually reverted, afterwards.
    Reverted,
}

/// Full output for the `get_data_request` method.
///
/// This is used with the `full` mode.
#[derive(Debug, Deserialize, Serialize)]
pub struct GetDataRequestFullOutput {
    status: DataRequestStatus,
    report: DataRequestInfo,
}

/// Extremely abridged output for the `get_data_request` method.
///
/// This is used with the `ethereal` mode.
#[derive(Debug, Deserialize, Serialize)]
pub struct GetDataRequestEtherealOutput {
    block_epoch: Option<u32>,
    block_hash: Option<Hash>,
    confirmations: Option<u32>,
    hash: Hash,
    query: Option<DataRequestQueryParams>,
    result: Option<DataRequestQueryResult>,
    status: DataRequestStatus,
}

/// Radon specs attached to every data request transaction.
#[derive(Debug, Deserialize, Serialize)]
pub struct DataRequestQueryParams {
    dro_hash: Hash,
    rad_hash: Hash,
    rad_bytecode: String,
    collateral_ratio: u16,
    unitary_reward: u64,
    witnesses: u16,
}

/// Radon specs attached to every data request transaction.
#[derive(Debug, Deserialize, Serialize)]
pub struct DataRequestQueryResult {
    cbor_bytes: String,
    finalized: bool,
    timestamp: i64,
}

/// Query data request transaction status and result data, if solved
pub async fn get_data_request(params: Result<GetDataRequestParams, Error>) -> JsonRpcResult {
    let params = params?;
    let hash = params
        .hash
        .try_do_magic(|string| Hash::from_str(&string))
        .map_err(internal_error)?;

    // Unless forced, we need to make sure that the node is fully synchronized, otherwise the
    // transaction statuses might be off
    if !params.force {
        let sync_status = status().await?;
        let sync_status: SyncStatus =
            serde_json::from_value(sync_status).map_err(internal_error)?;

        if sync_status.node_state != StateMachine::Synced {
            return Err(internal_error_s(
                "The node is not synced yet. But you can use the `force: true` flag if you know what you are doing.",
            ));
        }
    }

    // This method piggybacks on the `data_request_report` method for fetching data request transaction facts
    let report = data_request_report(Ok((hash,))).await?;
    let report: DataRequestInfo = serde_json::from_value(report).map_err(internal_error)?;
    let status = if report.block_hash_tally_tx.is_some() {
        DataRequestStatus::Solved
    } else if report.block_hash_dr_tx.is_some() {
        match report.current_stage {
            Some(DataRequestStage::COMMIT) => DataRequestStatus::Committing,
            Some(_) => DataRequestStatus::Revealing,
            None => DataRequestStatus::Solved,
        }
    } else {
        DataRequestStatus::Pending
    };

    // retrieve and compute output data
    let output = if params.mode == GetDataRequestMode::Full {
        // For the "full" mode, the response resembles that of `get_transaction`
        GetDataRequestOutput::Full(GetDataRequestFullOutput { status, report })
    } else {
        // Get current epoch
        let current_epoch = match EpochManager::from_registry().send(GetEpoch).await {
            Ok(Ok(current_epoch)) => current_epoch,
            Ok(Err(e)) => return Err(internal_error(e)),
            Err(e) => return Err(internal_error(e)),
        };

        // Get data request transaction's block epoch:
        let block_hash = report.block_hash_dr_tx;

        let block_epoch = if let Some(block_hash) = block_hash {
            match get_block_epoch(block_hash).await {
                Ok((block_epoch, _)) => Some(block_epoch),
                Err(_) => None,
            }
        } else {
            None
        };

        // Compute block confirmations since inclusion of the data request transaction
        let confirmations =
            block_epoch.map(|block_epoch| current_epoch.saturating_sub(block_epoch + 1));

        // Get data request tally transaction's block epoch:
        let (_, finalized) = if let Some(block_hash_tally_tx) = report.block_hash_tally_tx {
            match get_block_epoch(block_hash_tally_tx).await {
                Ok((tally_epoch, confirmed)) => (Some(tally_epoch), confirmed),
                Err(_) => (None, false),
            }
        } else {
            (None, false)
        };

        // The timestamp can be only derived from the block epoch once the epoch constants are
        // known (this is specially relevant after the V2_0+ fork)
        let epoch_constants: EpochConstants =
            match EpochManager::from_registry().send(GetEpochConstants).await {
                Ok(Some(epoch_constats)) => epoch_constats,
                Err(e) => Err(internal_error(e))?,
                _ => Err(internal_error(""))?,
            };
        let timestamp = match block_epoch {
            Some(block_epoch) => epoch_constants
                .epoch_timestamp(
                    block_epoch.saturating_add(1u32 + u32::from(report.current_commit_round)),
                )
                .ok()
                .map(|(timestamp, _)| timestamp),
            None => None,
        };

        // Get data request transaction's query parameters
        let query = get_drt_query_params(hash).await.ok();

        // Get data request transaction's query result, if any
        let result = confirmations.map(|_| DataRequestQueryResult {
            cbor_bytes: hex::encode(report.tally.unwrap_or_default().tally),
            finalized,
            timestamp: timestamp.unwrap_or_default(),
        });

        GetDataRequestOutput::Ethereal(GetDataRequestEtherealOutput {
            block_epoch,
            block_hash,
            hash,
            confirmations,
            query,
            result,
            status,
        })
    };

    serde_json::to_value(output).map_err(internal_error)
}

#[allow(clippy::cast_possible_truncation)]
async fn get_drt_query_params(dr_tx_hash: Hash) -> Result<DataRequestQueryParams, Error> {
    let inventory_manager = InventoryManager::from_registry();
    let res = inventory_manager
        .send(GetItemTransaction { hash: dr_tx_hash })
        .await;
    match res {
        Ok(Ok((transaction, _, _))) => match transaction {
            Transaction::DataRequest(dr_tx) => {
                let bytecode = dr_tx
                    .body
                    .dr_output
                    .data_request
                    .to_pb_bytes()
                    .unwrap_or_default();

                Ok(DataRequestQueryParams {
                    collateral_ratio: dr_tx
                        .body
                        .dr_output
                        .collateral
                        .div_ceil(dr_tx.body.dr_output.witness_reward)
                        as u16,
                    dro_hash: dr_tx.body.dr_output.hash(),
                    rad_hash: calculate_sha256(&bytecode).into(),
                    rad_bytecode: hex::encode(&bytecode),
                    unitary_reward: dr_tx.body.dr_output.witness_reward,
                    witnesses: dr_tx.body.dr_output.witnesses,
                })
            }
            _ => Err(internal_error("Not a data request transaction")),
        },
        Ok(Err(InventoryManagerError::ItemNotFound)) => {
            let chain_manager = ChainManager::from_registry();
            let res = chain_manager
                .send(GetMemoryTransaction { hash: dr_tx_hash })
                .await;

            match res {
                Ok(Ok(transaction)) => match transaction {
                    Transaction::DataRequest(dr_tx) => {
                        let bytecode = dr_tx
                            .body
                            .dr_output
                            .data_request
                            .to_pb_bytes()
                            .unwrap_or_default();

                        Ok(DataRequestQueryParams {
                            collateral_ratio: dr_tx
                                .body
                                .dr_output
                                .collateral
                                .div_ceil(dr_tx.body.dr_output.witness_reward)
                                as u16,
                            dro_hash: dr_tx.body.dr_output.hash(),
                            rad_hash: calculate_sha256(&bytecode).into(),
                            rad_bytecode: hex::encode(&bytecode),
                            unitary_reward: dr_tx.body.dr_output.witness_reward,
                            witnesses: dr_tx.body.dr_output.witnesses,
                        })
                    }
                    _ => Err(internal_error("Not a data request transaction")),
                },
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

async fn get_block_epoch(block_hash: Hash) -> Result<(u32, bool), Error> {
    let inventory_manager = InventoryManager::from_registry();

    let res = inventory_manager
        .send(GetItemBlock { hash: block_hash })
        .await;

    match res {
        Ok(Ok(output)) => {
            let block_epoch = output.block_header.beacon.checkpoint;
            let block_hash = output.versioned_hash(get_protocol_version(Some(block_epoch)));

            // Check if this block is confirmed by a majority of superblock votes
            let chain_manager = ChainManager::from_registry();
            let res = chain_manager
                .send(IsConfirmedBlock {
                    block_hash,
                    block_epoch,
                })
                .await;

            match res {
                Ok(Ok(confirmed)) => Ok((block_epoch, confirmed)),
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

    use witty_jsonrpc::prelude::*;

    use witnet_data_structures::{chain::RADRequest, transaction::*};

    use super::*;

    #[test]
    fn empty_string_parse_error() {
        // An empty message should return a parse error
        let empty_string = "";
        let parse_error =
            r#"{"jsonrpc":"2.0","error":{"code":-32700,"message":"Parse error"},"id":null}"#
                .to_string();
        let mut server = WittyMultiServer::new();
        attach_api(&mut server, true, Subscriptions::default(), &None);
        let response = server.handle_request_sync(empty_string, Default::default());
        assert_eq!(response, Some(parse_error));
    }

    #[test]
    fn inventory_method() {
        // The expected behaviour of the inventory method
        use witnet_data_structures::chain::*;
        let block = block_example();

        let inv_elem = InventoryItem::Block(block);
        let s = serde_json::to_string(&inv_elem).unwrap();
        let msg = format!(r#"{{"jsonrpc":"2.0","method":"inventory","params":{s},"id":1}}"#);

        // Expected result: true
        let expected = r#"{"jsonrpc":"2.0","error":{"code":-32603,"message":"MailboxError(Mailbox has closed)"},"id":1}"#.to_string();
        let mut server = WittyMultiServer::new();
        attach_api(&mut server, true, Subscriptions::default(), &None);
        let response = server.handle_request_sync(&msg, Default::default());
        assert_eq!(response, Some(expected));
    }

    #[test]
    fn inventory_invalid_params() {
        // What happens when the inventory method is called with an invalid parameter?
        let msg = r#"{"jsonrpc":"2.0","method":"inventory","params":{ "header": 0 },"id":1}"#;
        let expected = r#"{"jsonrpc":"2.0","error":{"code":-32602,"message":"Invalid params: unknown variant `header`, expected one of"#.to_string();
        let mut server = WittyMultiServer::new();
        attach_api(&mut server, true, Subscriptions::default(), &None);
        let response = server.handle_request_sync(msg, Default::default());
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
        let mut server = WittyMultiServer::new();
        attach_api(&mut server, true, Subscriptions::default(), &None);
        let response = server.handle_request_sync(msg, Default::default());
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
        let mut server = WittyMultiServer::new();
        attach_api(&mut server, true, Subscriptions::default(), &None);
        let response = server.handle_request_sync(&msg, Default::default());
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
        let mut server = WittyMultiServer::new();
        attach_api(&mut server, true, Subscriptions::default(), &None);
        let response = server.handle_request_sync(&msg, Default::default());
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
        let response = server.handle_request_sync(&msg, Default::default());
        assert_eq!(response, Some(expected));
    }

    #[test]
    fn subscribe_invalid_method() {
        // Try to subscribe to a non-existent subscription?
        let msg = r#"{"jsonrpc":"2.0","method":"witnet_subscribe","params":["asdf"],"id":1}"#;
        let expected =
            r#"{"jsonrpc":"2.0","error":{"code":-32602,"message":"Invalid parameters: unknown subscription topic","data":"\"asdf\""},"id":1}"#
                .to_string();
        let mut server = WittyMultiServer::new();
        attach_api(&mut server, true, Subscriptions::default(), &None);
        let response = server.handle_request_sync(msg, Session::mock());
        assert_eq!(response, Some(expected));
    }

    #[test]
    fn subscribe_new_blocks() {
        // Subscribe to new blocks gives us a SubscriptionId
        let msg = r#"{"jsonrpc":"2.0","method":"witnet_subscribe","params":["blocks"],"id":1}"#;
        let expected = r#"{"jsonrpc":"2.0","result":"1","id":1}"#.to_string();
        let mut server = WittyMultiServer::new();
        attach_api(&mut server, true, Subscriptions::default(), &None);
        let response = server.handle_request_sync(msg, Session::mock());
        assert_eq!(response, Some(expected));
    }

    #[test]
    fn unsubscribe_returns_true() {
        // Check that unsubscribe returns true
        let msg2 = r#"{"jsonrpc":"2.0","method":"witnet_unsubscribe","params":["1"],"id":1}"#;
        let expected2 = r#"{"jsonrpc":"2.0","result":true,"id":1}"#.to_string();
        // But first, subscribe to blocks
        let msg1 = r#"{"jsonrpc":"2.0","method":"witnet_subscribe","params":["blocks"],"id":1}"#;
        let mut server = WittyMultiServer::new();
        attach_api(&mut server, true, Subscriptions::default(), &None);
        let meta = Session::mock();
        let _response1 = server.handle_request_sync(msg1, meta.clone());
        let response2 = server.handle_request_sync(msg2, meta);
        assert_eq!(response2, Some(expected2));
    }

    #[test]
    fn unsubscribe_can_fail() {
        // Check that unsubscribe returns false when unsubscribing to a non-existent subscription
        let msg = r#"{"jsonrpc":"2.0","method":"witnet_unsubscribe","params":["999"],"id":1}"#;
        let expected = r#"{"jsonrpc":"2.0","error":{"code":-32602,"message":"Invalid subscription id."},"id":1}"#.to_string();
        let mut server = WittyMultiServer::new();
        attach_api(&mut server, true, Subscriptions::default(), &None);
        let response = server.handle_request_sync(msg, Session::mock());
        assert_eq!(response, Some(expected));
    }

    #[ignore]
    #[test]
    fn serialize_block() {
        // Check that the serialization of `Block` doesn't change
        use witnet_data_structures::chain::*;
        let block = block_example();
        let inv_elem = InventoryItem::Block(block);
        let s = serde_json::to_string(&inv_elem).unwrap();
        let expected = r#"{"block":{"block_header":{"signals":0,"beacon":{"checkpoint":0,"hashPrevBlock":"0000000000000000000000000000000000000000000000000000000000000000"},"merkle_roots":{"mint_hash":"0000000000000000000000000000000000000000000000000000000000000000","vt_hash_merkle_root":"0000000000000000000000000000000000000000000000000000000000000000","dr_hash_merkle_root":"0000000000000000000000000000000000000000000000000000000000000000","commit_hash_merkle_root":"0000000000000000000000000000000000000000000000000000000000000000","reveal_hash_merkle_root":"0000000000000000000000000000000000000000000000000000000000000000","tally_hash_merkle_root":"0000000000000000000000000000000000000000000000000000000000000000","stake_hash_merkle_root":"0000000000000000000000000000000000000000000000000000000000000000","unstake_hash_merkle_root":"0000000000000000000000000000000000000000000000000000000000000000"},"proof":{"proof":{"proof":[],"public_key":{"compressed":0,"bytes":[0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0]}}},"bn256_public_key":null},"block_sig":{"signature":{"Secp256k1":{"der":[]}},"public_key":{"compressed":0,"bytes":[0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0]}},"txns":{"mint":{"epoch":0,"outputs":[]},"value_transfer_txns":[],"data_request_txns":[{"body":{"inputs":[{"output_pointer":"0000000000000000000000000000000000000000000000000000000000000000:0"}],"outputs":[{"pkh":"wit1qqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqwrt3a4","value":0,"time_lock":0}],"dr_output":{"data_request":{"time_lock":0,"retrieve":[{"kind":"HTTP-GET","url":"https://openweathermap.org/data/2.5/weather?id=2950159&appid=b6907d289e10d714a6e88b30761fae22"},{"kind":"HTTP-GET","url":"https://openweathermap.org/data/2.5/weather?id=2950159&appid=b6907d289e10d714a6e88b30761fae22"}],"aggregate":{"filters":[],"reducer":0},"tally":{"filters":[],"reducer":0}},"witness_reward":0,"witnesses":0,"commit_and_reveal_fee":0,"min_consensus_percentage":0,"collateral":0}},"signatures":[{"signature":{"Secp256k1":{"der":[]}},"public_key":{"compressed":0,"bytes":[0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0]}}]}],"commit_txns":[],"reveal_txns":[],"tally_txns":[],"stake_txns":[],"unstake_txns":[]}}}"#;
        assert_eq!(s, expected, "\n{s}\n");
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
        assert_eq!(s, expected, "\n{s}\n");
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
            DRTransactionBody::new(inputs, data_request_output, vec![]),
            keyed_signatures,
        ))
    }

    #[ignore]
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
        let expected = r#"{"dro":{"data_request":{"time_lock":0,"retrieve":[],"aggregate":{"filters":[],"reducer":0},"tally":{"filters":[],"reducer":0}},"witness_reward":0,"witnesses":0,"commit_and_reveal_fee":0,"min_consensus_percentage":0,"collateral":0},"fee":{"absolute":0},"dry_run":false}"#;
        assert_eq!(s, expected, "\n{s}\n");
    }

    #[ignore]
    #[test]
    fn list_jsonrpc_methods() {
        // This test will break when adding or removing JSON-RPC methods.
        // When adding a new method, please make sure to mark it as sensitive if that's the case.
        // Removing a method means breaking the API and should be avoided.
        let mut server = WittyMultiServer::new();
        attach_api(&mut server, true, Default::default(), &None);
        let api = server.describe_api();
        let all_methods = api.iter().map(String::as_str).collect::<BTreeSet<_>>();
        let all_methods_vec = all_methods.iter().copied().collect::<Vec<_>>();
        assert_eq!(
            all_methods_vec,
            vec![
                "addPeers",
                "authorizeStake",
                "chainExport",
                "chainImport",
                "clearPeers",
                "createVRF",
                "dataRequestReport",
                "getBalance",
                "getBalance2",
                "getBlock",
                "getBlockChain",
                "getConsensusConstants",
                "getDataRequest",
                "getMempool",
                "getPkh",
                "getPublicKey",
                "getReputation",
                "getReputationAll",
                "getSuperblock",
                "getSupplyInfo",
                "getTransaction",
                "getUtxoInfo",
                "getValueTransfer",
                "initializePeers",
                "inventory",
                "knownPeers",
                "masterKeyExport",
                "nodeStats",
                "peers",
                "priority",
                "rewind",
                "sendRequest",
                "sendValue",
                "sign",
                "signalingInfo",
                "stake",
                "syncStatus",
                "tryRequest",
                "witnet_subscribe",
                "witnet_unsubscribe",
            ]
        );

        let mut server = WittyMultiServer::new();
        attach_api(&mut server, false, Default::default(), &None);
        let api = server.describe_api();
        let sensitive_methods = api.iter().map(String::as_str).collect::<BTreeSet<&str>>();

        // Disabling sensitive methods does not unregister them, the methods still exist but
        // they return a custom error message
        assert_eq!(all_methods.difference(&sensitive_methods).count(), 0);

        let expected_sensitive_methods = vec![
            "addPeers",
            "authorizeStake",
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
            "stake",
        ];

        for method_name in expected_sensitive_methods {
            let msg = format!(r#"{{"jsonrpc":"2.0","method":"{method_name}","id":1}}"#);
            let error_msg = format!(
                r#"{{"jsonrpc":"2.0","error":{{"code":-32603,"message":{:?}}},"id":1}}"#,
                unauthorized_message(method_name)
            );

            let response = server.handle_request_sync(&msg, Default::default());

            assert_eq!(response.unwrap(), error_msg);
        }
    }
}
