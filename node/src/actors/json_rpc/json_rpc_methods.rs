use std::{
    collections::HashMap,
    sync::atomic::{AtomicUsize, Ordering},
    sync::Arc,
};

#[cfg(not(test))]
use actix::System;
use actix::{MailboxError, SystemService};
use jsonrpc_core::{futures, futures::Future, BoxFuture, MetaIoHandler, Params, Value};
use jsonrpc_pubsub::{PubSubHandler, Session, Subscriber, SubscriptionId};
use log::{debug, error, info};
use serde::{Deserialize, Serialize};

use witnet_data_structures::{
    chain::{self, Block, CheckpointBeacon, Hash},
    transaction::Transaction,
    vrf::VrfMessage,
};

use crate::actors::{
    chain_manager::{ChainManager, ChainManagerError},
    epoch_manager::EpochManager,
    inventory_manager::InventoryManager,
    messages::{
        AddCandidates, AddTransaction, BuildDrt, BuildVtt, GetBlocksEpochRange, GetEpoch, GetItem,
        GetState, NumSessions,
    },
    sessions_manager::SessionsManager,
};
use crate::signature_mngr;

//use std::str::FromStr;
use super::Subscriptions;

#[cfg(test)]
use self::mock_actix::System;
use crate::actors::chain_manager::StateMachine;
use crate::actors::messages::{GetBalance, GetDataRequestReport, GetHighestCheckpointBeacon};
use futures::future;
use witnet_data_structures::chain::PublicKeyHash;

type JsonRpcResult = Result<Value, jsonrpc_core::Error>;
type JsonRpcResultAsync = Box<dyn Future<Item = Value, Error = jsonrpc_core::Error> + Send>;

