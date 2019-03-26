//! Tests to check that common types like Block are storable
// This tests exist because the Rust implementation of MessagePack has some bugs,
// and also because in JSON the keys of a map must be strings.

use std::collections::BTreeMap;
use witnet_data_structures::chain::{Block, ChainState, Hash};
use witnet_storage::storage::Storable;

fn build_hardcoded_block(checkpoint: u32, influence: u64, hash_prev_block: Hash) -> Block {
    use witnet_data_structures::chain::*;
    let signature = Signature::Secp256k1(Secp256k1Signature {
        r: [0; 32],
        s: [0; 32],
        v: 0,
    });
    let keyed_signature = vec![KeyedSignature {
        public_key: [0; 32],
        signature: signature.clone(),
    }];

    let reveal_input = Input::Reveal(RevealInput {
        output_index: 0,
        transaction_id: Hash::SHA256([0; 32]),
    });

    let commit_input = Input::Commit(CommitInput {
        nonce: 0,
        output_index: 0,
        reveal: [0; 32].to_vec(),
        transaction_id: Hash::SHA256([0; 32]),
    });

    let data_request_input = Input::DataRequest(DataRequestInput {
        output_index: 0,
        poe: [0; 32],
        transaction_id: Hash::SHA256([0; 32]),
    });
    let value_transfer_output = Output::ValueTransfer(ValueTransferOutput {
        pkh: [0; 20],
        value: 0,
    });
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
    let data_request_output = Output::DataRequest(DataRequestOutput {
        backup_witnesses: 0,
        commit_fee: 0,
        data_request: rad_request,
        pkh: [0; 20],
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
        pkh: [0; 20],
        reveal: [0; 32].to_vec(),
        value: 0,
    });
    let consensus_output = Output::Tally(TallyOutput {
        pkh: [0; 20],
        result: [0; 32].to_vec(),
        value: 0,
    });

    let inputs = vec![commit_input, data_request_input, reveal_input];
    let outputs = vec![
        value_transfer_output,
        data_request_output,
        commit_output,
        reveal_output,
        consensus_output,
    ];
    let txns = vec![Transaction::new(
        TransactionBody::new(0, inputs, outputs),
        keyed_signature,
    )];
    let proof = LeadershipProof {
        block_sig: Some(signature),
        influence,
    };

    Block {
        block_header: BlockHeader {
            version: 1,
            beacon: CheckpointBeacon {
                checkpoint,
                hash_prev_block,
            },
            hash_merkle_root: Hash::SHA256([222; 32]),
        },
        proof,
        txns,
    }
}

#[test]
fn block_storable() {
    use witnet_data_structures::chain::*;

    let b = InventoryItem::Block(build_hardcoded_block(0, 0, Hash::SHA256([111; 32])));
    let msp = b.to_bytes().unwrap();
    assert_eq!(InventoryItem::from_bytes(&msp).unwrap(), b);
}

#[test]
fn block_storable_fail() {
    use witnet_data_structures::chain::Hash::SHA256;
    use witnet_data_structures::chain::Signature::Secp256k1;
    use witnet_data_structures::chain::*;

    let mined_block = InventoryItem::Block(Block {
        block_header: BlockHeader {
            version: 0,
            beacon: CheckpointBeacon {
                checkpoint: 400,
                hash_prev_block: SHA256([
                    47, 17, 139, 130, 7, 164, 151, 185, 64, 43, 88, 183, 53, 213, 38, 89, 76, 66,
                    231, 53, 78, 216, 230, 217, 245, 184, 150, 33, 182, 15, 111, 38,
                ]),
            },
            hash_merkle_root: SHA256([
                227, 176, 196, 66, 152, 252, 28, 20, 154, 251, 244, 200, 153, 111, 185, 36, 39,
                174, 65, 228, 100, 155, 147, 76, 164, 149, 153, 27, 120, 82, 184, 85,
            ]),
        },
        proof: LeadershipProof {
            block_sig: Some(Secp256k1(Secp256k1Signature {
                r: [
                    128, 205, 5, 48, 74, 223, 4, 72, 223, 231, 60, 90, 128, 196, 37, 74, 225, 60,
                    123, 112, 167, 2, 28, 201, 210, 41, 9, 128, 136, 223, 228, 35,
                ],
                s: [
                    128, 205, 5, 48, 74, 223, 4, 72, 223, 231, 60, 90, 128, 196, 37, 74, 225, 60,
                    123, 112, 167, 2, 28, 201, 210, 41, 9, 128, 136, 223, 228, 35,
                ],
                v: 0,
            })),
            influence: 0,
        },
        txns: vec![],
    });
    let msp = mined_block.to_bytes().unwrap();

    assert_eq!(InventoryItem::from_bytes(&msp).unwrap(), mined_block);
}

