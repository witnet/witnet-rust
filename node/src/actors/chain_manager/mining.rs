use std::convert::TryFrom;

use actix::{ActorFuture, Context, ContextFutureSpawner, Handler, SystemService, WrapFuture};
use ansi_term::Color::{White, Yellow};
use futures::future::{join_all, Future};
use log::{debug, error, info, warn};
use rayon::prelude::*;

use witnet_data_structures::{
    chain::{
        Block, BlockHeader, BlockMerkleRoots, BlockTransactions, CheckpointBeacon, EpochConstants,
        Hashable, PublicKeyHash, TransactionsPool, UnspentOutputsPool, ValueTransferOutput,
    },
    data_request::{create_tally, DataRequestPool},
    transaction::{
        CommitTransaction, CommitTransactionBody, MintTransaction, RevealTransaction,
        RevealTransactionBody, TallyTransaction, Transaction,
    },
    vrf::{BlockEligibilityClaim, DataRequestEligibilityClaim, VrfMessage},
};
use witnet_rad::types::RadonTypes;
use witnet_validations::validations::{
    block_reward, calculate_randpoe_threshold, calculate_reppoe_threshold, dr_transaction_fee,
    merkle_tree_root, update_utxo_diff, validate_block, vt_transaction_fee, UtxoDiff,
};

use crate::{
    actors::{
        chain_manager::{transaction_factory::sign_transaction, ChainManager, StateMachine},
        messages::{
            AddCandidates, AddTransaction, GetHighestCheckpointBeacon, ResolveRA, RunTally,
        },
        rad_manager::RadManager,
    },
    signature_mngr,
};