/// Define the JSON-RPC interface:
/// All the methods available through JSON-RPC
pub fn jsonrpc_io_handler(subscriptions: Subscriptions) -> PubSubHandler<Arc<Session>> {
    let mut io = PubSubHandler::new(MetaIoHandler::default());

    io.add_method("inventory", |params: Params| inventory(params.parse()?));
    io.add_method("getBlockChain", |params: Params| {
        get_block_chain(params.parse())
    });
    io.add_method("getBlock", |params: Params| get_block(params.parse()));
    //io.add_method("getOutput", |params: Params| get_output(params.parse()));
    io.add_method("sendRequest", |params: Params| send_request(params.parse()));
    io.add_method("sendValue", |params: Params| send_value(params.parse()));
    io.add_method("status", |_params: Params| status());
    io.add_method("getPublicKey", |_params: Params| get_public_key());
    io.add_method("sign", |params: Params| sign_data(params.parse()));
    io.add_method("createVRF", |params: Params| create_vrf(params.parse()));
    io.add_method("dataRequestReport", |params: Params| {
        data_request_report(params.parse())
    });
    io.add_method("getBalance", |params: Params| get_balance(params.parse()));

    // We need two Arcs, one for subscribe and one for unsuscribe
    let ss = subscriptions.clone();
    let ssu = subscriptions.clone();
    let atomic_counter = AtomicUsize::new(1);
    io.add_subscription(
        "witnet_subscription",
        (
            "witnet_subscribe",
            move |params: Params, _meta: Arc<Session>, subscriber: Subscriber| {
                debug!("Called witnet_subscribe");
                let params_vec: Vec<serde_json::Value> = match params {
                    Params::Array(v) => v,
                    _ => {
                        // Ignore errors with `.ok()` because an error here means the connection was closed
                        subscriber
                            .reject(jsonrpc_core::Error::invalid_params("Expected array"))
                            .ok();
                        return;
                    }
                };

                let method_name: String = match serde_json::from_value(params_vec[0].clone()) {
                    Ok(s) => s,
                    Err(e) => {
                        // Ignore errors with `.ok()` because an error here means the connection was closed
                        subscriber
                            .reject(jsonrpc_core::Error::invalid_params(e.to_string()))
                            .ok();
                        return;
                    }
                };

                // Get params, or set to Value::Null if the "params" key does not exist
                let method_params = params_vec.get(1).cloned().unwrap_or_default();

                let add_subscription = |method_name, subscriber: Subscriber| {
                    if let Ok(mut s) = ss.lock() {
                        let id = SubscriptionId::String(
                            atomic_counter.fetch_add(1, Ordering::SeqCst).to_string(),
                        );
                        if let Ok(sink) = subscriber.assign_id(id.clone()) {
                            let v = s.entry(method_name).or_insert_with(HashMap::new);
                            v.insert(id, (sink, method_params));
                            debug!("Subscribed to {}", method_name);
                            debug!("This session has {} subscriptions to this method", v.len());
                        } else {
                            // Session closed before we got a chance to reply
                            debug!("Failed to assing id: session closed");
                        }
                    } else {
                        error!("Failed to adquire lock in add_subscription");
                    }
                };

                match method_name.as_str() {
                    "newBlocks" => {
                        debug!("New subscription to newBlocks");
                        add_subscription("newBlocks", subscriber);
                    }
                    e => {
                        debug!("Unknown subscription method: {}", e);
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
            move |id: SubscriptionId, meta: Option<Arc<Session>>| -> BoxFuture<Value> {
                debug!("Closing subscription {:?}", id);
                match (ssu.lock(), meta) {
                    (Ok(mut s), Some(_meta)) => {
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
                    (Ok(_s), None) => {
                        // The connection was closed
                        // No need to remove from hashmap, it is removed in
                        // impl Handler<Unregister> for JsonRpcServer
                        Box::new(futures::future::ok(Value::Bool(true)))
                    }
                    (Err(e), _meta) => {
                        error!("Failed to adquire lock in witnet_unsubscribe");
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
pub fn inventory(inv_elem: InventoryItem) -> JsonRpcResult {
    match inv_elem {
        InventoryItem::Block(block) => {
            debug!("Got block from JSON-RPC. Sending AnnounceItems message.");

            // Get SessionsManager's address
            let chain_manager_addr = System::current().registry().get::<ChainManager>();
            // If this function was called asynchronously, it could wait for the result
            // But it's not so we just assume success
            chain_manager_addr.do_send(AddCandidates {
                blocks: vec![block],
            });

            // Returns a boolean indicating success
            Ok(Value::Bool(true))
        }

        InventoryItem::Transaction(transaction) => {
            debug!("Got transaction from JSON-RPC. Sending AnnounceItems message.");

            // Get SessionsManager's address
            let chain_manager_addr = System::current().registry().get::<ChainManager>();
            // If this function was called asynchronously, it could wait for the result
            // But it's not so we just assume success
            chain_manager_addr.do_send(AddTransaction { transaction });

            // Returns a boolean indicating success
            Ok(Value::Bool(true))
        }

        inv_elem => {
            info!(
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
    /// TODO
    #[serde(default)] // default to 0
    pub epoch: i64,
    /// TODO
    #[serde(default)] // default to 0
    pub limit: u32,
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

    let GetBlockChainParams { epoch, limit } = match params {
        Ok(x) => x.unwrap_or_default(),
        Err(e) => return Box::new(futures::failed(e)),
    };

    let limit = limit as usize;
    let chain_manager_addr = ChainManager::from_registry();
    if epoch >= 0 {
        let epoch = epoch as u32;
        let fut = chain_manager_addr
            .send(GetBlocksEpochRange::new_with_limit(epoch.., limit))
            .then(process_get_block_chain);
        Box::new(fut)
    } else {
        // On negative epoch, get blocks from last n epochs
        // But, what is the current epoch?
        let fut = EpochManager::from_registry()
            .send(GetEpoch)
            .then(move |res| match res {
                Ok(Ok(current_epoch)) => {
                    let epoch = (i64::from(current_epoch) + epoch) as u32;

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
                    .send(GetBlocksEpochRange::new_with_limit(epoch.., limit))
                    .then(process_get_block_chain)
            });
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
            .send(GetItem { hash })
            .then(move |res| match res {
                Ok(Ok(chain::InventoryItem::Block(output))) => {
                    let value = match serde_json::to_value(output) {
                        Ok(x) => x,
                        Err(e) => {
                            let err = internal_error(e);
                            return futures::failed(err);
                        }
                    };
                    futures::finished(value)
                }
                Ok(Ok(chain::InventoryItem::Transaction(_))) => {
                    // Not a block
                    let err = internal_error(format!("Not a block, {} is a transaction", hash));
                    futures::failed(err)
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
    debug!("Creating data request from JSON-RPC.");

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
    debug!("Creating value transfer from JSON-RPC.");

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

/// Node status
#[derive(Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Status {
    chain_beacon: CheckpointBeacon,
    synchronized: bool,
    num_peers_inbound: u32,
    num_peers_outbound: u32,
}

/// Get node status
pub fn status() -> JsonRpcResultAsync {
    let chain_manager = ChainManager::from_registry();
    let sessions_manager = SessionsManager::from_registry();

    let synchronized_fut =
        chain_manager
            .send(GetState)
            .map_err(internal_error)
            .then(|res| match res {
                Ok(Ok(StateMachine::Synced)) => Ok(true),
                Ok(Ok(..)) => Ok(false),
                Ok(Err(())) => Err(internal_error(())),
                Err(e) => Err(internal_error(e)),
            });
    let num_peers_fut = sessions_manager.send(NumSessions).then(|res| match res {
        Ok(Ok(res)) => Ok(res),
        Ok(Err(())) => Err(internal_error(())),
        Err(e) => Err(internal_error(e)),
    });
    let chain_beacon_fut = chain_manager
        .send(GetHighestCheckpointBeacon)
        .then(|res| match res {
            Ok(Ok(x)) => Ok(x),
            Ok(Err(e)) => Err(internal_error(e)),
            Err(e) => Err(internal_error(e)),
        });

    let j = Future::join3(synchronized_fut, num_peers_fut, chain_beacon_fut)
        .map(|(synchronized, num_peers, chain_beacon)| Status {
            synchronized,
            num_peers_inbound: num_peers.inbound as u32,
            num_peers_outbound: num_peers.outbound as u32,
            chain_beacon,
        })
        .and_then(|res| match serde_json::to_value(res) {
            Ok(x) => futures::finished(x),
            Err(e) => {
                let err = internal_error(e);
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

/// Sign Data
pub fn sign_data(params: Result<[u8; 32], jsonrpc_core::Error>) -> JsonRpcResultAsync {
    let data = match params {
        Ok(x) => x,
        Err(e) => return Box::new(futures::failed(e)),
    };

    let fut = signature_mngr::sign_data(data)
        .map_err(internal_error)
        .and_then(|ks| {
            log::debug!("Signature: {:?}", ks.signature);
            // ks.signature.to_bytes().unwrap().to_vec().into()
            match ks.signature.to_bytes() {
                Ok(bytes) => future::Either::A(future::ok(bytes.to_vec().into())),
                Err(e) => {
                    let err = internal_error(e);
                    future::Either::B(futures::failed(err))
                }
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

    let chain_manager_addr = System::current().registry().get::<ChainManager>();

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

    let chain_manager_addr = System::current().registry().get::<ChainManager>();

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

#[cfg(test)]
mod mock_actix {
    use actix::{MailboxError, Message};
    use futures::Future;

    pub struct System;

    pub struct SystemRegistry;

    pub struct Addr;

    impl System {
        pub fn current() -> Self {
            System
        }
        pub fn registry(&self) -> &SystemRegistry {
            &SystemRegistry
        }
    }

    impl SystemRegistry {
        pub fn get<T>(&self) -> Addr {
            Addr
        }
    }

    impl Addr {
        pub fn do_send<T>(&self, _msg: T) {}
        pub fn send<T: Message>(
            &self,
            _msg: T,
        ) -> impl Future<Item = T::Result, Error = MailboxError> {
            // We cannot test methods which use `send`, so return an error
            futures::failed(MailboxError::Closed)
        }
    }
}

#[cfg(test)]
mod tests {
    use futures::sync::mpsc;

    use super::*;
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
        let io = jsonrpc_io_handler(subscriptions);
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
        let expected = r#"{"jsonrpc":"2.0","result":true,"id":1}"#.to_string();
        let subscriptions = Subscriptions::default();
        let (transport_sender, _transport_receiver) = mpsc::channel(0);
        let meta = Arc::new(Session::new(transport_sender));
        let io = jsonrpc_io_handler(subscriptions);
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
        let io = jsonrpc_io_handler(subscriptions);
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
        let io = jsonrpc_io_handler(subscriptions);
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
        let expected = r#"{"jsonrpc":"2.0","result":true,"id":1}"#.to_string();
        let subscriptions = Subscriptions::default();
        let (transport_sender, _transport_receiver) = mpsc::channel(0);
        let meta = Arc::new(Session::new(transport_sender));
        let io = jsonrpc_io_handler(subscriptions);
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
        let io = jsonrpc_io_handler(subscriptions);
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
        let io = jsonrpc_io_handler(subscriptions);
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
        let io = jsonrpc_io_handler(subscriptions);
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
        let io = jsonrpc_io_handler(subscriptions);
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
        let expected = r#"{"block":{"block_header":{"version":0,"beacon":{"checkpoint":0,"hashPrevBlock":"0000000000000000000000000000000000000000000000000000000000000000"},"merkle_roots":{"mint_hash":"0000000000000000000000000000000000000000000000000000000000000000","vt_hash_merkle_root":"0000000000000000000000000000000000000000000000000000000000000000","dr_hash_merkle_root":"0000000000000000000000000000000000000000000000000000000000000000","commit_hash_merkle_root":"0000000000000000000000000000000000000000000000000000000000000000","reveal_hash_merkle_root":"0000000000000000000000000000000000000000000000000000000000000000","tally_hash_merkle_root":"0000000000000000000000000000000000000000000000000000000000000000"},"proof":{"proof":{"proof":[],"public_key":{"compressed":0,"bytes":[0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0]}}}},"block_sig":{"signature":{"Secp256k1":{"der":[]}},"public_key":{"compressed":0,"bytes":[0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0]}},"txns":{"mint":{"epoch":0,"output":{"pkh":"0000000000000000000000000000000000000000","value":0}},"value_transfer_txns":[],"data_request_txns":[{"body":{"inputs":[{"output_pointer":"0000000000000000000000000000000000000000000000000000000000000000:0"}],"outputs":[{"pkh":"0000000000000000000000000000000000000000","value":0}],"dr_output":{"data_request":{"not_before":0,"retrieve":[{"kind":"HTTP-GET","url":"https://openweathermap.org/data/2.5/weather?id=2950159&appid=b6907d289e10d714a6e88b30761fae22","script":[]},{"kind":"HTTP-GET","url":"https://openweathermap.org/data/2.5/weather?id=2950159&appid=b6907d289e10d714a6e88b30761fae22","script":[]}],"aggregate":{"script":[]},"consensus":{"script":[]},"deliver":[{"kind":"HTTP-GET","url":"https://hooks.zapier.com/hooks/catch/3860543/l2awcd/"},{"kind":"HTTP-GET","url":"https://hooks.zapier.com/hooks/catch/3860543/l1awcw/"}]},"value":0,"witnesses":0,"backup_witnesses":0,"commit_fee":0,"reveal_fee":0,"tally_fee":0,"time_lock":0}},"signatures":[{"signature":{"Secp256k1":{"der":[]}},"public_key":{"compressed":0,"bytes":[0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0]}}]}],"commit_txns":[],"reveal_txns":[],"tally_txns":[]}}}"#;
        assert_eq!(s, expected, "\n{}\n", s);
    }

    #[test]
    fn serialize_transaction() {
        use witnet_data_structures::chain::*;
        let rad_aggregate = RADAggregate { script: vec![0] };

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

        let rad_consensus = RADConsensus { script: vec![0] };

        let rad_deliver_1 = RADDeliver {
            kind: RADType::HttpGet,
            url: "https://hooks.zapier.com/hooks/catch/3860543/l2awcd/".to_string(),
        };

        let rad_deliver_2 = RADDeliver {
            kind: RADType::HttpGet,
            url: "https://hooks.zapier.com/hooks/catch/3860543/l1awcw/".to_string(),
        };

        let rad_request = RADRequest {
            aggregate: rad_aggregate,
            not_before: 0,
            retrieve: vec![rad_retrieve_1, rad_retrieve_2],
            consensus: rad_consensus,
            deliver: vec![rad_deliver_1, rad_deliver_2],
        };
        // Check that the serialization of `Transaction` doesn't change
        let transaction = build_hardcoded_transaction(rad_request);

        let inv_elem = InventoryItem::Transaction(transaction);
        let s = serde_json::to_string(&inv_elem).unwrap();
        let expected = r#"{"transaction":{"DataRequest":{"body":{"inputs":[{"output_pointer":"0909090909090909090909090909090909090909090909090909090909090909:0"}],"outputs":[],"dr_output":{"data_request":{"not_before":0,"retrieve":[{"kind":"HTTP-GET","url":"https://openweathermap.org/data/2.5/weather?id=2950159&appid=b6907d289e10d714a6e88b30761fae22","script":[0]},{"kind":"HTTP-GET","url":"https://openweathermap.org/data/2.5/weather?id=2950159&appid=b6907d289e10d714a6e88b30761fae22","script":[0]}],"aggregate":{"script":[0]},"consensus":{"script":[0]},"deliver":[{"kind":"HTTP-GET","url":"https://hooks.zapier.com/hooks/catch/3860543/l2awcd/"},{"kind":"HTTP-GET","url":"https://hooks.zapier.com/hooks/catch/3860543/l1awcw/"}]},"value":0,"witnesses":0,"backup_witnesses":0,"commit_fee":0,"reveal_fee":0,"tally_fee":0,"time_lock":0}},"signatures":[{"signature":{"Secp256k1":{"der":[]}},"public_key":{"compressed":0,"bytes":[0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0]}}]}}}"#;
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
        let expected = r#"{"dro":{"data_request":{"not_before":0,"retrieve":[],"aggregate":{"script":[]},"consensus":{"script":[]},"deliver":[]},"value":0,"witnesses":0,"backup_witnesses":0,"commit_fee":0,"reveal_fee":0,"tally_fee":0,"time_lock":0},"fee":0}"#;
        assert_eq!(s, expected, "\n{}\n", s);
    }
}
