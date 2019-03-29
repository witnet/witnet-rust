use actix::{
    ActorFuture, AsyncContext, Context, ContextFutureSpawner, Handler, System, WrapFuture,
};
use ansi_term::Color::{White, Yellow};
use log::{debug, error, info, warn};
use serde_json;

use futures::future::{join_all, Future};
use std::{collections::HashMap, time::Duration};

use super::ChainManager;
use crate::actors::{
    messages::{
        AddCandidates, AddTransaction, GetHighestCheckpointBeacon, ResolveRA, RunConsensus,
    },
    rad_manager::RadManager,
};

use witnet_crypto::hash::calculate_sha256;
use witnet_data_structures::{
    chain::{
        Block, BlockHeader, CheckpointBeacon, Hash, Hashable, Input, KeyedSignature,
        LeadershipProof, Output, OutputPointer, PublicKeyHash, Secp256k1Signature, Signature,
        Transaction, TransactionsPool, UnspentOutputsPool, ValueTransferOutput,
    },
    data_request::{create_commit_body, create_reveal_body, create_tally_body, create_vt_tally},
    serializers::decoders::TryFrom,
};
use witnet_rad::types::RadonTypes;
use witnet_validations::validations::{
    block_reward, merkle_tree_root, transaction_fee, validate_block, verify_poe_data_request,
};

impl ChainManager {
    /// Try to mine a block
    pub fn try_mine_block(&mut self, ctx: &mut Context<Self>) {
        if self.current_epoch.is_none() {
            warn!("Cannot mine a block because current epoch is unknown");

            return;
        }

        let current_epoch = self.current_epoch.unwrap();

        debug!("Periodic epoch notification received {:?}", current_epoch);

        // Check eligibility
        // S(H(beacon))
        let mut beacon = match self.handle(GetHighestCheckpointBeacon, ctx) {
            Ok(b) => b,
            _ => return,
        };

        if beacon.checkpoint > current_epoch {
            // We got a block from the future
            error!(
                "The current highest checkpoint beacon is from the future ({:?} > {:?})",
                beacon.checkpoint, current_epoch
            );
            return;
        }
        if beacon.checkpoint == current_epoch {
            // For some reason we already got a valid block for this epoch
            // TODO: Check eligibility anyway?
        }
        // The highest checkpoint beacon should contain the current epoch
        beacon.checkpoint = current_epoch;
        let beacon_hash = Hash::from(calculate_sha256(&serde_json::to_vec(&beacon).unwrap()));
        let private_key = 1;

        // TODO: send Sign message to CryptoManager
        let sign = |x, _k| match x {
            Hash::SHA256(mut x) => {
                // Add some randomness to the signature
                // TODO: since the hash of the block depends only on the block header,
                // this does not change the hash. Therefore, until we implement signatures,
                // all the nodes will always mine blocks with the same hash.
                x[0] = self.random as u8;
                x
            }
        };
        let signed_beacon_hash = sign(beacon_hash, private_key);
        // Currently, every hash is valid
        // Fake signature which will be accepted anyway
        let signature = Signature::Secp256k1(Secp256k1Signature {
            r: signed_beacon_hash,
            s: signed_beacon_hash,
            v: 0,
        });
        let leadership_proof = LeadershipProof {
            block_sig: Some(signature),
            influence: 0,
        };

        // TODO: Use a real PoE
        let poe = true;
        if poe {
            // FIXME (tmpolaczyk): block creation must happen after data request mining
            // (we must wait for all the potential nodes to sent their transactions)
            // The best way would be to start mining a few seconds _before_ the epoch
            // checkpoint, but for simplicity we just wait for 5 seconds after the checkpoint
            ctx.run_later(Duration::from_secs(5), move |act, ctx| {
                info!(
                    "{} Discovered eligibility for mining a block for epoch #{}",
                    Yellow.bold().paint("[Mining]"),
                    Yellow.bold().paint(beacon.checkpoint.to_string())
                );
                // Send proof of eligibility to chain manager,
                // which will construct and broadcast the block

                act.create_tally_transactions()
                    .into_actor(act)
                    .and_then(move |tally_transactions, act, ctx| {
                        // Build the block using the supplied beacon and eligibility proof
                        let block = build_block(
                            &act.transactions_pool,
                            &act.chain_state.unspent_outputs_pool,
                            act.max_block_weight,
                            beacon,
                            leadership_proof,
                            &tally_transactions,
                        );

                        match validate_block(
                            &block,
                            current_epoch,
                            beacon,
                            act.genesis_block_hash,
                            &act.chain_state.unspent_outputs_pool,
                            &act.transactions_pool,
                            &act.data_request_pool,
                        ) {
                            Ok(_) => {
                                // Send AddCandidates message to self
                                // This will run all the validations again
                                act.handle(
                                    AddCandidates {
                                        blocks: vec![block],
                                    },
                                    ctx,
                                );
                            }

                            Err(e) => error!("Error trying to mine a block: {}", e),
                        }

                        actix::fut::ok(())
                    })
                    .wait(ctx);
            });
        }
    }