#[test]
fn leadership_storable() {
    use witnet_data_structures::chain::*;
    let signed_beacon_hash = [4; 32];

    let signature = Signature::Secp256k1(Secp256k1Signature {
        r: signed_beacon_hash,
        s: signed_beacon_hash,
        v: 0,
    });
    let a = LeadershipProof {
        block_sig: Some(signature),
        influence: 0,
    };

    let msp = a.to_bytes().unwrap();

    assert_eq!(LeadershipProof::from_bytes(&msp).unwrap(), a);
}

#[test]
fn signature_storable() {
    use witnet_data_structures::chain::*;
    let signed_beacon_hash = [4; 32];

    let a = Some(Signature::Secp256k1(Secp256k1Signature {
        r: signed_beacon_hash,
        s: signed_beacon_hash,
        v: 0,
    }));
    let msp = a.to_bytes().unwrap();

    assert_eq!(Option::<Signature>::from_bytes(&msp).unwrap(), a);
}

#[test]
fn nested_option_storable() {
    use witnet_storage::storage::Storable;

    let a = Some(Some(1u8));
    let msp = a.to_bytes().unwrap();
    assert_eq!(Option::<Option<u8>>::from_bytes(&msp).unwrap(), a);
}

#[test]
fn empty_chain_state_to_bytes() {
    use witnet_storage::storage::Storable;

    let chain_state = ChainState::default();

    assert!(chain_state.to_bytes().is_ok());
}

#[test]
fn chain_state_to_bytes() {
    use witnet_data_structures::chain::*;
    use witnet_storage::storage::Storable;

    let chain_state = ChainState {
        chain_info: Some(ChainInfo {
            environment: Environment::Mainnet,
            consensus_constants: ConsensusConstants {
                checkpoint_zero_timestamp: 0,
                checkpoints_period: 0,
                genesis_hash: Hash::default(),
                reputation_demurrage: 0.0,
                reputation_punishment: 0.0,
                max_block_weight: 0,
            },
            highest_block_checkpoint: CheckpointBeacon {
                checkpoint: 0,
                hash_prev_block: Hash::default(),
            },
        }),
        unspent_outputs_pool: UnspentOutputsPool::default(),
        data_request_pool: ActiveDataRequestPool::default(),
        block_chain: BTreeMap::default(),
    };

    assert!(chain_state.to_bytes().is_ok());
}

#[test]
fn chain_state_with_chain_info_to_bytes() {
    use witnet_data_structures::chain::*;

    let chain_state = ChainState {
        chain_info: Some(ChainInfo {
            environment: Environment::Testnet1,
            consensus_constants: ConsensusConstants {
                checkpoint_zero_timestamp: 1546427376,
                checkpoints_period: 10,
                genesis_hash: Hash::SHA256([
                    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
                    0, 0, 0, 0, 0, 0,
                ]),
                reputation_demurrage: 0.0,
                reputation_punishment: 0.0,
                max_block_weight: 10000,
            },
            highest_block_checkpoint: CheckpointBeacon {
                checkpoint: 122533,
                hash_prev_block: Hash::SHA256([
                    239, 173, 3, 247, 9, 44, 43, 68, 13, 51, 67, 110, 79, 191, 165, 135, 157, 167,
                    155, 126, 49, 39, 120, 119, 206, 75, 15, 74, 97, 167, 220, 214,
                ]),
            },
        }),
        unspent_outputs_pool: UnspentOutputsPool::default(),
        data_request_pool: ActiveDataRequestPool::default(),
        block_chain: BTreeMap::default(),
    };

    assert!(chain_state.to_bytes().is_ok());
}

