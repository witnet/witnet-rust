use actix::prelude::*;
use actix::{
    ActorFuture, AsyncContext, Context, ContextFutureSpawner, Handler, System, WrapFuture,
};
use ansi_term::Color::{White, Yellow};
use log::{debug, error, info, warn};

use futures::future::{join_all, Future};
use std::{collections::HashMap, time::Duration};

use super::ChainManager;
use crate::actors::{
    messages::{
        AddCandidates, AddTransaction, GetHighestCheckpointBeacon, ResolveRA, RunConsensus,
    },
    rad_manager::RadManager,
};

use crate::actors::chain_manager::transaction_factory::sign_transaction;
use crate::signature_mngr;

use witnet_data_structures::{
    chain::{
        transaction_tag, Block, BlockHeader, CheckpointBeacon, Hashable, LeadershipProof, Output,
        PublicKeyHash, Transaction, TransactionType, TransactionsPool, UnspentOutputsPool,
        ValueTransferOutput,
    },
    data_request::{create_commit_body, create_reveal_body, create_tally_body, create_vt_tally},
    serializers::decoders::TryFrom,
};
use witnet_rad::types::RadonTypes;
use witnet_validations::validations::{
    block_reward, merkle_tree_root, transaction_fee, validate_block, verify_poe_data_request,
    UtxoDiff,
};