    /// Try to mine a data_request
    // TODO: refactor this procedure into multiple functions that can be tested separately.
    pub fn try_mine_data_request(&mut self, ctx: &mut Context<Self>) {
        if self.current_epoch.is_none() {
            warn!("Cannot mine a data request because current epoch is unknown");

            return;
        }

        let current_epoch = self.current_epoch.unwrap();

        // Data Request mining
        let dr_output_pointers = self
            .data_request_pool
            .get_dr_output_pointers_by_epoch(current_epoch);

        for dr_output_pointer in dr_output_pointers {
            let data_request_output = self.data_request_pool.get_dr_output(&dr_output_pointer);

            if data_request_output.is_some() && verify_poe_data_request() {
                let data_request_output = data_request_output.unwrap();
                let rad_request = data_request_output.data_request.clone();

                // Send ResolveRA message to RADManager
                let rad_manager_addr = System::current().registry().get::<RadManager>();
                rad_manager_addr
                    .send(ResolveRA {
                        rad_request,
                    })
                    .into_actor(self)
                    .then(move |res, act, ctx| match res {
                        // Process the response from RADManager
                        Err(e) => {
                            // Error when sending message
                            error!("Unsuccessful communication with RADManager: {}", e);
                            actix::fut::err(())
                        }
                        Ok(res) => match res {
                            Err(e) => {
                                // Error executing the rad_request
                                error!("RadManager error: {}", e);
                                actix::fut::err(())
                            }

                            Ok(reveal_value) => {
                                // Create commitment transaction
                                let commit_body = create_commit_body(&dr_output_pointer, &data_request_output, reveal_value.clone());
                                // TODO: produce real signature
                                let sig = KeyedSignature::default();
                                let commit_transaction = Transaction::new(commit_body, vec![sig]);

                                // Create reveal transaction
                                let commit_pointer = OutputPointer {
                                    transaction_id: commit_transaction.hash(),
                                    output_index: 0,
                                };
                                let reveal_body = create_reveal_body(commit_pointer,  &data_request_output, reveal_value);
                                // TODO: produce real signature
                                let sig = KeyedSignature::default();
                                let reveal_transaction = Transaction::new(reveal_body, vec![sig]);

                                // Hold reveal transaction under "waiting_for_reveal" field of data requests pool
                                act.data_request_pool.insert_reveal(dr_output_pointer.clone(), reveal_transaction);

                                info!(
                                    "{} Discovered eligibility for mining a data request {} for epoch #{}",
                                    Yellow.bold().paint("[Mining]"),
                                    Yellow.bold().paint(dr_output_pointer.to_string()),
                                    Yellow.bold().paint(current_epoch.to_string())
                                );

                                // Send AddTransaction message to self
                                // And broadcast it to all of peers
                                act.handle(
                                    AddTransaction {
                                        transaction: commit_transaction,
                                    },
                                    ctx,
                                );

                                actix::fut::ok(())
                            }
                        },
                    })
                    .wait(ctx)
            }
        }
    }

    fn create_tally_transactions(&mut self) -> impl Future<Item = Vec<Transaction>, Error = ()> {
        let data_request_pool = &self.data_request_pool;
        let utxo = &self.chain_state.unspent_outputs_pool;

        // Include Tally transactions, one for each data request in tally stage
        let mut future_tally_transactions = vec![];
        let dr_reveals = data_request_pool.get_all_reveals(&utxo);
        for ((dr_pointer, dr_output), reveals) in dr_reveals {
            debug!("Building tally for data request {}", dr_pointer);

            let (inputs, outputs, results) = create_vt_tally(&dr_output, reveals);

            let rad_manager_addr = System::current().registry().get::<RadManager>();
            let fut = rad_manager_addr
                .send(RunConsensus {
                    script: dr_output.data_request.consensus.clone(),
                    reveals: results.clone(),
                })
                .then(move |res| match res {
                    // Process the response from RADManager
                    Err(e) => {
                        // Error when sending message
                        error!("Unsuccessful communication with RADManager: {}", e);
                        futures::future::err(())
                    }
                    Ok(res) => match res {
                        Err(e) => {
                            // Error executing the RAD consensus
                            error!("RadManager error: {}", e);
                            futures::future::err(())
                        }

                        Ok(consensus) => {
                            let tally_body =
                                create_tally_body(&dr_output, inputs, outputs, consensus.clone());
                            // TODO: replace with actual call to signature manager
                            let tally_transaction = Transaction::new(tally_body, vec![]);

                            let print_results: Vec<_> = results
                                .into_iter()
                                .map(|result| RadonTypes::try_from(result.as_slice()))
                                .collect();
                            info!(
                                "{} Created Tally for Data Request {} with result: {}\n{}",
                                Yellow.bold().paint("[Data Request]"),
                                Yellow.bold().paint(&dr_pointer.to_string()),
                                Yellow.bold().paint(
                                    RadonTypes::try_from(consensus.as_slice())
                                        .map(|x| x.to_string())
                                        .unwrap_or_else(|_| "RADError".to_string())
                                ),
                                White.bold().paint(
                                    print_results
                                        .into_iter()
                                        .map(|result| result
                                            .map(|x| x.to_string())
                                            .unwrap_or_else(|_| "RADError".to_string()))
                                        .fold("Reveals:".to_string(), |acc, item| format!(
                                            "{}\n\t* {}",
                                            acc, item
                                        ))
                                ),
                            );

                            futures::future::ok(tally_transaction)
                        }
                    },
                });
            future_tally_transactions.push(fut);
        }

        join_all(future_tally_transactions)
    }
}

