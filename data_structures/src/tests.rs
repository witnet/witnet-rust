use super::chain::*;

#[test]
fn test_block_hashable_trait() {
    let block = block_example();
    let expected = "41d36ff16318f17350b0f0a74afb907bda00b89035d12ccede8ca404a4afb1c0";
    assert_eq!(block.hash().to_string(), expected);
}

#[test]
fn test_transaction_hashable_trait() {
    let transaction = transaction_example();
    let expected = "1fc485f4bb256a104e3d3b47ca0c5a5acacd3123a7d56fbac53efb69094d6353";
    assert_eq!(transaction.hash().to_string(), expected);
}

// TODO(#522): Uncomment and review block/transaction validation tests
/*
mod transaction {
    use super::*;
    use crate::validations::transaction_inputs_sum;
    use crate::validations::transaction_outputs_sum;
    use crate::validations::transaction_is_mint;
    use crate::validations::transaction_fee;

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
        let mut pool = UnspentOutputsPool::new();
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

        assert!(transaction_inputs_sum(&transaction, &pool).is_err());

        pool.insert(
            OutputPointer {
                transaction_id: hash,
                output_index: 0,
            },
            Output::Commit(CommitOutput {
                commitment: hash,
                value: 123,
            }),
        );
        pool.insert(
            OutputPointer {
                transaction_id: hash,
                output_index: 1,
            },
            Output::Commit(CommitOutput {
                commitment: hash,
                value: 10,
            }),
        );
        pool.insert(
            OutputPointer {
                transaction_id: hash,
                output_index: 2,
            },
            Output::Commit(CommitOutput {
                commitment: hash,
                value: 1,
            }),
        );

        assert_eq!(transaction_inputs_sum(&transaction, &pool).unwrap(), 124);
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

        assert_eq!(transaction_outputs_sum(&transaction), 123);
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

        assert!(transaction_is_mint(&transaction));
    }

    #[test]
    fn test_fee() {
        let pool = UnspentOutputsPool::new();


        let transaction = Transaction {
            version: 0,
            signatures: Vec::new(),
            inputs: Vec::new(),
            outputs: vec![Output::Commit(CommitOutput {
                commitment: HASH,
                value: 123,
            })],
        };

        assert_eq!(transaction_fee(&transaction, &pool).unwrap(), 123);
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
    };

    #[test]
    fn test_validate_correct_block() {
        let pool = UnspentOutputsPool::new();
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
        let pool = UnspentOutputsPool::new();
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
        let pool = UnspentOutputsPool::new();
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
        let pool = UnspentOutputsPool::new();
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
        let pool = UnspentOutputsPool::new();
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
*/

#[test]
fn test_input_output_pointer() {
    let input = Input::new(OutputPointer {
        transaction_id: Hash::default(),
        output_index: 123,
    });

    assert_eq!(
        input.output_pointer(),
        OutputPointer {
            transaction_id: Hash::default(),
            output_index: 123
        }
    );
}

#[test]
fn test_output_value() {
    let output = Output::Commit(CommitOutput {
        commitment: Hash::default(),
        value: 123,
    });

    assert_eq!(output.value(), 123);
}
