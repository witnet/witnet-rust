use actix::{
    ActorFuture, AsyncContext, Context, ContextFutureSpawner, Handler, SystemService, WrapFuture,
};
use ansi_term::Color::{White, Yellow};
use futures::future::{join_all, Future};
use itertools::Itertools;
use std::{
    cmp::Ordering,
    collections::HashSet,
    convert::TryFrom,
    sync::{
        atomic::{self, AtomicU16},
        Arc,
    },
};

use witnet_data_structures::{
    chain::{
        Block, BlockHeader, BlockMerkleRoots, BlockTransactions, Bn256PublicKey, CheckpointBeacon,
        CheckpointVRF, EpochConstants, Hash, Hashable, PublicKeyHash, ReputationEngine, SuperBlock,
        TransactionsPool, UnspentOutputsPool,
    },
    data_request::{calculate_witness_reward, create_tally, DataRequestPool},
    error::TransactionError,
    radon_report::{RadonReport, ReportContext},
    transaction::{
        CommitTransaction, CommitTransactionBody, MintTransaction, RevealTransaction,
        RevealTransactionBody, TallyTransaction,
    },
    vrf::{BlockEligibilityClaim, DataRequestEligibilityClaim, VrfMessage},
};
use witnet_rad::{error::RadError, types::serial_iter_decode};
use witnet_util::timestamp::get_timestamp;
use witnet_validations::validations::{
    block_reward, calculate_randpoe_threshold, calculate_reppoe_threshold, dr_transaction_fee,
    hash_merkle_tree_root, merkle_tree_root, update_utxo_diff, vt_transaction_fee, UtxoDiff,
};

use crate::{
    actors::{
        chain_manager::{
            transaction_factory::{build_commit_collateral, sign_transaction},
            ChainManager, StateMachine,
        },
        inventory_manager::InventoryManager,
        messages::{
            AddCandidates, AddCommitReveal, GetBlocksEpochRange, GetItemBlock, ResolveRA, RunTally,
        },
        rad_manager::RadManager,
    },
    signature_mngr,
};

