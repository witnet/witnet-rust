#[cfg(test)]
use self::mock_actix::System;
use crate::actors::chain_manager::{
    messages::{AddNewBlock, GetBlocksEpochRange},
    ChainManager,
};
#[cfg(not(test))]
use actix::System;
use actix::SystemService;
use jsonrpc_core::futures;
use jsonrpc_core::futures::Future;
use jsonrpc_core::{IoHandler, Params, Value};
use log::info;
use serde::{Deserialize, Serialize};
use witnet_data_structures::chain::{Block, InventoryEntry};

type JsonRpcResult = Result<Value, jsonrpc_core::Error>;

/// Define the JSON-RPC interface:
/// All the methods available through JSON-RPC
pub fn jsonrpc_io_handler() -> IoHandler<()> {
    let mut io = IoHandler::new();

    io.add_method("inventory", |params: Params| inventory(params.parse()?));
    io.add_method("getBlockChain", |_params| get_block_chain());

    io
}

/// Inventory element: block, tx, etc
#[derive(Debug, Eq, PartialEq, Clone, Serialize, Deserialize)]
pub enum InventoryItem {
    /// Error
    #[serde(rename = "error")]
    Error,
    /// Transaction
    #[serde(rename = "tx")]
    Tx,
    /// Block
    #[serde(rename = "block")]
    Block(Block),
    /// Data request
    #[serde(rename = "data_request")]
    DataRequest,
    /// Data result
    #[serde(rename = "data_result")]
    DataResult,
}

