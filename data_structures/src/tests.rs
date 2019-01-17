use super::chain::*;

const HASH: Hash = Hash::SHA256([0; 32]);

#[test]
fn test_block_hashable_trait() {
    let block_header = BlockHeader {
        version: 0,
        beacon: CheckpointBeacon {
            checkpoint: 0,
            hash_prev_block: HASH,
        },
        hash_merkle_root: HASH,
    };
    let signature = Signature::Secp256k1(Secp256k1Signature {
        r: [0; 32],
        s: [0; 32],
        v: 0,
    });
    let proof = LeadershipProof {
        block_sig: Some(signature.clone()),
        influence: 0,
    };
    let keyed_signatures = vec![KeyedSignature {
        public_key: [0; 32],
        signature,
    }];
    let commit_input = Input::Commit(CommitInput {
        nonce: 0,
        output_index: 0,
        reveal: [0; 32].to_vec(),
        transaction_id: HASH,
    });
    let reveal_input = Input::Reveal(RevealInput {
        output_index: 0,
        transaction_id: HASH,
    });
    let data_request_input = Input::DataRequest(DataRequestInput {
        output_index: 0,
        poe: [0; 32],
        transaction_id: HASH,
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
        commitment: HASH,
        value: 0,
    });
    let reveal_output = Output::Reveal(RevealOutput {
        pkh: [0; 20],
        reveal: [0; 32].to_vec(),
        value: 0,
    });
    let consensus_output = Output::Tally(TallyOutput {
        pkh: [0; 20],
        result: vec![0],
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
    let txns: Vec<Transaction> = vec![Transaction {
        inputs,
        signatures: keyed_signatures,
        outputs,
        version: 0,
    }];
    let block = Block {
        block_header,
        proof,
        txns,
    };
    let expected = Hash::SHA256([
        204, 111, 204, 123, 50, 100, 176, 227, 102, 35, 195, 223, 178, 106, 185, 156, 160, 24, 18,
        210, 236, 116, 217, 170, 103, 95, 92, 236, 208, 52, 134, 63,
    ]);
    assert_eq!(block.hash(), expected);
}

#[test]
fn test_transaction_hashable_trait() {
    let signature = Signature::Secp256k1(Secp256k1Signature {
        r: [0; 32],
        s: [0; 32],
        v: 0,
    });
    let signatures = vec![KeyedSignature {
        public_key: [0; 32],
        signature,
    }];
    let commit_input = Input::Commit(CommitInput {
        nonce: 0,
        output_index: 0,
        reveal: [0; 32].to_vec(),
        transaction_id: HASH,
    });
    let reveal_input = Input::Reveal(RevealInput {
        output_index: 0,
        transaction_id: HASH,
    });
    let data_request_input = Input::DataRequest(DataRequestInput {
        output_index: 0,
        poe: [0; 32],
        transaction_id: HASH,
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
        commitment: HASH,
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
    let transaction: Transaction = Transaction {
        inputs,
        outputs,
        signatures,
        version: 0,
    };
    let expected = Hash::SHA256([
        10, 241, 147, 199, 165, 174, 93, 237, 233, 213, 202, 27, 217, 126, 244, 196, 189, 74, 84,
        243, 4, 214, 2, 34, 22, 0, 118, 115, 137, 32, 203, 237,
    ]);
    assert_eq!(transaction.hash(), expected);
}

mod transaction {
    use super::*;

    #[test]
    fn test_output_value() {
        let transaction = Transaction {
            version: 0,
            signatures: Vec::new(),
            inputs: Vec::new(),
            outputs: vec![Output::Commit(CommitOutput {
                commitment: HASH,
                value: 123,
            })],
        };

        assert_eq!(transaction.get_output_value(0), Some(123));
    }

    #[test]
    fn test_inputs_sum() {
        let mut pool = TransactionsPool::new();
        let hash = HASH;
        let transaction = Transaction {
            version: 0,
            signatures: Vec::new(),
            outputs: Vec::new(),
            inputs: vec![
                Input::Commit(CommitInput {
                    transaction_id: hash,
                    output_index: 0,
                    reveal: vec![],
                    nonce: 0,
                }),
                Input::Commit(CommitInput {
                    transaction_id: hash,
                    output_index: 2,
                    reveal: vec![],
                    nonce: 0,
                }),
            ],
        };

        assert!(transaction.inputs_sum(&pool).is_err());

        pool.insert(
            hash,
            Transaction {
                version: 0,
                signatures: Vec::new(),
                inputs: Vec::new(),
                outputs: vec![
                    Output::Commit(CommitOutput {
                        commitment: hash,
                        value: 123,
                    }),
                    Output::Commit(CommitOutput {
                        commitment: hash,
                        value: 10,
                    }),
                    Output::Commit(CommitOutput {
                        commitment: hash,
                        value: 1,
                    }),
                ],
            },
        );

        assert_eq!(transaction.inputs_sum(&pool).unwrap(), 124);
    }

    #[test]
    fn test_outputs_sum() {
        let transaction = Transaction {
            version: 0,
            signatures: Vec::new(),
            inputs: Vec::new(),
            outputs: vec![Output::Commit(CommitOutput {
                commitment: HASH,
                value: 123,
            })],
        };

        assert_eq!(transaction.outputs_sum(), 123);
    }

    #[test]
    fn test_is_mint() {
        let transaction = Transaction {
            version: 0,
            signatures: Vec::new(),
            inputs: Vec::new(),
            outputs: vec![Output::Commit(CommitOutput {
                commitment: HASH,
                value: 123,
            })],
        };

        assert!(transaction.is_mint());
    }

    #[test]
    fn test_fee() {
        let pool = TransactionsPool::new();
        let transaction = Transaction {
            version: 0,
            signatures: Vec::new(),
            inputs: Vec::new(),
            outputs: vec![Output::Commit(CommitOutput {
                commitment: HASH,
                value: 123,
            })],
        };

        assert_eq!(transaction.fee(&pool).unwrap(), 123);
    }
}

mod block {
    use super::*;

    const HEADER: BlockHeader = BlockHeader {
        version: 0,
        beacon: CheckpointBeacon {
            checkpoint: 0,
            hash_prev_block: HASH,
        },
        hash_merkle_root: HASH,
    };

    const PROOF: LeadershipProof = LeadershipProof {
        block_sig: None,
        influence: 123,
    };

    #[test]
    fn test_validate_correct_block() {
        let pool = TransactionsPool::new();
        let reward = 123;
        let block = Block {
            block_header: HEADER,
            proof: PROOF,
            txns: vec![Transaction {
                version: 0,
                signatures: Vec::new(),
                inputs: Vec::new(),
                outputs: vec![Output::Commit(CommitOutput {
                    commitment: HASH,
                    value: reward,
                })],
            }],
        };

        assert_eq!(block.validate(reward, &pool).unwrap(), ());
    }

    #[test]
    fn test_validate_empty_block() {
        let pool = TransactionsPool::new();
        let reward = 123;
        let block = Block {
            block_header: HEADER,
            proof: PROOF,
            txns: vec![],
        };

        let error = block.validate(reward, &pool).unwrap_err();

        assert!(match error.downcast::<BlockError>() {
            Ok(BlockError::Empty) => true,
            _ => false,
        });
    }

    #[test]
    fn test_validate_block_with_no_mint() {
        let pool = TransactionsPool::new();
        let reward = 123;
        let block = Block {
            block_header: HEADER,
            proof: PROOF,
            txns: vec![Transaction {
                version: 0,
                signatures: Vec::new(),
                inputs: vec![Input::Commit(CommitInput {
                    transaction_id: HASH,
                    output_index: 0,
                    reveal: vec![],
                    nonce: 0,
                })],
                outputs: vec![Output::Commit(CommitOutput {
                    commitment: HASH,
                    value: reward,
                })],
            }],
        };

        let error = block.validate(reward, &pool).unwrap_err();

        assert!(match error.downcast::<BlockError>() {
            Ok(BlockError::NoMint) => true,
            _ => false,
        });
    }

    #[test]
    fn test_validate_block_with_multiple_mint() {
        let pool = TransactionsPool::new();
        let reward = 123;
        let mint = Transaction {
            version: 0,
            signatures: Vec::new(),
            inputs: Vec::new(),
            outputs: vec![Output::Commit(CommitOutput {
                commitment: HASH,
                value: reward,
            })],
        };
        let block = Block {
            block_header: HEADER,
            proof: PROOF,
            txns: vec![mint.clone(), mint.clone()],
        };

        let error = block.validate(reward, &pool).unwrap_err();

        assert!(match error.downcast::<BlockError>() {
            Ok(BlockError::MultipleMint) => true,
            _ => false,
        });
    }

    #[test]
    fn test_validate_block_with_mismatched_mint_value() {
        let pool = TransactionsPool::new();
        let reward = 123;
        let block = Block {
            block_header: HEADER,
            proof: PROOF,
            txns: vec![Transaction {
                version: 0,
                signatures: Vec::new(),
                inputs: Vec::new(),
                outputs: vec![Output::Commit(CommitOutput {
                    commitment: HASH,
                    value: reward,
                })],
            }],
        };

        let error = block.validate(reward - 10, &pool).unwrap_err();

        assert!(match error.downcast::<BlockError>() {
            Ok(BlockError::MismatchedMintValue) => true,
            _ => false,
        });
    }
}

#[test]
fn test_input_output_pointer() {
    let input = Input::ValueTransfer(ValueTransferInput {
        transaction_id: HASH,
        output_index: 123,
    });

    assert_eq!(
        input.output_pointer(),
        OutputPointer {
            transaction_id: HASH,
            output_index: 123
        }
    );
}

#[test]
fn test_output_value() {
    let output = Output::Commit(CommitOutput {
        commitment: HASH,
        value: 123,
    });

    assert_eq!(output.value(), 123);
}