/// Build a new Block using the supplied leadership proof and by filling transactions from the
/// `transaction_pool`
fn build_block(
    transactions_pool: &TransactionsPool,
    unspent_outputs_pool: &UnspentOutputsPool,
    max_block_weight: u32,
    beacon: CheckpointBeacon,
    proof: LeadershipProof,
    tally_transactions: &[Transaction],
) -> Block {
    // Get all the unspent transactions and calculate the sum of their fees
    let mut transaction_fees = 0;
    let mut block_weight = 0;
    let mut transactions = Vec::new();

    // Insert empty Transaction (future Mint Transaction)
    transactions.push(Transaction::default());

    // Keep track of the commitments for each data request
    let mut witnesses_per_dr = HashMap::new();

    // Push transactions from pool until `max_block_weight` is reached
    // TODO: refactor this statement into a functional `try_fold`
    for transaction in tally_transactions.iter().chain(transactions_pool.iter()) {
        debug!("Pushing transaction into block: {:?}", transaction);
        // Currently, 1 weight unit is equivalent to 1 byte
        let transaction_weight = transaction.size();
        let transaction_fee = match transaction_fee(&transaction.body, unspent_outputs_pool) {
            Ok(x) => x,
            Err(e) => {
                warn!(
                    "Error when calculating transaction fee for transaction: {}",
                    e
                );
                continue;
            }
        };
        let new_block_weight = block_weight + transaction_weight;

        if new_block_weight <= max_block_weight {
            if let Input::DataRequest(dri) = &transaction.body.inputs[0] {
                let dri_pointer = dri.output_pointer();
                if let Some(dr) = unspent_outputs_pool.get(&dri_pointer) {
                    if let Output::DataRequest(dr) = dr {
                        let w = dr.witnesses;
                        let new_w = witnesses_per_dr.entry(dri_pointer).or_insert(0);
                        if *new_w < w {
                            // Ok, push commitment
                            *new_w += 1;
                            transactions.push(transaction.clone());
                            transaction_fees += transaction_fee;
                            block_weight += transaction_weight;
                        }
                    }
                }
            } else {
                transactions.push(transaction.clone());
                transaction_fees += transaction_fee;
                block_weight += transaction_weight;
            }

            if new_block_weight == max_block_weight {
                break;
            }
        }
    }

    // Include Mint Transaction by miner
    // TODO: Include Witnet's node PKH (keyed signature is not needed as there is no input)
    let pkh = PublicKeyHash::default();
    let epoch = beacon.checkpoint;
    let reward = block_reward(epoch) + transaction_fees;

    // Build Mint Transaction
    transactions[0]
        .body
        .outputs
        .push(Output::ValueTransfer(ValueTransferOutput {
            pkh,
            value: reward,
        }));

    // Compute `hash_merkle_root` and build block header
    let hash_merkle_root = merkle_tree_root(&transactions);
    let block_header = BlockHeader {
        version: 0,
        beacon,
        hash_merkle_root,
    };

    Block {
        block_header,
        proof,
        txns: transactions,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use witnet_data_structures::chain::*;

    #[test]
    fn build_empty_block() {
        // Initialize transaction_pool with 1 transaction
        let mut transaction_pool = TransactionsPool::default();
        // In protocol buffers, when version is 0 and all the other fields are empty vectors, the
        // transaction size is 0 bytes (since missing fields are initialized with the default
        // values). Therefore version cannot be 0.
        let transaction = Transaction::default();
        transaction_pool.insert(transaction.hash(), transaction.clone());

        let unspent_outputs_pool = UnspentOutputsPool::default();

        // Set `max_block_weight` to zero (no transaction should be included)
        let max_block_weight = 0;

        // Fields required to mine a block
        let block_beacon = CheckpointBeacon::default();
        let block_proof = LeadershipProof {
            block_sig: None,
            influence: 0,
        };

        // Build empty block (because max weight is zero)
        let block = build_block(
            &transaction_pool,
            &unspent_outputs_pool,
            max_block_weight,
            block_beacon,
            block_proof,
            &[],
        );

        // Check if block only contains the Mint Transaction
        assert_eq!(block.txns.len(), 1);
        assert_eq!(block.txns[0].body.inputs.len(), 0);
        assert_eq!(block.txns[0].body.outputs.len(), 1);
        assert_eq!(block.txns[0].signatures.len(), 0);

        // Check that transaction in block is not the transaction in `transactions_pool`
        assert_ne!(block.txns[0], transaction);
    }

    #[test]
    #[ignore]
    fn build_block_with_transactions() {
        // Build sample transactions
        let transaction_1 = Transaction::new(
            TransactionBody::new(
                0,
                vec![Input::ValueTransfer(ValueTransferInput {
                    transaction_id: Hash::SHA256([1; 32]),
                    output_index: 0,
                })],
                vec![Output::ValueTransfer(ValueTransferOutput {
                    pkh: PublicKeyHash::default(),
                    value: 1,
                })],
            ),
            vec![],
        );
        let transaction_2 = Transaction::new(
            TransactionBody::new(
                0,
                vec![
                    Input::ValueTransfer(ValueTransferInput {
                        transaction_id: Hash::SHA256([2; 32]),
                        output_index: 0,
                    }),
                    Input::ValueTransfer(ValueTransferInput {
                        transaction_id: Hash::SHA256([3; 32]),
                        output_index: 0,
                    }),
                ],
                vec![
                    Output::ValueTransfer(ValueTransferOutput {
                        pkh: PublicKeyHash::default(),
                        value: 2,
                    }),
                    Output::ValueTransfer(ValueTransferOutput {
                        pkh: PublicKeyHash::default(),
                        value: 3,
                    }),
                ],
            ),
            vec![],
        );
        let transaction_3 = Transaction::new(
            TransactionBody::new(
                0,
                vec![
                    Input::ValueTransfer(ValueTransferInput {
                        transaction_id: Hash::SHA256([4; 32]),
                        output_index: 0,
                    }),
                    Input::ValueTransfer(ValueTransferInput {
                        transaction_id: Hash::SHA256([5; 32]),
                        output_index: 0,
                    }),
                ],
                vec![
                    Output::ValueTransfer(ValueTransferOutput {
                        pkh: PublicKeyHash::default(),
                        value: 4,
                    }),
                    Output::ValueTransfer(ValueTransferOutput {
                        pkh: PublicKeyHash::default(),
                        value: 5,
                    }),
                ],
            ),
            vec![],
        );

        // Insert transactions into `transactions_pool`
        // TODO: Currently the insert function does not take into account the fees to compute the transaction's weight
        let mut transaction_pool = TransactionsPool::default();
        transaction_pool.insert(transaction_1.hash(), transaction_1.clone());
        transaction_pool.insert(transaction_2.hash(), transaction_2.clone());
        transaction_pool.insert(transaction_3.hash(), transaction_3.clone());

        let unspent_outputs_pool = UnspentOutputsPool::default();

        // Set `max_block_weight` to fit only `transaction_1` size
        let max_block_weight = transaction_1.size();

        // Fields required to mine a block
        let block_beacon = CheckpointBeacon::default();
        let block_proof = LeadershipProof {
            block_sig: None,
            influence: 0,
        };

        // Build block with
        let block = build_block(
            &transaction_pool,
            &unspent_outputs_pool,
            max_block_weight,
            block_beacon,
            block_proof,
            &[],
        );

        // Check if block contains only 2 transactions (Mint Transaction + 1 included transaction)
        assert_eq!(block.txns.len(), 2);

        // Check that first transaction is the Mint Transaction
        assert_eq!(block.txns[0].body.inputs.len(), 0);
        assert_eq!(block.txns[0].body.outputs.len(), 1);
        assert_eq!(block.txns[0].signatures.len(), 0);
        // Check that transaction in block is not a transaction from `transactions_pool`
        assert_ne!(block.txns[0], transaction_1);
        assert_ne!(block.txns[0], transaction_2);
        assert_ne!(block.txns[0], transaction_3);

        // Check that the included transaction is the only one that fits the `max_block_weight`
        assert_eq!(block.txns[1], transaction_1);
    }
}