/// Make the node process, validate and potentially broadcast a new inventory entry.
///
/// Input: the JSON serialization of a well-formed inventory entry
///
/// Returns a boolean indicating success.
/* Test string:
{"jsonrpc": "2.0","method": "inventory","params": {"block": {"block_header":{"version":1,"beacon":{"checkpoint":2,"hash_prev_block": {"SHA256": [4,4,4,4,4,4,4,4,4,4,4,4,4,4,4,4,4,4,4,4,4,4,4,4,4,4,4,4,4,4,4,4]}},"hash_merkle_root":{"SHA256":[3,3,3,3,3,3,3,3,3,3,3,3,3,3,3,3,3,3,3,3,3,3,3,3,3,3,3,3,3,3,3,3]}},"proof":{"block_sig": null,"influence":99999}"txns":[null]}},"id": 1}
*/
pub fn inventory(inv_elem: InventoryItem) -> JsonRpcResult {
    match inv_elem {
        InventoryItem::Block(block) => {
            info!("Got block from JSON-RPC. Sending AnnounceItems message.");

            // Get SessionsManager's address
            let chain_manager_addr = System::current().registry().get::<ChainManager>();
            // If this function was called asynchronously, it could wait for the result
            // But it's not so we just assume success
            chain_manager_addr.do_send(AddNewBlock { block });

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

/// Get the list of all the known block hashes.
///
/// Returns a list of `(epoch, block_hash)` pairs.
/* test
{"jsonrpc": "2.0","method": "getBlockChain", "id": 1}
*/
pub fn get_block_chain() -> impl Future<Item = Value, Error = jsonrpc_core::Error> {
    let chain_manager_addr = ChainManager::from_registry();
    chain_manager_addr
        .send(GetBlocksEpochRange::new(..))
        .then(|res| match res {
            Ok(Ok(vec_inv_entry)) => {
                let epoch_and_hash: Vec<_> = vec_inv_entry
                    .into_iter()
                    .map(|(epoch, inv_entry)| {
                        let hash = match inv_entry {
                            InventoryEntry::Block(hash) => hash,
                            x => panic!("{:?} is not a block", x),
                        };
                        let hash_string = format!("{}", hash);
                        (epoch, hash_string)
                    })
                    .collect();
                let value = match serde_json::to_value(epoch_and_hash) {
                    Ok(x) => x,
                    Err(e) => {
                        let err = jsonrpc_core::Error {
                            code: jsonrpc_core::ErrorCode::InternalError,
                            message: format!("{}", e),
                            data: None,
                        };
                        return futures::failed(err);
                    }
                };
                futures::finished(value)
            }
            Ok(Err(e)) => {
                let err = jsonrpc_core::Error {
                    code: jsonrpc_core::ErrorCode::InternalError,
                    message: format!("{:?}", e),
                    data: None,
                };
                futures::failed(err)
            }
            Err(e) => {
                let err = jsonrpc_core::Error {
                    code: jsonrpc_core::ErrorCode::InternalError,
                    message: format!("{:?}", e),
                    data: None,
                };
                futures::failed(err)
            }
        })
}

#[cfg(test)]
mod mock_actix {
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
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_string_parse_error() {
        // An empty message should return a parse error
        let empty_string = "";
        let parse_error =
            r#"{"jsonrpc":"2.0","error":{"code":-32700,"message":"Parse error"},"id":null}"#
                .to_string();
        let io = jsonrpc_io_handler();
        let response = io.handle_request_sync(empty_string);
        assert_eq!(response, Some(parse_error));
    }

    #[test]
    fn inventory_method() {
        // The expected behaviour of the inventory method
        use witnet_data_structures::chain::*;
        let signature = Signature::Secp256k1(Secp256k1Signature {
            r: [0; 32],
            s: [0; 32],
            v: 0,
        });
        let keyed_signatures = vec![KeyedSignature {
            public_key: [0; 32],
            signature,
        }];

        let value_transfer_input = Input::ValueTransfer(ValueTransferInput {
            output_index: 0,
            transaction_id: [0; 32],
        });

        let reveal_input = Input::Reveal(RevealInput {
            nonce: 0,
            output_index: 0,
            reveal: [0; 32],
            transaction_id: [0; 32],
        });
        let tally_input = Input::Tally(TallyInput {
            output_index: 0,
            transaction_id: [0; 32],
        });

        let commit_input = Input::Commit(CommitInput {
            output_index: 0,
            poe: [0; 32],
            transaction_id: [0; 32],
        });

        let value_transfer_output = Output::ValueTransfer(ValueTransferOutput {
            pkh: Hash::SHA256([0; 32]),
            value: 0,
        });

        let data_request_output = Output::DataRequest(DataRequestOutput {
            backup_witnesses: 0,
            commit_fee: 0,
            data_request: [0; 32],
            reveal_fee: 0,
            tally_fee: 0,
            time_lock: 0,
            value: 0,
            witnesses: 0,
        });
        let commit_output = Output::Commit(CommitOutput {
            commitment: Hash::SHA256([0; 32]),
            value: 0,
        });
        let reveal_output = Output::Reveal(RevealOutput {
            pkh: Hash::SHA256([0; 32]),
            reveal: [0; 32],
            value: 0,
        });
        let consensus_output = Output::Consensus(ConsensusOutput {
            pkh: Hash::SHA256([0; 32]),
            result: [0; 32],
            value: 0,
        });

        let inputs = vec![
            value_transfer_input,
            reveal_input,
            tally_input,
            commit_input,
        ];
        let outputs = vec![
            value_transfer_output,
            data_request_output,
            commit_output,
            reveal_output,
            consensus_output,
        ];
        let txns: Vec<Transaction> = vec![Transaction {
            inputs,
            signatures: keyed_signatures,
            outputs,
            version: 0,
        }];
        let block = Block {
            block_header: BlockHeader {
                version: 1,
                beacon: CheckpointBeacon {
                    checkpoint: 2,
                    hash_prev_block: Hash::SHA256([4; 32]),
                },
                hash_merkle_root: Hash::SHA256([3; 32]),
            },
            proof: LeadershipProof {
                block_sig: None,
                influence: 99999,
            },
            txns,
        };

        let inv_elem = InventoryItem::Block(block);
        let s = serde_json::to_string(&inv_elem).unwrap();
        let msg = format!(
            r#"{{"jsonrpc":"2.0","method":"inventory","params":{},"id":1}}"#,
            s
        );

        // Expected result: true
        let expected = r#"{"jsonrpc":"2.0","result":true,"id":1}"#.to_string();
        let io = jsonrpc_io_handler();
        let response = io.handle_request_sync(&msg);
        assert_eq!(response, Some(expected));
    }

    #[test]
    fn inventory_invalid_params() {
        // What happens when the inventory method is called with an invalid parameter?
        let msg = r#"{"jsonrpc":"2.0","method":"inventory","params":{ "header": 0 },"id":1}"#;
        let expected = r#"{"jsonrpc":"2.0","error":{"code":-32602,"message":"Invalid params: unknown variant `header`, expected one of"#.to_string();
        let io = jsonrpc_io_handler();
        let response = io.handle_request_sync(&msg);
        // Compare only the first N characters
        let response =
            response.map(|s| s.chars().take(expected.chars().count()).collect::<String>());
        assert_eq!(response, Some(expected));
    }

    #[test]
    fn inventory_unimplemented_type() {
        // What happens when the inventory method is called with an unimplemented type?
        let msg = r#"{"jsonrpc":"2.0","method":"inventory","params":{ "tx": null },"id":1}"#;
        let expected =
            r#"{"jsonrpc":"2.0","error":{"code":-32602,"message":"Item type not implemented"#
                .to_string();
        let io = jsonrpc_io_handler();
        let response = io.handle_request_sync(&msg);
        // Compare only the first N characters
        let response =
            response.map(|s| s.chars().take(expected.chars().count()).collect::<String>());
        assert_eq!(response, Some(expected));
    }

    #[test]
    fn serialize_block() {
        // Check that the serialization of `Block` doesn't change
        use witnet_data_structures::chain::*;
        let signature = Signature::Secp256k1(Secp256k1Signature {
            r: [0; 32],
            s: [0; 32],
            v: 0,
        });
        let keyed_signatures = vec![KeyedSignature {
            public_key: [0; 32],
            signature,
        }];
        let value_transfer_input = Input::ValueTransfer(ValueTransferInput {
            output_index: 0,
            transaction_id: [0; 32],
        });
        let reveal_input = Input::Reveal(RevealInput {
            nonce: 0,
            output_index: 0,
            reveal: [0; 32],
            transaction_id: [0; 32],
        });
        let tally_input = Input::Tally(TallyInput {
            output_index: 0,
            transaction_id: [0; 32],
        });
        let commit_input = Input::Commit(CommitInput {
            output_index: 0,
            poe: [0; 32],
            transaction_id: [0; 32],
        });
        let value_transfer_output = Output::ValueTransfer(ValueTransferOutput {
            pkh: Hash::SHA256([0; 32]),
            value: 0,
        });
        let data_request_output = Output::DataRequest(DataRequestOutput {
            backup_witnesses: 0,
            commit_fee: 0,
            data_request: [0; 32],
            reveal_fee: 0,
            tally_fee: 0,
            time_lock: 0,
            value: 0,
            witnesses: 0,
        });
        let commit_output = Output::Commit(CommitOutput {
            commitment: Hash::SHA256([0; 32]),
            value: 0,
        });
        let reveal_output = Output::Reveal(RevealOutput {
            pkh: Hash::SHA256([0; 32]),
            reveal: [0; 32],
            value: 0,
        });
        let consensus_output = Output::Consensus(ConsensusOutput {
            pkh: Hash::SHA256([0; 32]),
            result: [0; 32],
            value: 0,
        });

        let inputs = vec![
            value_transfer_input,
            reveal_input,
            tally_input,
            commit_input,
        ];
        let outputs = vec![
            value_transfer_output,
            data_request_output,
            commit_output,
            reveal_output,
            consensus_output,
        ];
        let txns: Vec<Transaction> = vec![Transaction {
            inputs,
            signatures: keyed_signatures,
            outputs,
            version: 0,
        }];
        let block = Block {
            block_header: BlockHeader {
                version: 1,
                beacon: CheckpointBeacon {
                    checkpoint: 2,
                    hash_prev_block: Hash::SHA256([4; 32]),
                },
                hash_merkle_root: Hash::SHA256([3; 32]),
            },
            proof: LeadershipProof {
                block_sig: None,
                influence: 99999,
            },
            txns,
        };
        let inv_elem = InventoryItem::Block(block);
        let s = serde_json::to_string(&inv_elem);
        let expected = r#"{"block":{"block_header":{"version":1,"beacon":{"checkpoint":2,"hash_prev_block":{"SHA256":[4,4,4,4,4,4,4,4,4,4,4,4,4,4,4,4,4,4,4,4,4,4,4,4,4,4,4,4,4,4,4,4]}},"hash_merkle_root":{"SHA256":[3,3,3,3,3,3,3,3,3,3,3,3,3,3,3,3,3,3,3,3,3,3,3,3,3,3,3,3,3,3,3,3]}},"proof":{"block_sig":null,"influence":99999},"txns":[{"version":0,"inputs":[{"ValueTransfer":{"transaction_id":[0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0],"output_index":0}},{"Reveal":{"transaction_id":[0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0],"output_index":0,"reveal":[0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0],"nonce":0}},{"Tally":{"transaction_id":[0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0],"output_index":0}},{"Commit":{"transaction_id":[0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0],"output_index":0,"poe":[0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0]}}],"outputs":[{"ValueTransfer":{"pkh":{"SHA256":[0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0]},"value":0}},{"DataRequest":{"data_request":[0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0],"value":0,"witnesses":0,"backup_witnesses":0,"commit_fee":0,"reveal_fee":0,"tally_fee":0,"time_lock":0}},{"Commit":{"commitment":{"SHA256":[0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0]},"value":0}},{"Reveal":{"reveal":[0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0],"pkh":{"SHA256":[0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0]},"value":0}},{"Consensus":{"result":[0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0],"pkh":{"SHA256":[0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0]},"value":0}}],"signatures":[{"signature":{"Secp256k1":{"r":[0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0],"s":[0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0],"v":0}},"public_key":[0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0]}]}]}}"#;
        assert_eq!(s.unwrap(), expected);
    }
}