impl ChainManager {
    /// Try to mine a block
    pub fn try_mine_block(&mut self, ctx: &mut Context<Self>) {
        if self.current_epoch.is_none() {
            warn!("Cannot mine a block because current epoch is unknown");

            return;
        }
        if self.own_pkh.is_none() {
            warn!("PublicKeyHash is not set. All mined wits will be lost!");
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
                    .join(
                        signature_mngr::sign(&beacon)
                            .map_err(|e| error!("Couldn't sign beacon: {}", e)),
                    )
                    .into_actor(act)
                    .and_then(move |(tally_transactions, keyed_signature), act, ctx| {
                        let leadership_proof = LeadershipProof {
                            block_sig: keyed_signature,
                        };

                        // Build the block using the supplied beacon and eligibility proof
                        let block = build_block(
                            &act.transactions_pool,
                            &act.chain_state.unspent_outputs_pool,
                            act.max_block_weight,
                            beacon,
                            leadership_proof,
                            &tally_transactions,
                            act.own_pkh.unwrap_or_default(),
                        );

                        match validate_block(
                            &block,
                            current_epoch,
                            beacon,
                            act.genesis_block_hash,
                            &act.chain_state.unspent_outputs_pool,
                            &act.chain_state.data_request_pool,
                        ) {
                            Ok(_) => {
                                // Send AddCandidates message to self
                                // This will run all the validations again

                                let block_hash = block.hash();
                                log::info!(
                                    "Proposed block candidate {}",
                                    Yellow.bold().paint(block_hash.to_string())
                                );
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
        if self.current_epoch.is_none() || self.own_pkh.is_none() {
            warn!("Cannot mine a data request because current epoch or own pkh is unknown");

            return;
        }
        let own_pkh = self.own_pkh.unwrap();

        let current_epoch = self.current_epoch.unwrap();

        // Data Request mining
        let dr_output_pointers = self
            .chain_state
            .data_request_pool
            .get_dr_output_pointers_by_epoch(current_epoch);

        for dr_output_pointer in dr_output_pointers {
            let data_request_output = self
                .chain_state
                .data_request_pool
                .get_dr_output(&dr_output_pointer);

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
                    .then(|result, _, _| match result {
                        Ok(Ok(value)) => fut::ok(value),
                        Ok(Err(e)) => {
                            log::error!("Couldn't resolve rad request: {}", e);
                            fut::err(())
                        }
                        Err(e) => {
                            log::error!("Couldn't resolve rad request: {}", e);
                            fut::err(())
                        }
                    })
                    .and_then(move |reveal_value, act, _ctx| {
                        // Create commitment transaction
                        let commit_body = create_commit_body(dr_output_pointer.clone(), &data_request_output, reveal_value.clone());
                        sign_transaction(commit_body)
                            .map_err(|e| log::error!("Couldn't sign commit body: {}", e))
                            .into_actor(act)
                            .and_then(move |commit_transaction, act, _ctx| {
                                let reveal_body = create_reveal_body(dr_output_pointer.clone(),  &data_request_output, reveal_value, own_pkh);

                                sign_transaction(reveal_body)
                                    .map_err(|e| log::error!("Couldn't sign reveal body: {}", e))
                                    .into_actor(act)
                                    .and_then(move |reveal_transaction, act, ctx| {
                                        // Hold reveal transaction under "waiting_for_reveal" field of data requests pool
                                        act.chain_state.data_request_pool.insert_reveal(dr_output_pointer.clone(), reveal_transaction);

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
                                    })
                            })
                    })
                    .wait(ctx)
            }
        }
    }

    fn create_tally_transactions(&mut self) -> impl Future<Item = Vec<Transaction>, Error = ()> {
        let data_request_pool = &self.chain_state.data_request_pool;

        // Include Tally transactions, one for each data request in tally stage
        let mut future_tally_transactions = vec![];
        let dr_reveals = data_request_pool.get_all_reveals();
        for (dr_pointer, reveals) in dr_reveals {
            debug!("Building tally for data request {}", dr_pointer);

            // "get_all_reveals" returns a HashMap with valid data request output pointer
            let dr_output = data_request_pool.data_request_pool[&dr_pointer]
                .data_request
                .clone();
            let (inputs, outputs, results) =
                create_vt_tally(dr_pointer.clone(), &dr_output, reveals);

            let rad_manager_addr = System::current().registry().get::<RadManager>();
            let fut = rad_manager_addr
                .send(RunConsensus {
                    script: dr_output.data_request.consensus.clone(),
                    reveals: results.clone(),
                })
                .then(|result| match result {
                    Ok(Ok(value)) => futures::future::ok(value),
                    Ok(Err(e)) => {
                        log::error!("Couldn't run consensus: {}", e);
                        futures::future::err(())
                    }
                    Err(e) => {
                        log::error!("Couldn't run consensus: {}", e);
                        futures::future::err(())
                    }
                })
                .and_then(move |consensus| {
                    let tally_body =
                        create_tally_body(&dr_output, inputs, outputs, consensus.clone());

                    sign_transaction(tally_body)
                        .map_err(|e| log::error!("Couldn't sign tally body: {}", e))
                        .and_then(move |tally_transaction| {
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
                        })
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
    own_pkh: PublicKeyHash,
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
        let utxo_diff = UtxoDiff::new(unspent_outputs_pool);
        let transaction_fee = match transaction_fee(&transaction.body, &utxo_diff) {
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
            if let TransactionType::Commit = transaction_tag(&transaction.body) {
                let dri = &transaction.body.inputs[0];
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
    let epoch = beacon.checkpoint;
    let reward = block_reward(epoch) + transaction_fees;

    // Build Mint Transaction
    transactions[0]
        .body
        .outputs
        .push(Output::ValueTransfer(ValueTransferOutput {
            pkh: own_pkh,
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
    use secp256k1::{
        PublicKey as Secp256k1_PublicKey, Secp256k1, SecretKey as Secp256k1_SecretKey,
    };
    use witnet_crypto::signature::{sign, verify};
    use witnet_data_structures::chain::*;
    use witnet_validations::validations::validate_block_signature;

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
        let block_proof = LeadershipProof::default();

        // Build empty block (because max weight is zero)
        let block = build_block(
            &transaction_pool,
            &unspent_outputs_pool,
            max_block_weight,
            block_beacon,
            block_proof,
            &[],
            PublicKeyHash::default(),
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
    fn build_signed_empty_block() {
        // Initialize transaction_pool with 1 transaction
        let mut transaction_pool = TransactionsPool::default();
        let transaction = Transaction::default();
        transaction_pool.insert(transaction.hash(), transaction.clone());

        let unspent_outputs_pool = UnspentOutputsPool::default();

        // Set `max_block_weight` to zero (no transaction should be included)
        let max_block_weight = 0;

        // Fields required to mine a block
        let block_beacon = CheckpointBeacon::default();

        // Create a KeyedSignature
        let Hash::SHA256(data) = block_beacon.hash();
        let secp = Secp256k1::new();
        let secret_key =
            Secp256k1_SecretKey::from_slice(&[0xcd; 32]).expect("32 bytes, within curve order");
        let public_key = Secp256k1_PublicKey::from_secret_key(&secp, &secret_key);
        let signature = sign(secret_key, &data);

        // Check Signature
        assert!(verify(&public_key, &data, &signature).is_ok());

        let witnet_signature: Signature = Signature::from(signature);
        let witnet_pk: PublicKey = PublicKey::from(public_key);

        let block_proof = LeadershipProof {
            block_sig: KeyedSignature {
                signature: witnet_signature,
                public_key: witnet_pk,
            },
        };

        // Build empty block (because max weight is zero)
        let block = build_block(
            &transaction_pool,
            &unspent_outputs_pool,
            max_block_weight,
            block_beacon,
            block_proof,
            &[],
            PublicKeyHash::default(),
        );

        // Check if block only contains the Mint Transaction
        assert_eq!(block.txns.len(), 1);
        assert_eq!(block.txns[0].body.inputs.len(), 0);
        assert_eq!(block.txns[0].body.outputs.len(), 1);
        assert_eq!(block.txns[0].signatures.len(), 0);

        // Check that transaction in block is not the transaction in `transactions_pool`
        assert_ne!(block.txns[0], transaction);

        // Validate block signature
        assert!(validate_block_signature(&block).is_ok());
    }

    #[test]
    #[ignore]
    fn build_block_with_transactions() {
        // Build sample transactions
        let transaction_1 = Transaction::new(
            TransactionBody::new(
                0,
                vec![Input::new(OutputPointer {
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
                    Input::new(OutputPointer {
                        transaction_id: Hash::SHA256([2; 32]),
                        output_index: 0,
                    }),
                    Input::new(OutputPointer {
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
                    Input::new(OutputPointer {
                        transaction_id: Hash::SHA256([4; 32]),
                        output_index: 0,
                    }),
                    Input::new(OutputPointer {
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
            block_sig: KeyedSignature::default(),
        };

        // Build block with
        let block = build_block(
            &transaction_pool,
            &unspent_outputs_pool,
            max_block_weight,
            block_beacon,
            block_proof,
            &[],
            PublicKeyHash::default(),
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

    #[test]
    fn test_signature_and_serialization() {
        use secp256k1::{
            PublicKey as Secp256k1_PublicKey, Secp256k1, SecretKey as Secp256k1_SecretKey,
        };
        use witnet_data_structures::serializers::decoders::TryInto;

        let secret_key = SecretKey {
            bytes: [
                106, 203, 222, 17, 245, 196, 188, 111, 78, 241, 172, 142, 124, 110, 248, 199, 64,
                127, 236, 133, 218, 0, 32, 60, 14, 113, 138, 102, 2, 247, 54, 107,
            ],
        };
        let sk: Secp256k1_SecretKey = secret_key.into();

        let secp = Secp256k1::new();
        let public_key = Secp256k1_PublicKey::from_secret_key(&secp, &sk);

        let data = [
            0xca, 0x18, 0xf5, 0xad, 0xc2, 0x18, 0x45, 0x25, 0x0e, 0x88, 0x14, 0x18, 0x1f, 0xf7,
            0x8c, 0x5b, 0x83, 0x68, 0x5c, 0x0c, 0xda, 0x55, 0x62, 0xda, 0x30, 0xc1, 0x95, 0x8d,
            0x84, 0x9e, 0xc6, 0xb9,
        ];

        let signature = sign(sk, &data);
        assert!(verify(&public_key, &data, &signature).is_ok());

        // Conversion step
        let witnet_signature = Signature::from(signature);
        let witnet_pk = PublicKey::from(public_key);

        let signature2 = witnet_signature.try_into().unwrap();
        let public_key2 = witnet_pk.try_into().unwrap();

        assert_eq!(signature, signature2);
        assert_eq!(public_key, public_key2);

        assert!(verify(&public_key2, &data, &signature2).is_ok());
    }
}