impl ChainManager {
    /// Try to mine a block
    pub fn try_mine_block(&mut self, ctx: &mut Context<Self>) {
        if !self.mining_enabled {
            debug!("Mining not enabled");
            return;
        }

        // We only want to mine in Synced state
        if self.sm_state != StateMachine::Synced {
            debug!("Not mining because node is not synced");
            return;
        }

        if self.current_epoch.is_none() {
            warn!("Cannot mine a block because current epoch is unknown");

            return;
        }
        if self.own_pkh.is_none() {
            warn!("PublicKeyHash is not set. All mined wits will be lost!");
        }

        if self.chain_state.reputation_engine.is_none() {
            warn!("Reputation engine is not set");

            return;
        }
        if self.epoch_constants.is_none() {
            warn!("EpochConstants is not set");

            return;
        }
        let epoch_constants = self.epoch_constants.unwrap();
        let total_identities = self
            .chain_state
            .reputation_engine
            .as_ref()
            .unwrap()
            .ars
            .active_identities_number() as u32;

        let current_epoch = self.current_epoch.unwrap();

        // Check eligibility
        // S(H(beacon))
        let mut beacon = match self.handle(GetHighestCheckpointBeacon, ctx) {
            Ok(b) => b,
            _ => return,
        };

        if beacon.checkpoint >= current_epoch {
            // We got a block from the future
            // Due to block consolidation from epoch N is done in epoch N+1,
            // and chain beacon is the same that the last block known.
            // Our chain beacon always come from the past epoch. So, a chain beacon
            // with the current epoch is the same error if it is come from the future
            error!(
                "The current highest checkpoint beacon is from the future ({:?} >= {:?})",
                beacon.checkpoint, current_epoch
            );
            return;
        }
        // The highest checkpoint beacon should contain the current epoch
        beacon.checkpoint = current_epoch;

        // Create a VRF proof and if eligible build block
        signature_mngr::vrf_prove(VrfMessage::block_mining(beacon))
            .map_err(|e| error!("Failed to create block eligibility proof: {}", e))
            .map(move |(vrf_proof, vrf_proof_hash)| {
                // invalid: vrf_hash > target_hash
                let target_hash = calculate_randpoe_threshold(total_identities);
                let proof_invalid = vrf_proof_hash > target_hash;

                debug!("Target hash: {}", target_hash);
                debug!("Our proof:   {}", vrf_proof_hash);
                if proof_invalid {
                    debug!("No eligibility for mining");
                    Err(())
                } else {
                    info!(
                        "{} Discovered eligibility for mining a block for epoch #{}",
                        Yellow.bold().paint("[Mining]"),
                        Yellow.bold().paint(beacon.checkpoint.to_string())
                    );
                    Ok(vrf_proof)
                }
            })
            .flatten()
            .into_actor(self)
            .and_then(|vrf_proof, act, _ctx| {
                act.create_tally_transactions()
                    .map(|tally_transactions| (vrf_proof, tally_transactions))
                    .into_actor(act)
            })
            .and_then(move |(vrf_proof, tally_transactions), act, _ctx| {
                let eligibility_claim = BlockEligibilityClaim { proof: vrf_proof };

                // Build the block using the supplied beacon and eligibility proof
                let (block_header, txns) = build_block(
                    (
                        &mut act.transactions_pool,
                        &act.chain_state.unspent_outputs_pool,
                        &act.chain_state.data_request_pool,
                    ),
                    act.max_block_weight,
                    beacon,
                    eligibility_claim,
                    &tally_transactions,
                    act.own_pkh.unwrap_or_default(),
                    epoch_constants,
                );

                // Sign the block hash
                signature_mngr::sign(&block_header)
                    .map_err(|e| error!("Couldn't sign beacon: {}", e))
                    .map(|block_sig| Block {
                        block_header,
                        block_sig,
                        txns,
                    })
                    .into_actor(act)
            })
            .and_then(move |block, act, ctx| {
                match validate_block(
                    &block,
                    current_epoch,
                    beacon,
                    &act.chain_state.unspent_outputs_pool,
                    &act.chain_state.data_request_pool,
                    // The unwrap is safe because if there is no VRF context,
                    // the actor should have stopped execution
                    act.vrf_ctx.as_mut().unwrap(),
                    act.chain_state.reputation_engine.as_ref().unwrap(),
                    act.epoch_constants.unwrap(),
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
    }

    /// Try to mine a data_request
    // TODO: refactor this procedure into multiple functions that can be tested separately.
    pub fn try_mine_data_request(&mut self, ctx: &mut Context<Self>) {
        let beacon = self
            .chain_state
            .chain_info
            .as_ref()
            .map(|x| x.highest_block_checkpoint);

        if self.current_epoch.is_none() || self.own_pkh.is_none() || beacon.is_none() {
            warn!("Cannot mine a data request because current epoch or own pkh is unknown");

            return;
        }

        let beacon = beacon.unwrap();
        let own_pkh = self.own_pkh.unwrap();
        let current_epoch = self.current_epoch.unwrap();

        // Data Request mining
        let dr_pointers = self
            .chain_state
            .data_request_pool
            .get_dr_output_pointers_by_epoch(current_epoch);

        let rep_eng = self.chain_state.reputation_engine.as_ref().unwrap();
        let my_reputation = rep_eng.trs.get(&own_pkh);
        let total_active_reputation = rep_eng.trs.get_sum(rep_eng.ars.active_identities());
        let num_active_identities = rep_eng.ars.active_identities_number() as u32;
        debug!("{} data requests for this epoch", dr_pointers.len());
        debug!(
            "Reputation: {}, total: {}, active identities: {}",
            my_reputation.0, total_active_reputation.0, num_active_identities,
        );

        for (dr_pointer, data_request_output) in dr_pointers.into_iter().filter_map(|dr_pointer| {
            // Filter data requests that are not in data_request_pool
            self.chain_state
                .data_request_pool
                .get_dr_output(&dr_pointer)
                .map(|data_request_output| (dr_pointer, data_request_output))
        }) {
            let num_witnesses =
                data_request_output.witnesses + data_request_output.backup_witnesses;
            // The beacon used to create and verify data requests must be set to the current epoch
            let dr_beacon = CheckpointBeacon {
                checkpoint: current_epoch,
                ..beacon
            };
            signature_mngr::vrf_prove(VrfMessage::data_request(dr_beacon, dr_pointer))
                .map_err(move |e| {
                    error!(
                        "Couldn't create VRF proof for data request {}: {}",
                        dr_pointer, e
                    )
                })
                .map(move |(vrf_proof, vrf_proof_hash)| {
                    // invalid: vrf_hash > target_hash
                    let target_hash = calculate_reppoe_threshold(
                        my_reputation,
                        total_active_reputation,
                        num_witnesses,
                        num_active_identities,
                    );
                    let proof_invalid = vrf_proof_hash > target_hash;

                    debug!("{} witnesses", num_witnesses);
                    debug!("[DR] Target hash: {}", target_hash);
                    debug!("[DR] Our proof:   {}", vrf_proof_hash);
                    if proof_invalid {
                        debug!("No eligibility for data request {}", dr_pointer);
                        Err(())
                    } else {
                        info!(
                            "{} Discovered eligibility for mining a data request {} for epoch #{}",
                            Yellow.bold().paint("[Mining]"),
                            Yellow.bold().paint(dr_pointer.to_string()),
                            Yellow.bold().paint(current_epoch.to_string())
                        );
                        Ok(vrf_proof)
                    }
                })
                .flatten()
                .and_then(move |vrf_proof| {
                    let rad_request = data_request_output.data_request.clone();

                    // Send ResolveRA message to RADManager
                    let rad_manager_addr = RadManager::from_registry();
                    rad_manager_addr
                        .send(ResolveRA { rad_request })
                        .map(|result| match result {
                            Ok(value) => Ok((vrf_proof, value)),
                            Err(e) => {
                                log::error!("Couldn't resolve rad request: {}", e);
                                Err(())
                            }
                        })
                        .map_err(|e| log::error!("Couldn't resolve rad request: {}", e))
                })
                .flatten()
                .and_then(move |(vrf_proof, reveal_value)| {
                    let vrf_proof_dr = DataRequestEligibilityClaim { proof: vrf_proof };

                    Vec::<u8>::try_from(&reveal_value)
                        .map(|reveal_bytes| (reveal_bytes, vrf_proof_dr))
                        .map_err(|e| log::error!("Couldn't decode tally value from bytes: {}", e))
                })
                .and_then(move |(reveal_bytes, vrf_proof_dr)| {
                    let reveal_body = RevealTransactionBody::new(dr_pointer, reveal_bytes, own_pkh);

                    sign_transaction(&reveal_body, 1)
                        .map_err(|e| log::error!("Couldn't sign reveal body: {}", e))
                        .and_then(move |reveal_signatures| {
                            // Commitment is the hash of the RevealTransaction signature
                            // that will be published later
                            let commitment = reveal_signatures[0].signature.hash();
                            let commit_body =
                                CommitTransactionBody::new(dr_pointer, commitment, vrf_proof_dr);

                            sign_transaction(&commit_body, 1)
                                .map(|commit_signatures| {
                                    let commit_transaction = Transaction::Commit(
                                        CommitTransaction::new(commit_body, commit_signatures),
                                    );
                                    let reveal_transaction =
                                        RevealTransaction::new(reveal_body, reveal_signatures);
                                    (commit_transaction, reveal_transaction)
                                })
                                .map_err(|e| log::error!("Couldn't sign commit body: {}", e))
                        })
                })
                .into_actor(self)
                .and_then(move |(commit_transaction, reveal_transaction), act, ctx| {
                    // Hold reveal transaction under "waiting_for_reveal" field of data requests pool
                    act.chain_state
                        .data_request_pool
                        .insert_reveal(dr_pointer, reveal_transaction);

                    // Send AddTransaction message to self
                    // And broadcast it to all of peers
                    if let Err(e) = act.handle(
                        AddTransaction {
                            transaction: commit_transaction,
                        },
                        ctx,
                    ) {
                        log::warn!("Failed to add commit transaction: {}", e);
                    }

                    actix::fut::ok(())
                })
                .wait(ctx);
        }
    }

    fn create_tally_transactions(
        &mut self,
    ) -> impl Future<Item = Vec<TallyTransaction>, Error = ()> {
        let data_request_pool = &self.chain_state.data_request_pool;

        let dr_reveals = data_request_pool
            .get_all_reveals()
            .into_iter()
            .map(|(dr_pointer, reveals)| {
                (
                    dr_pointer,
                    reveals,
                    // "get_all_reveals" returns a HashMap with valid data request output pointer
                    data_request_pool.data_request_pool[&dr_pointer].clone(),
                )
            })
            .collect::<Vec<_>>();

        let future_tally_transactions =
            dr_reveals
                .into_iter()
                .map(|(dr_pointer, reveals, dr_state)| {
                    debug!("Building tally for data request {}", dr_pointer);

                    // Ignore reveals that could not be decoded.
                    //  Those reveal that cannot be decoded are most likely malformed and therefore
                    //  their authors are punished in the same way as non-revealers.
                    let results: Vec<RadonTypes> = reveals.clone()
                        .into_par_iter()
                        .filter_map(|reveal_tx| {
                            let slice = reveal_tx.body.reveal.as_slice();
                            match RadonTypes::try_from(slice) {
                                Ok(result) => Some(result),
                                Err(e) =>  {
                                    log::warn!("Could not decode reveal from {} (revealed bytes were `{:?}`): {:?}", reveal_tx.body.pkh, &slice, e);
                                    None
                                },
                            }
                        })
                        .collect();

                    let rad_manager_addr = RadManager::from_registry();
                    rad_manager_addr
                        .send(RunTally {
                            script: dr_state.data_request.data_request.tally.clone(),
                            reveals: results.clone(),
                        })
                        .then(|result| match result {
                            // If the result of `RunTally` is `Ok`, it will be published as tally
                            Ok(Ok(value)) => futures::future::ok(value),
                            // If the result of `RunTally` is `Err`, we ignore this data request.
                            // If a data request has an invalid tally script, it will never resolve.
                            // The Radon engine should return `Ok(RadonError)` for errors which should
                            // be published as the result of the data request, or we could use a
                            // special radon value to indicate "generic error"
                            Ok(Err(e)) => {
                                log::warn!("Couldn't run tally: {}", e);
                                futures::future::err(())
                            }
                            Err(e) => {
                                log::error!("Couldn't run tally: {}", e);
                                futures::future::err(())
                            }
                        })
                        .and_then(move |tally_result| {
                            create_tally(
                                dr_pointer,
                                &dr_state.data_request,
                                dr_state.pkh,
                                &tally_result,
                                reveals,
                            )
                            .map(|tally| (tally, tally_result))
                            .map_err(|e| log::error!("Could not create tally transaction: {}", e))
                        })
                        .and_then(move |(tally, tally_result)| {
                            info!(
                                "{} Created Tally for Data Request {} with result: {}\n{}",
                                Yellow.bold().paint("[Data Request]"),
                                Yellow.bold().paint(&dr_pointer.to_string()),
                                Yellow.bold().paint(format!("{:?}", &tally_result.result)),
                                White
                                    .bold()
                                    .paint(results.into_iter().map(|result| result).fold(
                                        String::from("Reveals:"),
                                        |acc, item| format!("{}\n\t* {}", acc, item)
                                    )),
                            );

                            futures::future::ok(tally)
                        })
                        // This future should always return Ok because join_all short-circuits on the
                        // first Err, and we want to keep creating tallies after the first error
                        // Map Result<T, E> to Result<Option<T>, ()>
                        .then(|x| futures::future::ok(x.ok()))
                });

        join_all(future_tally_transactions)
            // Map Option<Vec<T>> to Vec<T>, this returns all the non-error results
            .map(|x| x.into_iter().flatten().collect())
    }
}

/// Build a new Block using the supplied leadership proof and by filling transactions from the
/// `transaction_pool`
/// Returns an unsigned block!
fn build_block(
    pools_ref: (&mut TransactionsPool, &UnspentOutputsPool, &DataRequestPool),
    max_block_weight: u32,
    beacon: CheckpointBeacon,
    proof: BlockEligibilityClaim,
    tally_transactions: &[TallyTransaction],
    own_pkh: PublicKeyHash,
    epoch_constants: EpochConstants,
) -> (BlockHeader, BlockTransactions) {
    let (transactions_pool, unspent_outputs_pool, dr_pool) = pools_ref;
    let mut utxo_diff = UtxoDiff::new(unspent_outputs_pool);

    let epoch = beacon.checkpoint;

    // Get all the unspent transactions and calculate the sum of their fees
    let mut transaction_fees = 0;
    let mut block_weight = 0;
    let mut value_transfer_txns = Vec::new();
    let mut data_request_txns = Vec::new();
    let mut tally_txns = Vec::new();

    // Currently only value transfer transactions weight is taking into account
    for vt_tx in transactions_pool.vt_iter() {
        // Currently, 1 weight unit is equivalent to 1 byte
        let transaction_weight = vt_tx.size();
        let transaction_fee = match vt_transaction_fee(&vt_tx, &utxo_diff, epoch, epoch_constants) {
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
            value_transfer_txns.push(vt_tx.clone());

            update_utxo_diff(
                &mut utxo_diff,
                vt_tx.body.inputs.iter().collect(),
                vt_tx.body.outputs.iter().collect(),
                vt_tx.hash(),
            );
            transaction_fees += transaction_fee;
            block_weight += transaction_weight;
        }

        if new_block_weight == max_block_weight {
            break;
        }
    }

    for dr_tx in transactions_pool.dr_iter() {
        let transaction_fee = match dr_transaction_fee(&dr_tx, &utxo_diff, epoch, epoch_constants) {
            Ok(x) => x,
            Err(e) => {
                warn!(
                    "Error when calculating transaction fee for transaction: {}",
                    e
                );
                continue;
            }
        };

        update_utxo_diff(
            &mut utxo_diff,
            dr_tx.body.inputs.iter().collect(),
            dr_tx.body.outputs.iter().collect(),
            dr_tx.hash(),
        );
        data_request_txns.push(dr_tx.clone());
        transaction_fees += transaction_fee;
    }

    for ta_tx in tally_transactions {
        if let Some(dr_output) = dr_pool.get_dr_output(&ta_tx.dr_pointer) {
            tally_txns.push(ta_tx.clone());
            transaction_fees += dr_output.tally_fee;
        } else {
            warn!("Data Request pointed by tally transaction doesn't exist in DataRequestPool");
        }
    }

    let (commit_txns, commits_fees) = transactions_pool.remove_commits(dr_pool);
    transaction_fees += commits_fees;

    let (reveal_txns, reveals_fees) = transactions_pool.remove_reveals(dr_pool);
    transaction_fees += reveals_fees;

    // Include Mint Transaction by miner
    let reward = block_reward(epoch) + transaction_fees;

    // Build Mint Transaction
    let mint = MintTransaction::new(
        epoch,
        ValueTransferOutput {
            pkh: own_pkh,
            value: reward,
            time_lock: 0,
        },
    );

    // Compute `hash_merkle_root` and build block header
    let vt_hash_merkle_root = merkle_tree_root(&value_transfer_txns);
    let dr_hash_merkle_root = merkle_tree_root(&data_request_txns);
    let commit_hash_merkle_root = merkle_tree_root(&commit_txns);
    let reveal_hash_merkle_root = merkle_tree_root(&reveal_txns);
    let tally_hash_merkle_root = merkle_tree_root(&tally_txns);
    let merkle_roots = BlockMerkleRoots {
        mint_hash: mint.hash(),
        vt_hash_merkle_root,
        dr_hash_merkle_root,
        commit_hash_merkle_root,
        reveal_hash_merkle_root,
        tally_hash_merkle_root,
    };

    let block_header = BlockHeader {
        version: 0,
        beacon,
        merkle_roots,
        proof,
    };

    let txns = BlockTransactions {
        mint,
        value_transfer_txns,
        data_request_txns,
        commit_txns,
        reveal_txns,
        tally_txns,
    };

    (block_header, txns)
}

#[cfg(test)]
mod tests {
    use std::convert::TryInto;

    use secp256k1::{
        PublicKey as Secp256k1_PublicKey, Secp256k1, SecretKey as Secp256k1_SecretKey,
    };

    use witnet_crypto::signature::{sign, verify};
    use witnet_data_structures::{chain::*, transaction::*, vrf::VrfCtx};
    use witnet_protected::Protected;
    use witnet_validations::validations::validate_block_signature;

    use super::*;

    #[test]
    fn build_empty_block() {
        // Initialize transaction_pool with 1 transaction
        let mut transaction_pool = TransactionsPool::default();
        // In protocol buffers, when version is 0 and all the other fields are empty vectors, the
        // transaction size is 0 bytes (since missing fields are initialized with the default
        // values). Therefore version cannot be 0.
        let transaction = Transaction::ValueTransfer(VTTransaction::default());
        transaction_pool.insert(transaction.clone());

        let unspent_outputs_pool = UnspentOutputsPool::default();
        let dr_pool = DataRequestPool::default();

        // Set `max_block_weight` to zero (no transaction should be included)
        let max_block_weight = 0;

        // Fields required to mine a block
        let block_beacon = CheckpointBeacon::default();
        let block_proof = BlockEligibilityClaim::default();

        // Build empty block (because max weight is zero)
        let (block_header, txns) = build_block(
            (&mut transaction_pool, &unspent_outputs_pool, &dr_pool),
            max_block_weight,
            block_beacon,
            block_proof,
            &[],
            PublicKeyHash::default(),
            EpochConstants::default(),
        );
        let block = Block {
            block_header,
            block_sig: KeyedSignature::default(),
            txns,
        };

        // Check if block only contains the Mint Transaction
        assert_eq!(block.txns.value_transfer_txns.len(), 0);
        assert_eq!(block.txns.data_request_txns.len(), 0);
        assert_eq!(block.txns.commit_txns.len(), 0);
        assert_eq!(block.txns.reveal_txns.len(), 0);
        assert_eq!(block.txns.tally_txns.len(), 0);

        // Check that transaction in block is not the transaction in `transactions_pool`
        assert_ne!(Transaction::Mint(block.txns.mint), transaction);
    }

    #[test]
    fn build_signed_empty_block() {
        // Initialize transaction_pool with 1 transaction
        let mut transaction_pool = TransactionsPool::default();
        let transaction = Transaction::ValueTransfer(VTTransaction::default());
        transaction_pool.insert(transaction.clone());

        let unspent_outputs_pool = UnspentOutputsPool::default();
        let dr_pool = DataRequestPool::default();

        // Set `max_block_weight` to zero (no transaction should be included)
        let max_block_weight = 0;

        // Fields required to mine a block
        let block_beacon = CheckpointBeacon::default();

        // Add valid vrf proof
        let vrf = &mut VrfCtx::secp256k1().unwrap();
        let secret_key = SecretKey {
            bytes: Protected::from(vec![0xcd; 32]),
        };
        let block_proof = BlockEligibilityClaim::create(vrf, &secret_key, block_beacon).unwrap();

        // Build empty block (because max weight is zero)

        let (block_header, txns) = build_block(
            (&mut transaction_pool, &unspent_outputs_pool, &dr_pool),
            max_block_weight,
            block_beacon,
            block_proof,
            &[],
            PublicKeyHash::default(),
            EpochConstants::default(),
        );

        // Create a KeyedSignature
        let Hash::SHA256(data) = block_header.hash();
        let secp = Secp256k1::new();
        let secret_key =
            Secp256k1_SecretKey::from_slice(&[0xcd; 32]).expect("32 bytes, within curve order");
        let public_key = Secp256k1_PublicKey::from_secret_key(&secp, &secret_key);
        let signature = sign(secret_key, &data);
        let witnet_pk = PublicKey::from(public_key);
        let witnet_signature = Signature::from(signature);

        let block = Block {
            block_header,
            block_sig: KeyedSignature {
                signature: witnet_signature,
                public_key: witnet_pk,
            },
            txns,
        };

        // Check Signature
        assert!(verify(&public_key, &data, &signature).is_ok());

        // Check if block only contains the Mint Transaction
        assert_eq!(block.txns.mint.len(), 1);
        assert_eq!(block.txns.value_transfer_txns.len(), 0);
        assert_eq!(block.txns.data_request_txns.len(), 0);
        assert_eq!(block.txns.commit_txns.len(), 0);
        assert_eq!(block.txns.reveal_txns.len(), 0);
        assert_eq!(block.txns.tally_txns.len(), 0);

        // Validate block signature
        assert!(validate_block_signature(&block).is_ok());
    }

    #[test]
    #[ignore]
    fn build_block_with_transactions() {
        // Build sample transactions
        let vt_tx1 = VTTransaction::new(
            VTTransactionBody::new(
                vec![Input::new(OutputPointer {
                    transaction_id: Hash::SHA256([1; 32]),
                    output_index: 0,
                })],
                vec![ValueTransferOutput {
                    time_lock: 0,
                    pkh: PublicKeyHash::default(),
                    value: 1,
                }],
            ),
            vec![],
        );

        let vt_tx2 = VTTransaction::new(
            VTTransactionBody::new(
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
                    ValueTransferOutput {
                        time_lock: 0,
                        pkh: PublicKeyHash::default(),
                        value: 2,
                    },
                    ValueTransferOutput {
                        time_lock: 0,
                        pkh: PublicKeyHash::default(),
                        value: 3,
                    },
                ],
            ),
            vec![],
        );
        let vt_tx3 = VTTransaction::new(
            VTTransactionBody::new(
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
                    ValueTransferOutput {
                        time_lock: 0,
                        pkh: PublicKeyHash::default(),
                        value: 4,
                    },
                    ValueTransferOutput {
                        time_lock: 0,
                        pkh: PublicKeyHash::default(),
                        value: 5,
                    },
                ],
            ),
            vec![],
        );

        let transaction_1 = Transaction::ValueTransfer(vt_tx1.clone());
        let transaction_2 = Transaction::ValueTransfer(vt_tx2);
        let transaction_3 = Transaction::ValueTransfer(vt_tx3);

        // Insert transactions into `transactions_pool`
        // TODO: Currently the insert function does not take into account the fees to compute the transaction's weight
        let mut transaction_pool = TransactionsPool::default();
        transaction_pool.insert(transaction_1.clone());
        transaction_pool.insert(transaction_2.clone());
        transaction_pool.insert(transaction_3.clone());

        let unspent_outputs_pool = UnspentOutputsPool::default();
        let dr_pool = DataRequestPool::default();

        // Set `max_block_weight` to fit only `transaction_1` size
        let max_block_weight = transaction_1.size();

        // Fields required to mine a block
        let block_beacon = CheckpointBeacon::default();
        let block_proof = BlockEligibilityClaim::default();

        // Build block with

        let (block_header, txns) = build_block(
            (&mut transaction_pool, &unspent_outputs_pool, &dr_pool),
            max_block_weight,
            block_beacon,
            block_proof,
            &[],
            PublicKeyHash::default(),
            EpochConstants::default(),
        );
        let block = Block {
            block_header,
            block_sig: KeyedSignature::default(),
            txns,
        };

        // Check if block contains only 2 transactions (Mint Transaction + 1 included transaction)
        assert_eq!(block.txns.len(), 2);

        // Check that exist Mint Transaction
        assert_eq!(block.txns.mint.is_empty(), false);

        // Check that the included transaction is the only one that fits the `max_block_weight`
        assert_eq!(block.txns.value_transfer_txns[0], vt_tx1);
    }

    #[test]
    fn test_signature_and_serialization() {
        let secret_key = SecretKey {
            bytes: Protected::from(vec![
                106, 203, 222, 17, 245, 196, 188, 111, 78, 241, 172, 142, 124, 110, 248, 199, 64,
                127, 236, 133, 218, 0, 32, 60, 14, 113, 138, 102, 2, 247, 54, 107,
            ]),
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