#[test]
fn chain_state_with_utxo_to_bytes() {
    use witnet_data_structures::chain::*;

    let mut utxo = UnspentOutputsPool::default();
    utxo.insert(
        OutputPointer {
            transaction_id: Hash::SHA256([
                191, 75, 125, 95, 27, 78, 216, 89, 168, 222, 88, 21, 171, 139, 44, 170, 127, 120,
                139, 142, 98, 209, 129, 129, 16, 52, 0, 62, 43, 116, 67, 245,
            ]),
            output_index: 0,
        },
        Output::ValueTransfer(ValueTransferOutput {
            pkh: [0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0],
            value: 50000000000,
        }),
    );

    let chain_state = ChainState {
        chain_info: Some(ChainInfo {
            environment: Environment::Testnet1,
            consensus_constants: ConsensusConstants {
                checkpoint_zero_timestamp: 1546427376,
                checkpoints_period: 10,
                genesis_hash: Hash::SHA256([
                    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
                    0, 0, 0, 0, 0, 0,
                ]),
                reputation_demurrage: 0.0,
                reputation_punishment: 0.0,
                max_block_weight: 10000,
            },
            highest_block_checkpoint: CheckpointBeacon {
                checkpoint: 122533,
                hash_prev_block: Hash::SHA256([
                    239, 173, 3, 247, 9, 44, 43, 68, 13, 51, 67, 110, 79, 191, 165, 135, 157, 167,
                    155, 126, 49, 39, 120, 119, 206, 75, 15, 74, 97, 167, 220, 214,
                ]),
            },
        }),
        unspent_outputs_pool: utxo,
        data_request_pool: ActiveDataRequestPool::default(),
        block_chain: BTreeMap::default(),
    };

    assert!(chain_state.to_bytes().is_ok());
}

#[test]
fn utxo_to_bytes() {
    use witnet_data_structures::chain::*;

    let mut utxo = UnspentOutputsPool::default();
    utxo.insert(
        OutputPointer {
            transaction_id: Hash::SHA256([
                191, 75, 125, 95, 27, 78, 216, 89, 168, 222, 88, 21, 171, 139, 44, 170, 127, 120,
                139, 142, 98, 209, 129, 129, 16, 52, 0, 62, 43, 116, 67, 245,
            ]),
            output_index: 0,
        },
        Output::ValueTransfer(ValueTransferOutput {
            pkh: [0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0],
            value: 50000000000,
        }),
    );

    assert!(utxo.to_bytes().is_ok());
}

#[test]
fn output_pointer_to_bytes() {
    use witnet_data_structures::chain::*;

    let output_pointer = OutputPointer {
        transaction_id: Hash::SHA256([
            191, 75, 125, 95, 27, 78, 216, 89, 168, 222, 88, 21, 171, 139, 44, 170, 127, 120, 139,
            142, 98, 209, 129, 129, 16, 52, 0, 62, 43, 116, 67, 245,
        ]),
        output_index: 0,
    };

    assert!(output_pointer.to_bytes().is_ok());
}

#[test]
fn output_to_bytes() {
    use witnet_data_structures::chain::*;

    let output = Output::ValueTransfer(ValueTransferOutput {
        pkh: [0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0],
        value: 50000000000,
    });

    assert!(output.to_bytes().is_ok());
}