impl ChainManager {
    /// Try to mine a block
    pub fn try_mine_block(&mut self, ctx: &mut Context<Self>) {
        if !self.mining_enabled {
            log::debug!("Mining disabled in configuration");
            return;
        }

        // We only want to mine in Live state
        if self.sm_state != StateMachine::Live {
            log::debug!("Not mining because node is not in Live State");
            return;
        }

        if self.current_epoch.is_none() {
            log::warn!("Cannot mine a block because current epoch is unknown");

            return;
        }
        if self.own_pkh.is_none() {
            log::warn!("PublicKeyHash is not set. All mined wits will be lost!");
        }

        if self.chain_state.reputation_engine.is_none() {
            log::warn!("Reputation engine is not set");

            return;
        }
        if self.epoch_constants.is_none() {
            log::warn!("EpochConstants is not set");

            return;
        }
        if self.chain_state.chain_info.is_none() {
            log::warn!("ChainInfo is not set");

            return;
        }
        let epoch_constants = self.epoch_constants.unwrap();
        let rep_engine = self.chain_state.reputation_engine.as_ref().unwrap().clone();
        let total_identities = u32::try_from(rep_engine.ars().active_identities_number()).unwrap();

        let current_epoch = self.current_epoch.unwrap();

        let chain_info = self.chain_state.chain_info.as_ref().unwrap();
        let genesis_hash = chain_info.consensus_constants.genesis_hash;
        let max_block_weight = chain_info.consensus_constants.max_block_weight;

        let mining_bf = chain_info.consensus_constants.mining_backup_factor;

        let mining_rf = chain_info.consensus_constants.mining_replication_factor;

        let collateral_minimum = chain_info.consensus_constants.collateral_minimum;

        let mut beacon = chain_info.highest_block_checkpoint;
        let mut vrf_input = chain_info.highest_vrf_output;

        if beacon.checkpoint >= current_epoch {
            // We got a block from the future
            // Due to block consolidation from epoch N is done in epoch N+1,
            // and chain beacon is the same that the last block known.
            // Our chain beacon always come from the past epoch. So, a chain beacon
            // with the current epoch is the same error if it is come from the future
            log::error!(
                "The current highest checkpoint beacon is from the future ({:?} >= {:?})",
                beacon.checkpoint,
                current_epoch
            );
            return;
        }
        // The highest checkpoint beacon should contain the current epoch
        beacon.checkpoint = current_epoch;
        vrf_input.checkpoint = current_epoch;

        let own_pkh = self.own_pkh.unwrap_or_default();
        let is_ars_member = rep_engine.is_ars_member(&own_pkh);

        let superblock_period = u32::from(chain_info.consensus_constants.superblock_period);
        // FIXME: Only ARS members from the last block of the previous SuperBlock can create a SuperBlock
        if rep_engine.is_ars_member(&own_pkh) && current_epoch % superblock_period == 0 {
            // FIXME(#1236): ARS Members have to include the BLS signature instead of PublicKeyHash
            // FIXME: After Reputation Merkelitation, only the ARS Members from the previous consolidated
            // block will be added, to avoid include addresses that could be wrong later by
            // block reorganization
            let ars_members: Vec<PublicKeyHash> = rep_engine
                .ars()
                .active_identities()
                .cloned()
                .sorted()
                .collect();

            self.superblock_creating_and_broadcasting(
                ctx,
                current_epoch,
                superblock_period,
                ars_members,
                genesis_hash,
            );
        }

        // Create a VRF proof and if eligible build block
        signature_mngr::vrf_prove(VrfMessage::block_mining(vrf_input))
            .map_err(|e| log::error!("Failed to create block eligibility proof: {}", e))
            .map(move |(vrf_proof, vrf_proof_hash)| {
                // invalid: vrf_hash > target_hash
                let (target_hash, probability) =
                    calculate_randpoe_threshold(total_identities, mining_bf);
                let proof_invalid = vrf_proof_hash > target_hash;

                log::info!(
                    "Probability to create a valid mining proof: {:.6}%",
                    probability * 100_f64
                );
                log::trace!("Target hash: {}", target_hash);
                log::trace!("Our proof:   {}", vrf_proof_hash);
                if proof_invalid {
                    log::debug!("No eligibility for mining a block");
                    Err(())
                } else {
                    log::info!(
                        "{} Discovered eligibility for mining a block for epoch #{}",
                        Yellow.bold().paint("[Mining]"),
                        Yellow.bold().paint(beacon.checkpoint.to_string())
                    );
                    let mining_prob =
                        calculate_mining_probability(&rep_engine, own_pkh, mining_rf, mining_bf);
                    // Discount the already reached probability
                    let mining_prob = mining_prob / probability * 100.0;
                    log::info!(
                        "Probability that the mined block will be selected: {:.6}%",
                        mining_prob
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

                // If pkh is in ARS, no need to send bn256 public key
                let bn256_public_key = if is_ars_member {
                    None
                } else {
                    act.bn256_public_key.clone()
                };

                // Build the block using the supplied beacon and eligibility proof
                let (block_header, txns) = build_block(
                    (
                        &mut act.transactions_pool,
                        &act.chain_state.unspent_outputs_pool,
                        &act.chain_state.data_request_pool,
                    ),
                    max_block_weight,
                    beacon,
                    eligibility_claim,
                    &tally_transactions,
                    own_pkh,
                    epoch_constants,
                    act.chain_state.block_number(),
                    collateral_minimum,
                    bn256_public_key,
                    act.external_address,
                    act.external_percentage,
                );

                // Sign the block hash
                signature_mngr::sign(&block_header)
                    .map_err(|e| log::error!("Couldn't sign beacon: {}", e))
                    .map(|block_sig| Block {
                        block_header,
                        block_sig,
                        txns,
                    })
                    .into_actor(act)
            })
            .and_then(move |block, act, _ctx| {
                act.future_process_validations(
                    block.clone(),
                    current_epoch,
                    vrf_input,
                    beacon,
                    epoch_constants,
                    mining_bf,
                )
                .map(|_diff, act, ctx| {
                    // Send AddCandidates message to self
                    // This will run all the validations again

                    let block_hash = block.hash();
                    act.chain_state.node_stats.last_block_proposed = block_hash;
                    act.chain_state.node_stats.block_proposed_count += 1;
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
                })
                .map_err(|e, _, _| log::error!("Error trying to mine a block: {}", e))
            })
            .wait(ctx);
    }

    /// Try to mine a data_request
    // TODO: refactor this procedure into multiple functions that can be tested separately.
    pub fn try_mine_data_request(&mut self, ctx: &mut Context<Self>) {
        let vrf_input = self
            .chain_state
            .chain_info
            .as_ref()
            .map(|x| x.highest_vrf_output);

        if self.current_epoch.is_none() || self.own_pkh.is_none() || vrf_input.is_none() {
            log::warn!("Cannot mine a data request because current epoch or own pkh is unknown");

            return;
        }

        let vrf_input = vrf_input.unwrap();
        let own_pkh = self.own_pkh.unwrap();
        let current_epoch = self.current_epoch.unwrap();
        let data_request_timeout = self.data_request_timeout;
        let timestamp = u64::try_from(get_timestamp()).unwrap();

        // Data Request mining
        let dr_pointers = self
            .chain_state
            .data_request_pool
            .get_dr_output_pointers_by_epoch(current_epoch);

        let rep_eng = self.chain_state.reputation_engine.as_ref().unwrap();
        let is_ars_member = rep_eng.is_ars_member(&own_pkh);

        let my_reputation = rep_eng.trs().get(&own_pkh).0 + 1;
        let total_active_reputation = rep_eng.total_active_reputation();
        let num_active_identities =
            u32::try_from(rep_eng.ars().active_identities_number()).unwrap();
        log::debug!("{} data requests for this epoch", dr_pointers.len());
        log::debug!(
            "Reputation: {}, total: {}, active identities: {}",
            my_reputation,
            total_active_reputation,
            num_active_identities,
        );

        // `current_retrieval_count` keeps track of how many sources are being retrieved in this
        // epoch by using a reference-counted atomic counter that can be read and updated safely.
        let current_retrieval_count = Arc::new(AtomicU16::new(0u16));
        let maximum_retrieval_count = self.data_request_max_retrievals_per_epoch;

        for (dr_pointer, data_request_output) in dr_pointers.into_iter().filter_map(|dr_pointer| {
            // Filter data requests that are not in data_request_pool
            self.chain_state
                .data_request_pool
                .get_dr_output(&dr_pointer)
                .map(|data_request_output| (dr_pointer, data_request_output))
        }) {
            let num_witnesses = data_request_output.witnesses;
            let num_backup_witnesses = data_request_output.backup_witnesses;
            // The vrf_input used to create and verify data requests must be set to the current epoch
            let dr_vrf_input = CheckpointVRF {
                checkpoint: current_epoch,
                ..vrf_input
            };

            let (target_hash, probability) =
                calculate_reppoe_threshold(rep_eng, &own_pkh, num_witnesses + num_backup_witnesses);

            // Grab a reference to `current_retrieval_count`
            let cloned_retrieval_count = Arc::clone(&current_retrieval_count);
            let cloned_retrieval_count2 = Arc::clone(&current_retrieval_count);
            let added_retrieval_count =
                u16::try_from(data_request_output.data_request.retrieve.len())
                    .unwrap_or(core::u16::MAX);

            let collateral_amount = if data_request_output.collateral == 0 {
                self.chain_state
                    .chain_info
                    .as_ref()
                    .unwrap()
                    .consensus_constants
                    .collateral_minimum
            } else {
                data_request_output.collateral
            };

            signature_mngr::vrf_prove(VrfMessage::data_request(dr_vrf_input, dr_pointer))
                .map_err(move |e| {
                    log::error!(
                        "Couldn't create VRF proof for data request {}: {}",
                        dr_pointer,
                        e
                    )
                })
                .map(move |(vrf_proof, vrf_proof_hash)| {
                    // invalid: vrf_hash > target_hash
                    let proof_invalid = vrf_proof_hash > target_hash;

                    log::debug!(
                        "{} witnesses and {} backup witnesses",
                        num_witnesses,
                        num_backup_witnesses
                    );
                    log::debug!(
                        "Probability to be eligible for this data request: {:.6}%",
                        probability * 100.0
                    );
                    log::trace!("[DR] Target hash: {}", target_hash);
                    log::trace!("[DR] Our proof:   {}", vrf_proof_hash);
                    if proof_invalid {
                        log::debug!("No eligibility for data request {}", dr_pointer);
                        Err(())
                    } else {
                        log::info!(
                            "{} Discovered eligibility for mining a data request {} for epoch #{}",
                            Yellow.bold().paint("[Mining]"),
                            Yellow.bold().paint(dr_pointer.to_string()),
                            Yellow.bold().paint(current_epoch.to_string())
                        );
                        Ok(vrf_proof)
                    }
                })
                .flatten()
                .into_actor(self)
                // Refrain from trying to resolve any more requests if we have already hit the limit
                // of retrievals per epoch.
                .and_then(move |vrf_proof, act, _| {
                    let mut start_retrieval_count = cloned_retrieval_count.load(atomic::Ordering::Relaxed);
                    let mut final_retrieval_count = start_retrieval_count.saturating_add(added_retrieval_count);

                    act.chain_state.node_stats.dr_eligibility_count += 1;

                    if final_retrieval_count > maximum_retrieval_count {
                        log::info!("{} Refrained from resolving data request {} for epoch #{} because it contains {} \
                        sources, which added to the sources that have already been retrieved ({}) would total {} \
                        retrievals, which exceed current limit per epoch ({}). This limit exists for performance and \
                        security reasons. You can increase the limit (AT YOUR OWN RISK) by adjusting the \
                        `data_request_max_retrievals_per_epoch` inside the `[mining]` section in the `witnet.toml` \
                        configuration file.",
                            Yellow.bold().paint("[Mining]"),
                            Yellow.bold().paint(dr_pointer.to_string()),
                            Yellow.bold().paint(current_epoch.to_string()),
                            Yellow.bold().paint(added_retrieval_count.to_string()),
                            Yellow.bold().paint(start_retrieval_count.to_string()),
                            Yellow.bold().paint(final_retrieval_count.to_string()),
                            Yellow.bold().paint(maximum_retrieval_count.to_string())
                        );

                        actix::fut::err(())

                    } else {
                        // Update `current_retrieval_count` thanks to interior mutability of the
                        // `cloned_retrieval_count` reference. This is a recursive operation so as
                        // to guarantee addition and prevent potential race conditions in a concurrent
                        // scenario.
                        loop {
                            let internal_retrieval_count = cloned_retrieval_count.compare_and_swap(start_retrieval_count, final_retrieval_count, atomic::Ordering::Relaxed);
                            if internal_retrieval_count == start_retrieval_count {
                                // The counter update was updated successfully, we can move on.
                                break actix::fut::ok(vrf_proof);
                            } else {
                                // The counter was updated somewhere else, addition must be retried
                                // after verifying that the limit has not been exceeded since last
                                // it was checked.
                                start_retrieval_count = internal_retrieval_count;
                                final_retrieval_count = start_retrieval_count.saturating_add(added_retrieval_count);

                                if final_retrieval_count > maximum_retrieval_count {
                                    break actix::fut::err(());
                                }
                            }
                        }
                    }
                })
                // Collect outputs to be used as input for collateralized commitment,
                // as well as outputs for change.
                .and_then(move |vrf_proof, act, _| {
                    let (collateral_age, checkpoint_period) = match &act.chain_state.chain_info {
                        Some(x) => (x.consensus_constants.collateral_age, x.consensus_constants.checkpoints_period),
                        None => {
                            log::error!("ChainInfo is None");
                            return actix::fut::err(());
                        },
                    };

                    let block_number_limit = act.chain_state.block_number().saturating_sub(collateral_age);
                    // Check if we have enough collateralizable unspent outputs before starting
                    // retrieval
                    match build_commit_collateral(
                        collateral_amount,
                        &mut act.chain_state.own_utxos,
                        own_pkh,
                        &act.chain_state.unspent_outputs_pool,
                        timestamp,
                        // The timeout included when using collateral is only one epoch to ensure
                        // that if your commit has not been accepted you can use your utxo in
                        // the next epoch or at least in two epochs
                        u64::from(checkpoint_period),
                        // The block number must be lower than this limit
                        block_number_limit
                    ) {
                        Ok(collateral) => actix::fut::ok((vrf_proof, collateral)),
                        Err(TransactionError::NoMoney {
                                available_balance, transaction_value, ..
                        }) => {
                            let required_collateral = transaction_value;
                            log::warn!("Not enough mature UTXOs for collateral for data request {}: Available balance: {}, Required collateral: {}",
                                Yellow.bold().paint(dr_pointer.to_string()),
                                available_balance,
                                required_collateral,
                            );
                            // Decrease the retrieval limit hoping that some other, cheaper,
                            // data request can be resolved instead
                            cloned_retrieval_count2.fetch_sub(added_retrieval_count, atomic::Ordering::Relaxed);
                            actix::fut::err(())
                        }
                        Err(e) => {
                            log::error!("Unexpected error when trying to select UTXOs to be used for collateral in data request {}: {}", dr_pointer, e);
                            actix::fut::err(())
                        }
                    }
                })
                .and_then(move |(vrf_proof, collateral), act, _| {
                    let rad_request = data_request_output.data_request.clone();

                    // Send ResolveRA message to RADManager
                    let rad_manager_addr = RadManager::from_registry();
                    rad_manager_addr
                        .send(ResolveRA {
                            rad_request,
                            timeout: data_request_timeout,
                        })
                        .map(move |result| match result {
                            Ok(value) => Ok((vrf_proof, collateral, value)),
                            Err(e) => {
                                log::error!("Couldn't resolve rad request {}: {}", dr_pointer, e);
                                Err(())
                            }
                        })
                        .map_err(move |e| {
                            log::error!("Couldn't resolve rad request {}: {}", dr_pointer, e)
                        })
                        .into_actor(act)
                })
                .then(|res, _, _| {
                    // This is .flatten()
                    match res {
                        Ok(Ok(x)) => actix::fut::ok(x),
                        Ok(Err(())) => actix::fut::err(()),
                        Err(()) => actix::fut::err(()),
                    }
                })
                .and_then(move |(vrf_proof, collateral, reveal_value), _, _| {
                    let vrf_proof_dr = DataRequestEligibilityClaim { proof: vrf_proof };

                    match Vec::<u8>::try_from(&reveal_value) {
                        Ok(reveal_bytes) => actix::fut::ok((reveal_bytes, vrf_proof_dr, collateral)),
                        Err(e) => {
                            log::error!("Couldn't decode tally value from bytes: {}", e);
                            actix::fut::err(())
                        },
                    }
                })
                .and_then(move |(reveal_bytes, vrf_proof_dr, collateral), act, _| {
                    let reveal_body = RevealTransactionBody::new(dr_pointer, reveal_bytes, own_pkh);

                    // If pkh is in ARS, no need to send bn256 public key
                    let bn256_public_key = if is_ars_member {
                        None
                    } else {
                        act.bn256_public_key.clone()
                    };

                    sign_transaction(&reveal_body, 1)
                        .map_err(|e| log::error!("Couldn't sign reveal body: {}", e))
                        .and_then(move |reveal_signatures| {
                            // Commitment is the hash of the RevealTransaction signature
                            // that will be published later
                            let commitment = reveal_signatures[0].signature.hash();
                            let (inputs, outputs) = collateral;
                            let commit_body =
                                CommitTransactionBody::new(dr_pointer, commitment, vrf_proof_dr, inputs, outputs, bn256_public_key);

                            sign_transaction(&commit_body, 1)
                                .map(|commit_signatures| {
                                    let commit_transaction =
                                        CommitTransaction::new(commit_body, commit_signatures);
                                    let reveal_transaction =
                                        RevealTransaction::new(reveal_body, reveal_signatures);
                                    (commit_transaction, reveal_transaction)
                                })
                                .map_err(|e| log::error!("Couldn't sign commit body: {}", e))
                        })
                        .into_actor(act)
                })
                .and_then(move |(commit_transaction, reveal_transaction), act, ctx| {
                    ctx.notify(AddCommitReveal {
                        commit_transaction,
                        reveal_transaction,
                    });

                    act.chain_state.node_stats.commits_proposed_count += 1;

                    actix::fut::ok(())
                })
                .spawn(ctx);
        }
    }

    /// Create a superblock and broadcast it
    fn superblock_creating_and_broadcasting(
        &mut self,
        ctx: &mut Context<Self>,
        current_epoch: u32,
        superblock_period: u32,
        ars_members: Vec<PublicKeyHash>,
        genesis_hash: Hash,
    ) {
        let superblock_index = current_epoch / superblock_period;

        let inventory_manager = InventoryManager::from_registry();

        let init_epoch = current_epoch - superblock_period;
        let init_epoch = init_epoch.saturating_sub(1);
        let final_epoch = current_epoch.saturating_sub(2);

        futures::future::ok(self.handle(
            GetBlocksEpochRange::new_with_limit(init_epoch..=final_epoch, 0),
            ctx,
        ))
        .and_then(move |res| match res {
            Ok(v) => {
                let block_hashes: Vec<Hash> = v.into_iter().map(|(_epoch, hash)| hash).collect();
                futures::future::ok(block_hashes)
            }
            Err(e) => {
                log::error!("Error in GetBlocksEpochRange: {}", e);
                futures::future::err(())
            }
        })
        .and_then(move |block_hashes| {
            let aux = block_hashes.into_iter().map(move |hash| {
                inventory_manager
                    .send(GetItemBlock { hash })
                    .then(move |res| match res {
                        Ok(Ok(block)) => futures::future::ok(block.block_header),
                        Ok(Err(e)) => {
                            log::error!("Error in GetItemBlock: {}", e);
                            futures::future::err(())
                        }
                        Err(e) => {
                            log::error!("Error in GetItemBlock: {}", e);
                            futures::future::err(())
                        }
                    })
                    .then(|x| futures::future::ok(x.ok()))
            });

            join_all(aux)
                // Map Option<Vec<T>> to Vec<T>, this returns all the non-error results
                .map(|x| x.into_iter().flatten().collect::<Vec<BlockHeader>>())
        })
        .into_actor(self)
        .and_then(move |block_headers, act, ctx| {
            let last_hash = act
                .handle(
                    GetBlocksEpochRange::new_with_limit_from_end(..init_epoch, 1),
                    ctx,
                )
                .map(move |v| {
                    v.first()
                        .map(|(_epoch, hash)| *hash)
                        .unwrap_or(genesis_hash)
                });
            match last_hash {
                Ok(last_hash) => actix::fut::ok((block_headers, last_hash)),
                Err(e) => {
                    log::error!("Error in GetBlocksEpochRange: {}", e);
                    actix::fut::err(())
                }
            }
        })
        .and_then(move |(block_headers, last_hash), _act, _ctx| {
            let superblock =
                build_superblock(&block_headers, &ars_members, superblock_index, last_hash);

            match superblock {
                Some(superblock) => {
                    let superblock_hash = superblock.hash();

                    // FIXME(#1236): Superblock signing and broadcasting (and remove these logs)
                    log::error!("SUPERBLOCK: {:?}", superblock);
                    log::error!("SUPERBLOCK hash: {}", superblock_hash);
                }
                None => log::warn!("No blocks to build a superblocks"),
            }

            actix::fut::ok(())
        })
        .map_err(|e, _, _| log::error!("Superblock forwarding process fail: {:?}", e))
        .wait(ctx)
    }

    fn create_tally_transactions(
        &mut self,
    ) -> impl Future<Item = Vec<TallyTransaction>, Error = ()> {
        let data_request_pool = &self.chain_state.data_request_pool;
        let collateral_minimum = self
            .chain_state
            .chain_info
            .as_ref()
            .unwrap()
            .consensus_constants
            .collateral_minimum;

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
                .map(move |(dr_pointer, reveals, dr_state)| {
                    log::debug!("Building tally for data request {}", dr_pointer);

                    // Use the serial decoder to decode all the reveals in a lossy way, i.e. will
                    // ignore reveals that cannot be decoded. At this point, reveals that cannot be
                    // decoded are most likely malformed and therefore their authors shall be
                    // punished in the same way as non-revealers.
                    // TODO: leverage `rayon` so as to make this a parallel iterator.
                    let reports = serial_iter_decode(
                        &mut reveals
                            .iter()
                            .map(|reveal_tx| (reveal_tx.body.reveal.as_slice(), reveal_tx)),
                        |e: RadError, slice: &[u8], reveal_tx: &RevealTransaction| {
                            log::warn!(
                            "Could not decode reveal from {:?} (revealed bytes were `{:?}`): {:?}",
                            reveal_tx,
                            &slice,
                            e
                        );
                            Some(RadonReport::from_result(
                                Err(RadError::MalformedReveal),
                                &ReportContext::default(),
                            ))
                        },
                    );

                    let min_consensus_ratio =
                        f64::from(dr_state.data_request.min_consensus_percentage) / 100.0;

                    let committers: HashSet<PublicKeyHash> =
                        dr_state.info.commits.keys().cloned().collect();
                    let commits_count = committers.len();

                    let rad_manager_addr = RadManager::from_registry();
                    rad_manager_addr
                        .send(RunTally {
                            min_consensus_ratio,
                            reports: reports.clone(),
                            script: dr_state.data_request.data_request.tally.clone(),
                            commits_count,
                        })
                        .then(|result| match result {
                            // The result of `RunTally` will be published as tally
                            Ok(value) => futures::future::ok(value),
                            // Mailbox error
                            Err(e) => {
                                log::error!("Couldn't run tally: {}", e);
                                futures::future::err(())
                            }
                        })
                        .and_then(move |tally_result| {
                            let tally = create_tally(
                                dr_pointer,
                                &dr_state.data_request,
                                dr_state.pkh,
                                &tally_result,
                                reveals.iter().map(|r| r.body.pkh).collect(),
                                committers,
                                collateral_minimum,
                            );

                            match tally {
                                Ok(t) => {
                                    log::info!(
                                        "{} Created Tally for Data Request {} with result: {}\n{}",
                                        Yellow.bold().paint("[Data Request]"),
                                        Yellow.bold().paint(&dr_pointer.to_string()),
                                        Yellow
                                            .bold()
                                            .paint(format!("{}", &tally_result.into_inner())),
                                        White.bold().paint(
                                            reports.into_iter().map(|result| result).fold(
                                                String::from("Reveals:"),
                                                |acc, item| format!(
                                                    "{}\n\t* {}",
                                                    acc,
                                                    item.into_inner()
                                                )
                                            )
                                        ),
                                    );

                                    futures::future::ok(t)
                                }
                                Err(e) => {
                                    log::error!("Couldn't create tally: {}", e);
                                    futures::future::err(())
                                }
                            }
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
#[allow(clippy::too_many_arguments)]
fn build_block(
    pools_ref: (&mut TransactionsPool, &UnspentOutputsPool, &DataRequestPool),
    max_block_weight: u32,
    beacon: CheckpointBeacon,
    proof: BlockEligibilityClaim,
    tally_transactions: &[TallyTransaction],
    own_pkh: PublicKeyHash,
    epoch_constants: EpochConstants,
    block_number: u32,
    collateral_minimum: u64,
    bn256_public_key: Option<Bn256PublicKey>,
    external_address: Option<PublicKeyHash>,
    external_percentage: u8,
) -> (BlockHeader, BlockTransactions) {
    let (transactions_pool, unspent_outputs_pool, dr_pool) = pools_ref;
    let epoch = beacon.checkpoint;
    let mut utxo_diff = UtxoDiff::new(unspent_outputs_pool, block_number);

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
                log::warn!(
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
                log::warn!(
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
        if let Some(dr_state) = dr_pool.data_request_state(&ta_tx.dr_pointer) {
            tally_txns.push(ta_tx.clone());
            let commits_count = dr_state.info.commits.len();
            let reveals_count = dr_state.info.reveals.len();
            let honests_count = commits_count - ta_tx.slashed_witnesses.len();
            // Remainder collateral goes to the miner
            let (_, extra_tally_fee) = calculate_witness_reward(
                commits_count,
                reveals_count,
                honests_count,
                &dr_state.data_request,
                collateral_minimum,
            );
            transaction_fees += dr_state.data_request.tally_fee + extra_tally_fee;
        } else {
            log::warn!(
                "Data Request pointed by tally transaction doesn't exist in DataRequestPool"
            );
        }
    }

    let (commit_txns, commits_fees) = transactions_pool.remove_commits(dr_pool);
    transaction_fees += commits_fees;

    let (reveal_txns, reveals_fees) = transactions_pool.remove_reveals(dr_pool);
    transaction_fees += reveals_fees;

    // Include Mint Transaction by miner
    let reward = block_reward(epoch) + transaction_fees;
    let mint = MintTransaction::with_external_address(
        epoch,
        reward,
        own_pkh,
        external_address,
        external_percentage,
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
        bn256_public_key,
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

/// Produces a `SuperBlock` that includes the blocks in `block_headers` if there is at least one of them.
fn build_superblock(
    block_headers: &[BlockHeader],
    sorted_ars_identities: &[PublicKeyHash],
    index: u32,
    last_block_in_previous_superblock: Hash,
) -> Option<SuperBlock> {
    let last_block = block_headers.last()?.hash();
    let merkle_drs: Vec<Hash> = block_headers
        .iter()
        .map(|b| b.merkle_roots.dr_hash_merkle_root)
        .collect();
    let merkle_tallies: Vec<Hash> = block_headers
        .iter()
        .map(|b| b.merkle_roots.tally_hash_merkle_root)
        .collect();

    let pkh_hashes: Vec<Hash> = sorted_ars_identities.iter().map(|pkh| pkh.hash()).collect();

    Some(SuperBlock {
        data_request_root: hash_merkle_tree_root(&merkle_drs),
        tally_root: hash_merkle_tree_root(&merkle_tallies),
        ars_root: hash_merkle_tree_root(&pkh_hashes),
        index,
        last_block,
        last_block_in_previous_superblock,
    })
}

#[allow(clippy::many_single_char_names)]
fn internal_calculate_mining_probability(
    rf: u32,
    n: f64,
    k: u32, // k: iterative rf until reach bf
    m: i32, // M: nodes with reputation greater than me
    l: i32, // L: nodes with reputation equal than me
    r: i32, // R: nodes with reputation less than me
) -> f64 {
    if k == rf {
        let rf = f64::from(rf);
        // Prob to mine is the probability that a node with the same reputation than me mine,
        // divided by all the nodes with the same reputation:
        // 1/L * (1 - ((N-RF)/N)^L)
        let prob_to_mine = (1.0 / f64::from(l)) * (1.0 - ((n - rf) / n).powi(l));
        // Prob that a node with more reputation than me mine is:
        // ((N-RF)/N)^M
        let prob_greater_neg = ((n - rf) / n).powi(m);

        prob_to_mine * prob_greater_neg
    } else {
        let k = f64::from(k);
        // Here we take into account that rf = 1 because is only a new slot
        let prob_to_mine = (1.0 / f64::from(l)) * (1.0 - ((n - 1.0) / n).powi(l));
        // The same equation than before
        let prob_bigger_neg = ((n - k) / n).powi(m);
        // Prob that a node with less or equal reputation than me mine with a lower slot is:
        // ((N+1-RF)/N)^(L+R-1)
        let prob_lower_slot_neg = ((n + 1.0 - k) / n).powi(l + r - 1);

        prob_to_mine * prob_bigger_neg * prob_lower_slot_neg
    }
}

fn calculate_mining_probability(
    rep_engine: &ReputationEngine,
    own_pkh: PublicKeyHash,
    rf: u32,
    bf: u32,
) -> f64 {
    let n = u32::try_from(rep_engine.ars().active_identities_number()).unwrap();

    // In case of any active node, the probability is maximum
    if n == 0 {
        return 1.0;
    }

    // First we need to know how many nodes have more or equal reputation than us
    let own_rep = rep_engine.trs().get(&own_pkh);
    let is_active_node = rep_engine.ars().contains(&own_pkh);
    let mut greater = 0;
    let mut equal = 0;
    let mut less = 0;
    for &active_id in rep_engine.ars().active_identities() {
        match rep_engine.trs().get(&active_id).cmp(&own_rep) {
            Ordering::Greater => greater += 1,
            Ordering::Equal => equal += 1,
            Ordering::Less => less += 1,
        }
    }
    // In case of not being active, the equal value is plus 1.
    if !is_active_node {
        equal += 1;
    }

    if rf > n && greater == 0 {
        // In case of replication factor exceed the active node number and being the most reputed
        // we obtain the maximum probability divided in the nodes we share the same reputation
        1.0 / f64::from(equal)
    } else if rf > n && greater > 0 {
        // In case of replication factor exceed the active node number and not being the most reputed
        // we obtain the minimum probability
        0.0
    } else {
        let mut aux =
            internal_calculate_mining_probability(rf, f64::from(n), rf, greater, equal, less);
        let mut k = rf + 1;
        while k <= bf && k <= n {
            aux += internal_calculate_mining_probability(rf, f64::from(n), k, greater, equal, less);
            k += 1;
        }
        aux
    }
}

#[cfg(test)]
mod tests {
    use std::convert::{TryFrom, TryInto};

    use secp256k1::{
        PublicKey as Secp256k1_PublicKey, Secp256k1, SecretKey as Secp256k1_SecretKey,
    };

    use witnet_crypto::signature::{sign, verify};
    use witnet_data_structures::{chain::*, transaction::*, vrf::VrfCtx};
    use witnet_protected::Protected;
    use witnet_validations::validations::validate_block_signature;

    use super::*;
    use crate::actors::chain_manager::verify_signatures;

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
        let block_number = 1;
        let collateral_minimum = 1_000_000_000;

        // Build empty block (because max weight is zero)
        let (block_header, txns) = build_block(
            (&mut transaction_pool, &unspent_outputs_pool, &dr_pool),
            max_block_weight,
            block_beacon,
            block_proof,
            &[],
            PublicKeyHash::default(),
            EpochConstants::default(),
            block_number,
            collateral_minimum,
            None,
            None,
            0,
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

    static LAST_VRF_INPUT: &str =
        "4da71b67e7e50ae4ad06a71e505244f8b490da55fc58c50386c908f7146d2239";
    #[test]
    fn build_signed_empty_block() {
        // Initialize transaction_pool with 1 transaction
        let mut transaction_pool = TransactionsPool::default();
        let transaction = Transaction::ValueTransfer(VTTransaction::default());
        transaction_pool.insert(transaction);

        let unspent_outputs_pool = UnspentOutputsPool::default();
        let dr_pool = DataRequestPool::default();

        // Set `max_block_weight` to zero (no transaction should be included)
        let max_block_weight = 0;

        // Fields required to mine a block
        let block_beacon = CheckpointBeacon::default();

        let mut vrf_input = CheckpointVRF::default();
        vrf_input.hash_prev_vrf = LAST_VRF_INPUT.parse().unwrap();

        // Add valid vrf proof
        let vrf = &mut VrfCtx::secp256k1().unwrap();
        let secret_key = SecretKey {
            bytes: Protected::from(vec![0xcd; 32]),
        };
        let block_proof = BlockEligibilityClaim::create(vrf, &secret_key, vrf_input).unwrap();
        let block_number = 1;
        let collateral_minimum = 1_000_000_000;

        // Build empty block (because max weight is zero)

        let (block_header, txns) = build_block(
            (&mut transaction_pool, &unspent_outputs_pool, &dr_pool),
            max_block_weight,
            block_beacon,
            block_proof,
            &[],
            PublicKeyHash::default(),
            EpochConstants::default(),
            block_number,
            collateral_minimum,
            None,
            None,
            0,
        );

        // Create a KeyedSignature
        let Hash::SHA256(data) = block_header.hash();
        let secp = &Secp256k1::new();
        let secret_key =
            Secp256k1_SecretKey::from_slice(&[0xcd; 32]).expect("32 bytes, within curve order");
        let public_key = Secp256k1_PublicKey::from_secret_key(secp, &secret_key);
        let signature = sign(secp, secret_key, &data).unwrap();
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
        assert!(verify(secp, &public_key, &data, &signature).is_ok());

        // Check if block only contains the Mint Transaction
        assert_eq!(block.txns.mint.len(), 1);
        assert_eq!(block.txns.value_transfer_txns.len(), 0);
        assert_eq!(block.txns.data_request_txns.len(), 0);
        assert_eq!(block.txns.commit_txns.len(), 0);
        assert_eq!(block.txns.reveal_txns.len(), 0);
        assert_eq!(block.txns.tally_txns.len(), 0);

        // Validate block signature
        let mut signatures_to_verify = vec![];
        assert!(validate_block_signature(&block, &mut signatures_to_verify).is_ok());
        assert!(verify_signatures(signatures_to_verify, vrf, secp).is_ok());
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

        // Set `max_block_weight` to fit only `transaction_1` size
        let max_block_weight = transaction_1.size();

        // Insert transactions into `transactions_pool`
        // TODO: Currently the insert function does not take into account the fees to compute the transaction's weight
        let mut transaction_pool = TransactionsPool::default();
        transaction_pool.insert(transaction_1);
        transaction_pool.insert(transaction_2);
        transaction_pool.insert(transaction_3);

        let unspent_outputs_pool = UnspentOutputsPool::default();
        let dr_pool = DataRequestPool::default();

        // Fields required to mine a block
        let block_beacon = CheckpointBeacon::default();
        let block_proof = BlockEligibilityClaim::default();
        let block_number = 1;
        let collateral_minimum = 1_000_000_000;

        // Build block with

        let (block_header, txns) = build_block(
            (&mut transaction_pool, &unspent_outputs_pool, &dr_pool),
            max_block_weight,
            block_beacon,
            block_proof,
            &[],
            PublicKeyHash::default(),
            EpochConstants::default(),
            block_number,
            collateral_minimum,
            None,
            None,
            0,
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

        let secp = &Secp256k1::new();
        let public_key = Secp256k1_PublicKey::from_secret_key(secp, &sk);

        let data = [
            0xca, 0x18, 0xf5, 0xad, 0xc2, 0x18, 0x45, 0x25, 0x0e, 0x88, 0x14, 0x18, 0x1f, 0xf7,
            0x8c, 0x5b, 0x83, 0x68, 0x5c, 0x0c, 0xda, 0x55, 0x62, 0xda, 0x30, 0xc1, 0x95, 0x8d,
            0x84, 0x9e, 0xc6, 0xb9,
        ];

        let signature = sign(secp, sk, &data).unwrap();
        assert!(verify(secp, &public_key, &data, &signature).is_ok());

        // Conversion step
        let witnet_signature = Signature::from(signature);
        let witnet_pk = PublicKey::from(public_key);

        let signature2 = witnet_signature.try_into().unwrap();
        let public_key2 = witnet_pk.try_into().unwrap();

        assert_eq!(signature, signature2);
        assert_eq!(public_key, public_key2);

        assert!(verify(secp, &public_key2, &data, &signature2).is_ok());
    }

    #[test]
    fn test_superblock_creation_no_blocks() {
        let default_hash = Hash::default();
        let superblock = build_superblock(&[], &[], 0, default_hash);
        assert_eq!(superblock, None);
    }

    static DR_MERKLE_ROOT_1: &str =
        "0000000000000000000000000000000000000000000000000000000000000000";
    static TALLY_MERKLE_ROOT_1: &str =
        "1111111111111111111111111111111111111111111111111111111111111111";
    static DR_MERKLE_ROOT_2: &str =
        "ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff";
    static TALLY_MERKLE_ROOT_2: &str =
        "dddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddd";
    #[test]
    fn test_superblock_creation_one_block() {
        let default_hash = Hash::default();
        let default_proof = BlockEligibilityClaim::default();
        let default_beacon = CheckpointBeacon::default();
        let dr_merkle_root_1 = DR_MERKLE_ROOT_1.parse().unwrap();
        let tally_merkle_root_1 = TALLY_MERKLE_ROOT_1.parse().unwrap();

        let block = BlockHeader {
            version: 1,
            beacon: default_beacon,
            merkle_roots: BlockMerkleRoots {
                mint_hash: default_hash,
                vt_hash_merkle_root: default_hash,
                dr_hash_merkle_root: dr_merkle_root_1,
                commit_hash_merkle_root: default_hash,
                reveal_hash_merkle_root: default_hash,
                tally_hash_merkle_root: tally_merkle_root_1,
            },
            proof: default_proof,
            bn256_public_key: None,
        };

        let expected_superblock = SuperBlock {
            data_request_root: dr_merkle_root_1,
            tally_root: tally_merkle_root_1,
            ars_root: PublicKeyHash::default().hash(),
            index: 0,
            last_block: block.hash(),
            last_block_in_previous_superblock: default_hash,
        };

        let superblock =
            build_superblock(&[block], &[PublicKeyHash::default()], 0, default_hash).unwrap();
        assert_eq!(superblock, expected_superblock);
    }

    #[test]
    fn test_superblock_creation_two_blocks() {
        let default_hash = Hash::default();
        let default_proof = BlockEligibilityClaim::default();
        let default_beacon = CheckpointBeacon::default();
        let dr_merkle_root_1 = DR_MERKLE_ROOT_1.parse().unwrap();
        let tally_merkle_root_1 = TALLY_MERKLE_ROOT_1.parse().unwrap();
        let dr_merkle_root_2 = DR_MERKLE_ROOT_2.parse().unwrap();
        let tally_merkle_root_2 = TALLY_MERKLE_ROOT_2.parse().unwrap();
        // Sha256(dr_merkle_root_1 || dr_merkle_root_2)
        let expected_superblock_dr_root =
            "bba91ca85dc914b2ec3efb9e16e7267bf9193b14350d20fba8a8b406730ae30a"
                .parse()
                .unwrap();
        // Sha256(tally_merkle_root_1 || tally_merkle_root_2)
        let expected_superblock_tally_root =
            "83a70a79e9bef7bd811df52736eb61373095d7a8936aed05d0dc96d959b30b50"
                .parse()
                .unwrap();

        let block_1 = BlockHeader {
            version: 1,
            beacon: default_beacon,
            merkle_roots: BlockMerkleRoots {
                mint_hash: default_hash,
                vt_hash_merkle_root: default_hash,
                dr_hash_merkle_root: dr_merkle_root_1,
                commit_hash_merkle_root: default_hash,
                reveal_hash_merkle_root: default_hash,
                tally_hash_merkle_root: tally_merkle_root_1,
            },
            proof: default_proof.clone(),
            bn256_public_key: None,
        };

        let block_2 = BlockHeader {
            version: 1,
            beacon: default_beacon,
            merkle_roots: BlockMerkleRoots {
                mint_hash: default_hash,
                vt_hash_merkle_root: default_hash,
                dr_hash_merkle_root: dr_merkle_root_2,
                commit_hash_merkle_root: default_hash,
                reveal_hash_merkle_root: default_hash,
                tally_hash_merkle_root: tally_merkle_root_2,
            },
            proof: default_proof,
            bn256_public_key: None,
        };

        let expected_superblock = SuperBlock {
            data_request_root: expected_superblock_dr_root,
            tally_root: expected_superblock_tally_root,
            ars_root: PublicKeyHash::default().hash(),
            index: 0,
            last_block: block_2.hash(),
            last_block_in_previous_superblock: default_hash,
        };

        let superblock = build_superblock(
            &[block_1, block_2],
            &[PublicKeyHash::default()],
            0,
            default_hash,
        )
        .unwrap();
        assert_eq!(superblock, expected_superblock);
    }

    fn init_rep_engine(v_rep: Vec<u32>) -> (ReputationEngine, Vec<PublicKeyHash>) {
        let mut rep_engine = ReputationEngine::new(1000);

        let mut ids = vec![];
        for (i, &rep) in v_rep.iter().enumerate() {
            let pkh = PublicKeyHash::from_bytes(&[u8::try_from(i).unwrap(); 20]).unwrap();
            rep_engine
                .trs_mut()
                .gain(Alpha(10), vec![(pkh, Reputation(rep))])
                .unwrap();
            ids.push(pkh);
        }
        rep_engine.ars_mut().push_activity(ids.clone());

        (rep_engine, ids)
    }

    fn calculate_mining_probs(v_rep: Vec<u32>, rf: u32, bf: u32) -> (Vec<f64>, f64) {
        let v_rep_len = v_rep.len();
        let (rep_engine, ids) = init_rep_engine(v_rep);
        let n = rep_engine.ars().active_identities_number();
        assert_eq!(n, v_rep_len);
        assert_eq!(ids.len(), v_rep_len);

        let mut probs = vec![];
        for id in ids {
            probs.push(calculate_mining_probability(&rep_engine, id, rf, bf))
        }

        let new_pkh = PublicKeyHash::from_bytes(&[0xFF as u8; 20]).unwrap();
        let new_prob = calculate_mining_probability(&rep_engine, new_pkh, rf, bf);

        (probs, new_prob)
    }

    // FIXME: Allow for now, wait for https://github.com/rust-lang/rust/issues/67058 to reach stable
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    #[test]
    fn calculate_mining_probabilities_rf1_bf1() {
        let v_rep = vec![10, 8, 8, 8, 5, 5, 5, 5, 2, 2];
        let (probs, new_prob) = calculate_mining_probs(v_rep, 1, 1);

        assert_eq!(
            (probs[0] * 10_000.0).round() as u32,
            (10.0 as f64 * 100.0).round() as u32
        );
        assert_eq!(
            (probs[1] * 10_000.0).round() as u32,
            (8.13 as f64 * 100.0).round() as u32
        );
        assert_eq!(
            (probs[2] * 10_000.0).round() as u32,
            (8.13 as f64 * 100.0).round() as u32
        );
        assert_eq!(
            (probs[3] * 10_000.0).round() as u32,
            (8.13 as f64 * 100.0).round() as u32
        );
        assert_eq!(
            (probs[4] * 10_000.0).round() as u32,
            (5.64 as f64 * 100.0).round() as u32
        );
        assert_eq!(
            (probs[5] * 10_000.0).round() as u32,
            (5.64 as f64 * 100.0).round() as u32
        );
        assert_eq!(
            (probs[6] * 10_000.0).round() as u32,
            (5.64 as f64 * 100.0).round() as u32
        );
        assert_eq!(
            (probs[7] * 10_000.0).round() as u32,
            (5.64 as f64 * 100.0).round() as u32
        );
        assert_eq!(
            (probs[8] * 10_000.0).round() as u32,
            (4.09 as f64 * 100.0).round() as u32
        );
        assert_eq!(
            (probs[9] * 10_000.0).round() as u32,
            (4.09 as f64 * 100.0).round() as u32
        );

        assert_eq!(
            (new_prob * 10_000.0).round() as u32,
            (3.49 as f64 * 100.0).round() as u32
        );
    }

    // FIXME: Allow for now, wait for https://github.com/rust-lang/rust/issues/67058 to reach stable
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    #[test]
    fn calculate_mining_probabilities_rf1_bf2() {
        let v_rep = vec![10, 8, 8, 8, 5, 5, 5, 5, 2, 2];
        let (probs, new_prob) = calculate_mining_probs(v_rep, 1, 2);

        assert_eq!(
            (probs[0] * 10_000.0).round() as u32,
            (13.87 as f64 * 100.0).round() as u32
        );
        assert_eq!(
            (probs[1] * 10_000.0).round() as u32,
            (11.24 as f64 * 100.0).round() as u32
        );
        assert_eq!(
            (probs[2] * 10_000.0).round() as u32,
            (11.24 as f64 * 100.0).round() as u32
        );
        assert_eq!(
            (probs[3] * 10_000.0).round() as u32,
            (11.24 as f64 * 100.0).round() as u32
        );
        assert_eq!(
            (probs[4] * 10_000.0).round() as u32,
            (7.72 as f64 * 100.0).round() as u32
        );
        assert_eq!(
            (probs[5] * 10_000.0).round() as u32,
            (7.72 as f64 * 100.0).round() as u32
        );
        assert_eq!(
            (probs[6] * 10_000.0).round() as u32,
            (7.72 as f64 * 100.0).round() as u32
        );
        assert_eq!(
            (probs[7] * 10_000.0).round() as u32,
            (7.72 as f64 * 100.0).round() as u32
        );
        assert_eq!(
            (probs[8] * 10_000.0).round() as u32,
            (5.52 as f64 * 100.0).round() as u32
        );
        assert_eq!(
            (probs[9] * 10_000.0).round() as u32,
            (5.52 as f64 * 100.0).round() as u32
        );

        assert_eq!(
            (new_prob * 10_000.0).round() as u32,
            (4.56 as f64 * 100.0).round() as u32
        );
    }

    // FIXME: Allow for now, wait for https://github.com/rust-lang/rust/issues/67058 to reach stable
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    #[test]
    fn calculate_mining_probabilities_rf2_bf2() {
        let v_rep = vec![10, 8, 8, 8, 5, 5, 5, 5, 2, 2];
        let (probs, new_prob) = calculate_mining_probs(v_rep, 2, 2);

        assert_eq!(
            (probs[0] * 10_000.0).round() as u32,
            (20.0 as f64 * 100.0).round() as u32
        );
        assert_eq!(
            (probs[1] * 10_000.0).round() as u32,
            (13.01 as f64 * 100.0).round() as u32
        );
        assert_eq!(
            (probs[2] * 10_000.0).round() as u32,
            (13.01 as f64 * 100.0).round() as u32
        );
        assert_eq!(
            (probs[3] * 10_000.0).round() as u32,
            (13.01 as f64 * 100.0).round() as u32
        );
        assert_eq!(
            (probs[4] * 10_000.0).round() as u32,
            (6.05 as f64 * 100.0).round() as u32
        );
        assert_eq!(
            (probs[5] * 10_000.0).round() as u32,
            (6.05 as f64 * 100.0).round() as u32
        );
        assert_eq!(
            (probs[6] * 10_000.0).round() as u32,
            (6.05 as f64 * 100.0).round() as u32
        );
        assert_eq!(
            (probs[7] * 10_000.0).round() as u32,
            (6.05 as f64 * 100.0).round() as u32
        );
        assert_eq!(
            (probs[8] * 10_000.0).round() as u32,
            (3.02 as f64 * 100.0).round() as u32
        );
        assert_eq!(
            (probs[9] * 10_000.0).round() as u32,
            (3.02 as f64 * 100.0).round() as u32
        );

        assert_eq!(
            (new_prob * 10_000.0).round() as u32,
            (2.15 as f64 * 100.0).round() as u32
        );
    }

    // FIXME: Allow for now, wait for https://github.com/rust-lang/rust/issues/67058 to reach stable
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    #[test]
    fn calculate_mining_probabilities_rf4_bf8() {
        let v_rep = vec![10, 8, 8, 8, 5, 5, 5, 5, 2, 2];
        let (probs, new_prob) = calculate_mining_probs(v_rep, 4, 8);

        assert_eq!(
            (probs[0] * 10_000.0).round() as u32,
            (40.12 as f64 * 100.0).round() as u32
        );
        assert_eq!(
            (probs[1] * 10_000.0).round() as u32,
            (15.77 as f64 * 100.0).round() as u32
        );
        assert_eq!(
            (probs[2] * 10_000.0).round() as u32,
            (15.77 as f64 * 100.0).round() as u32
        );
        assert_eq!(
            (probs[3] * 10_000.0).round() as u32,
            (15.77 as f64 * 100.0).round() as u32
        );
        assert_eq!(
            (probs[4] * 10_000.0).round() as u32,
            (2.87 as f64 * 100.0).round() as u32
        );
        assert_eq!(
            (probs[5] * 10_000.0).round() as u32,
            (2.87 as f64 * 100.0).round() as u32
        );
        assert_eq!(
            (probs[6] * 10_000.0).round() as u32,
            (2.87 as f64 * 100.0).round() as u32
        );
        assert_eq!(
            (probs[7] * 10_000.0).round() as u32,
            (2.87 as f64 * 100.0).round() as u32
        );
        assert_eq!(
            (probs[8] * 10_000.0).round() as u32,
            (0.56 as f64 * 100.0).round() as u32
        );
        assert_eq!(
            (probs[9] * 10_000.0).round() as u32,
            (0.56 as f64 * 100.0).round() as u32
        );

        assert_eq!(
            (new_prob * 10_000.0).round() as u32,
            (0.25 as f64 * 100.0).round() as u32
        );
    }

    // FIXME: Allow for now, wait for https://github.com/rust-lang/rust/issues/67058 to reach stable
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    #[test]
    fn calculate_mining_probabilities_rf1_bf1_10() {
        let v_rep = vec![10; 10];
        let (probs, new_prob) = calculate_mining_probs(v_rep, 1, 1);

        assert_eq!(
            (probs[0] * 10_000.0).round() as u32,
            (6.51 as f64 * 100.0).round() as u32
        );

        assert_eq!(
            (new_prob * 10_000.0).round() as u32,
            (3.49 as f64 * 100.0).round() as u32
        );
    }

    // FIXME: Allow for now, wait for https://github.com/rust-lang/rust/issues/67058 to reach stable
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    #[test]
    fn calculate_mining_probabilities_rf1_bf1_100() {
        let v_rep = vec![10; 100];
        let (probs, new_prob) = calculate_mining_probs(v_rep, 1, 1);

        assert_eq!(
            (probs[0] * 10_000.0).round() as u32,
            (0.63 as f64 * 100.0).round() as u32
        );

        assert_eq!(
            (new_prob * 10_000.0).round() as u32,
            (0.37 as f64 * 100.0).round() as u32
        );
    }

    // FIXME: Allow for now, wait for https://github.com/rust-lang/rust/issues/67058 to reach stable
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    #[test]
    fn calculate_mining_probabilities_rf4_bf8_100() {
        let v_rep = vec![10; 100];
        let (probs, new_prob) = calculate_mining_probs(v_rep, 4, 8);

        assert_eq!(
            (probs[0] * 10_000.0).round() as u32,
            (1.0 as f64 * 100.0).round() as u32
        );

        assert_eq!(
            (new_prob * 10_000.0).round() as u32,
            (0.08 as f64 * 100.0).round() as u32
        );
    }

    // FIXME: Allow for now, wait for https://github.com/rust-lang/rust/issues/67058 to reach stable
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    #[test]
    fn calculate_mining_probabilities_rf4_bf8_100_diff() {
        let mut v_rep = vec![10; 25];
        v_rep.extend(vec![8; 25]);
        v_rep.extend(vec![6; 25]);
        v_rep.extend(vec![4; 25]);
        let (probs, new_prob) = calculate_mining_probs(v_rep, 4, 8);

        assert_eq!(
            (probs[0] * 10_000.0).round() as u32,
            (2.58 as f64 * 100.0).round() as u32
        );
        assert_eq!(
            (probs[25] * 10_000.0).round() as u32,
            (0.94 as f64 * 100.0).round() as u32
        );
        assert_eq!(
            (probs[50] * 10_000.0).round() as u32,
            (0.35 as f64 * 100.0).round() as u32
        );
        assert_eq!(
            (probs[75] * 10_000.0).round() as u32,
            (0.13 as f64 * 100.0).round() as u32
        );

        assert_eq!(
            (new_prob * 10_000.0).round() as u32,
            (0.08 as f64 * 100.0).round() as u32
        );
    }

    // FIXME: Allow for now, wait for https://github.com/rust-lang/rust/issues/67058 to reach stable
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    #[test]
    fn calculate_mining_probabilities_rf_high() {
        let v_rep = vec![10, 8, 8, 2];
        let (probs, new_prob) = calculate_mining_probs(v_rep, 4, 8);

        assert_eq!(
            (probs[0] * 10_000.0).round() as u32,
            (100.0 as f64 * 100.0).round() as u32
        );
        assert_eq!(
            (probs[1] * 10_000.0).round() as u32,
            (0.0 as f64 * 100.0).round() as u32
        );
        assert_eq!(
            (probs[2] * 10_000.0).round() as u32,
            (0.0 as f64 * 100.0).round() as u32
        );
        assert_eq!(
            (probs[3] * 10_000.0).round() as u32,
            (0.0 as f64 * 100.0).round() as u32
        );

        assert_eq!(
            (new_prob * 10_000.0).round() as u32,
            (0.0 as f64 * 100.0).round() as u32
        );
    }
}
