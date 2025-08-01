use std::{
    collections::HashSet,
    convert::TryFrom,
    future,
    future::Future,
    sync::{
        Arc,
        atomic::{self, AtomicU16},
    },
};

use actix::{
    ActorFutureExt, ActorTryFutureExt, AsyncContext, Context, ContextFutureSpawner, SystemService,
    WrapFuture,
};
use ansi_term::Color::{White, Yellow};
use futures::future::{FutureExt, try_join_all};

use witnet_data_structures::{
    DEFAULT_VALIDATOR_COUNT_FOR_TESTS,
    chain::{
        Block, BlockHeader, BlockMerkleRoots, BlockTransactions, Bn256PublicKey, CheckpointBeacon,
        CheckpointVRF, ConsensusConstantsWit2, DataRequestOutput, EpochConstants, Hash, Hashable,
        Input, PublicKeyHash, RADTally, TransactionsPool, ValueTransferOutput,
        tapi::{ActiveWips, after_second_hard_fork},
    },
    data_request::{
        DataRequestPool, calculate_witness_reward,
        calculate_witness_reward_before_second_hard_fork, create_tally,
        data_request_has_too_many_witnesses,
    },
    error::TransactionError,
    get_environment, get_protocol_version,
    proto::versioning::{ProtocolVersion, ProtocolVersion::*, VersionedHashable},
    radon_error::RadonError,
    radon_report::{RadonReport, ReportContext, TypeLike},
    staking::{
        stake::totalize_stakes,
        stakes::{QueryStakesKey, StakesTracker},
    },
    transaction::{
        CommitTransaction, CommitTransactionBody, DRTransactionBody, MintTransaction,
        RevealTransaction, RevealTransactionBody, StakeTransactionBody, TallyTransaction,
        UnstakeTransactionBody, VTTransactionBody,
    },
    transaction_factory::{build_commit_collateral, check_commit_collateral},
    utxo_pool::{UnspentOutputsPool, UtxoDiff},
    vrf::{BlockEligibilityClaim, DataRequestEligibilityClaim, VrfMessage},
    wit::Wit,
};
use witnet_futures_utils::TryFutureExt2;
use witnet_rad::{
    conditions::radon_report_from_error,
    error::RadError,
    types::{RadonTypes, serial_iter_decode},
};
use witnet_util::timestamp::get_timestamp;
use witnet_validations::{
    eligibility::{
        current::{Eligibility, Eligible},
        legacy::*,
    },
    validations::{
        block_reward, calculate_liars_and_errors_count_from_tally, dr_transaction_fee,
        merkle_tree_root, run_tally, st_transaction_fee, tally_bytes_on_encode_error,
        update_utxo_diff, validate_stake_transaction, validate_unstake_transaction,
        vt_transaction_fee,
    },
};

use crate::{
    actors::{
        chain_manager::{ChainManager, ChainManagerError, StateMachine},
        messages::{AddCommitReveal, ResolveRA, RunTally},
        rad_manager::RadManager,
    },
    signature_mngr,
};

impl ChainManager {
    /// Try to mine a block
    pub fn try_mine_block(&mut self, ctx: &mut Context<Self>) -> Result<(), ChainManagerError> {
        if !self.mining_enabled {
            return Err(ChainManagerError::MiningIsDisabled);
        }

        // We only want to mine in Synced state
        if self.sm_state != StateMachine::Synced {
            return Err(ChainManagerError::NotSynced {
                current_state: self.sm_state,
            });
        }

        let current_epoch = self.current_epoch.ok_or(ChainManagerError::ChainNotReady)?;
        let own_pkh = self.own_pkh.ok_or(ChainManagerError::ChainNotReady)?;
        let epoch_constants = self
            .epoch_constants
            .ok_or(ChainManagerError::ChainNotReady)?;
        let chain_info = self
            .chain_state
            .chain_info
            .clone()
            .ok_or(ChainManagerError::ChainNotReady)?;

        let max_vt_weight = chain_info.consensus_constants.max_vt_weight;
        let max_dr_weight = chain_info.consensus_constants.max_dr_weight;
        let mining_bf = chain_info.consensus_constants.mining_backup_factor;
        let collateral_minimum = chain_info.consensus_constants.collateral_minimum;
        let minimum_difficulty = chain_info.consensus_constants.minimum_difficulty;
        let initial_block_reward = chain_info.consensus_constants.initial_block_reward;
        let checkpoint_zero_timestamp = chain_info.consensus_constants.checkpoint_zero_timestamp;
        let halving_period = chain_info.consensus_constants.halving_period;

        let previous_block_epoch = chain_info.highest_block_checkpoint.checkpoint;
        let mut beacon = chain_info.highest_block_checkpoint;
        let mut vrf_input = chain_info.highest_vrf_output;
        let protocol_version = get_protocol_version(self.current_epoch);

        // The highest checkpoint beacon should contain the current epoch
        beacon.checkpoint = current_epoch;
        vrf_input.checkpoint = current_epoch;

        let target_hash = if protocol_version == V2_0 {
            let replication_factor = self.consensus_constants_wit2.get_replication_factor(
                current_epoch,
                chain_info.highest_block_checkpoint.checkpoint,
            );
            let eligibility = self
                .chain_state
                .stakes
                .mining_eligibility(own_pkh, current_epoch, replication_factor)
                .map_err(ChainManagerError::Staking)?;

            match eligibility {
                Eligible::Yes => {
                    log::info!("Hurray! Found eligibility for proposing a block candidate!");
                }
                Eligible::No(_) => {
                    log::debug!("No eligibility for proposing a block candidate.");
                    return Ok(());
                }
            }

            Hash::max()
        } else {
            let rep_engine = self.chain_state.reputation_engine.as_ref().unwrap().clone();
            let total_identities =
                u32::try_from(rep_engine.ars().active_identities_number()).unwrap();
            let epochs_with_minimum_difficulty = chain_info
                .consensus_constants
                .epochs_with_minimum_difficulty;

            let active_wips = ActiveWips {
                active_wips: self.chain_state.tapi_engine.wip_activation.clone(),
                block_epoch: current_epoch,
            };

            // invalid: vrf_hash > target_hash
            let (target_hash, _probability) = calculate_randpoe_threshold(
                total_identities,
                mining_bf,
                current_epoch,
                minimum_difficulty,
                epochs_with_minimum_difficulty,
                &active_wips,
            );

            target_hash
        };

        let validator_count = self.chain_state.stakes.validator_count();

        signature_mngr::vrf_prove(VrfMessage::block_mining(vrf_input))
            .map(move |res| {
                res.map_err(|e| log::error!("Failed to create block eligibility proof: {e}"))
                    .map(move |(vrf_proof, vrf_proof_hash)| {
                        // For legacy protocol versions, check if our proof meets some thresholds

                        if vrf_proof_hash > target_hash {
                            Err(())?;
                        }

                        log::info!(
                            "{} Discovered eligibility for mining a block for epoch #{}",
                            Yellow.bold().paint("[Mining]"),
                            Yellow.bold().paint(beacon.checkpoint.to_string())
                        );

                        // TODO: figure out if mining probability estimates make any sense in the context of PoS

                        Ok::<_, ()>(vrf_proof)
                    })
            })
            .flatten_err()
            .into_actor(self)
            .and_then(|vrf_proof, act, _ctx| {
                act.create_tally_transactions()
                    .map(|res| res.map(|tally_transactions| (vrf_proof, tally_transactions)))
                    .into_actor(act)
            })
            .and_then(move |(vrf_proof, tally_transactions), act, _ctx| {
                let eligibility_claim = BlockEligibilityClaim { proof: vrf_proof };

                // TODO: assess if bn256 keys are redundant
                let bn256_public_key = act.bn256_public_key.clone();

                let tapi_version = act.tapi_signals_mask(current_epoch);

                let active_wips = ActiveWips {
                    active_wips: act.chain_state.tapi_engine.wip_activation.clone(),
                    block_epoch: current_epoch,
                };

                // Build the block using the supplied beacon and eligibility proof
                let block_number = act.chain_state.block_number();
                let (block_header, txns) = build_block(
                    (
                        &mut act.transactions_pool,
                        &act.chain_state.unspent_outputs_pool,
                        &mut act.chain_state.data_request_pool,
                    ),
                    max_vt_weight,
                    max_dr_weight,
                    beacon,
                    eligibility_claim,
                    &tally_transactions,
                    own_pkh,
                    epoch_constants,
                    block_number,
                    collateral_minimum,
                    bn256_public_key,
                    act.external_address,
                    act.external_percentage,
                    initial_block_reward,
                    checkpoint_zero_timestamp,
                    halving_period,
                    tapi_version,
                    &active_wips,
                    Some(validator_count),
                    &act.chain_state.stakes,
                    &act.consensus_constants_wit2,
                );

                // Sign the block hash
                let block_header_data = block_header.versioned_hash(protocol_version).data();

                signature_mngr::sign_data(block_header_data)
                    .map(|res| {
                        res.map_err(|e| log::error!("Couldn't sign beacon: {e}"))
                            .map(|block_sig| Block::new(block_header, block_sig, txns))
                    })
                    .into_actor(act)
            })
            .and_then(move |block, act, _ctx| {
                act.future_process_validations(
                    block.clone(),
                    previous_block_epoch,
                    current_epoch,
                    vrf_input,
                    beacon,
                    epoch_constants,
                )
                .map_ok(move |_diff, act, _ctx| {
                    // Send AddCandidates message to self
                    // This will run all the validations again

                    let block_hash = block.versioned_hash(protocol_version);
                    // FIXME(#1773): Currently last_block_proposed is not used, but removing it is a breaking change
                    act.chain_state.node_stats.last_block_proposed = block_hash;
                    act.chain_state.node_stats.block_proposed_count += 1;
                    log::info!(
                        "Proposed block candidate {}",
                        Yellow.bold().paint(block_hash.to_string())
                    );

                    act.process_candidate(block);
                })
                .map_err(|e, _, _| log::error!("Error trying to mine a block: {e}"))
            })
            .map(|_res: Result<(), ()>, _act, _ctx| ())
            .wait(ctx);

        Ok(())
    }

    /// Try to mine a data_request
    // TODO: refactor this procedure into multiple functions that can be tested separately.
    // TODO: double check if this is correct or not, because the need for
    //  using the `unused_assignments` directive smells bad.
    #[allow(unused_assignments)]
    pub fn try_mine_data_request(
        &mut self,
        ctx: &mut Context<Self>,
    ) -> Result<(), ChainManagerError> {
        if !self.mining_enabled {
            return Err(ChainManagerError::MiningIsDisabled);
        }

        let vrf_input = self
            .chain_state
            .chain_info
            .as_ref()
            .map(|x| x.highest_vrf_output);

        let vrf_input = vrf_input.ok_or(ChainManagerError::ChainNotReady)?;
        let own_pkh = self.own_pkh.ok_or(ChainManagerError::ChainNotReady)?;
        let current_epoch = self.current_epoch.ok_or(ChainManagerError::ChainNotReady)?;
        let data_request_timeout = self.data_request_timeout;
        let timestamp = u64::try_from(get_timestamp()).unwrap();
        let consensus_constants = self.consensus_constants();
        let minimum_reppoe_difficulty = consensus_constants.minimum_difficulty;
        let minimum_stake = Wit::from(
            self.consensus_constants_wit2
                .get_validator_min_stake_nanowits(current_epoch),
        );
        // Retrieve all stake entries in which we are the validator
        let stake_entries = self
            .chain_state
            .stakes
            .query_stakes(QueryStakesKey::Validator(own_pkh))
            .unwrap_or_default();
        // Retrieve the withdrawer address of the first stake entry in which we are the validator
        let first_withdrawer_address = stake_entries.first().map(|stake| stake.key.withdrawer);
        let mut available_stake = totalize_stakes(stake_entries).unwrap_or_default();

        // Data Request mining
        let dr_pointers = self
            .chain_state
            .data_request_pool
            .get_dr_output_pointers_by_epoch(current_epoch);

        let rep_eng = self.chain_state.reputation_engine.as_ref().unwrap();
        let is_ars_member = rep_eng.is_ars_member(&own_pkh);

        let my_reputation = rep_eng.trs().get(&own_pkh).0 + 1;
        let my_eligibility = rep_eng.get_eligibility(&own_pkh) + 1;
        let total_active_reputation = rep_eng.total_active_reputation();
        let num_active_identities =
            u32::try_from(rep_eng.ars().active_identities_number()).unwrap();
        log::debug!("{} data requests for this epoch", dr_pointers.len());
        log::debug!(
            "Reputation: {my_reputation}, eligibility: {my_eligibility}, total: {total_active_reputation}, active identities: {num_active_identities}",
        );

        // `current_retrieval_count` keeps track of how many sources are being retrieved in this
        // epoch by using a reference-counted atomic counter that can be read and updated safely.
        let current_retrieval_count = Arc::new(AtomicU16::new(0u16));
        let maximum_retrieval_count = self.data_request_max_retrievals_per_epoch;

        for (dr_pointer, dr_state) in dr_pointers.into_iter().filter_map(|dr_pointer| {
            // Filter data requests that are not in data_request_pool
            self.chain_state
                .data_request_pool
                .data_request_state(&dr_pointer)
                .map(|dr_state| (dr_pointer, dr_state.clone()))
        }) {
            // The protocol version under which a data request needs to be resolved is decided
            // based on the epoch of the block in which the data request was included.
            let protocol_version = get_protocol_version(Some(dr_state.epoch));

            // It's possible a data request requesting too many witnesses are in our local data
            // request pool (e.g., when we failed to validate a proposed block). Do not attempt
            // to solve this data request.
            let validator_count = self.chain_state.stakes.validator_count();
            if data_request_has_too_many_witnesses(
                &dr_state.data_request,
                validator_count,
                Some(dr_state.epoch),
            ) {
                continue;
            }

            let (checkpoint_period, max_rounds) = match &self.chain_state.chain_info {
                Some(x) => (
                    // Unwraps should be safe if we have a chain_info object
                    self.epoch_constants
                        .unwrap()
                        .get_epoch_period(current_epoch)
                        .unwrap(),
                    x.consensus_constants.extra_rounds + 1,
                ),
                None => {
                    log::error!("ChainInfo is None");
                    return Err(ChainManagerError::ChainNotReady);
                }
            };

            let num_witnesses = dr_state.data_request.witnesses;
            let round = dr_state.info.current_commit_round;
            let num_backup_witnesses = dr_state.backup_witnesses();
            // The vrf_input used to create and verify data requests must be set to the current epoch
            let dr_vrf_input = CheckpointVRF {
                checkpoint: current_epoch,
                ..vrf_input
            };
            let active_wips = ActiveWips {
                active_wips: self.chain_state.tapi_engine.wip_activation.clone(),
                block_epoch: current_epoch,
            };
            let collateral_age = self
                .consensus_constants_wit2
                .get_collateral_age(&active_wips);
            let (target_hash, probability) = if protocol_version >= V2_0 {
                let (eligibility, target_hash, probability) = self
                    .chain_state
                    .stakes
                    .witnessing_eligibility(
                        own_pkh,
                        current_epoch,
                        num_witnesses,
                        round,
                        max_rounds,
                    )
                    .map_err(ChainManagerError::Staking)?;

                match eligibility {
                    Eligible::Yes => {
                        log::info!("Hurray! Found eligibility for solving a data request!");
                    }
                    Eligible::No(_) => {
                        log::info!("No eligibility for solving a data request.");
                        return Ok(());
                    }
                }

                (target_hash, probability)
            } else {
                calculate_reppoe_threshold(
                    rep_eng,
                    &own_pkh,
                    num_witnesses + num_backup_witnesses,
                    minimum_reppoe_difficulty,
                    &active_wips,
                )
            };

            // Grab a reference to `current_retrieval_count`
            let cloned_retrieval_count = Arc::clone(&current_retrieval_count);
            let cloned_retrieval_count2 = Arc::clone(&current_retrieval_count);
            let added_retrieval_count =
                u16::try_from(dr_state.data_request.data_request.retrieve.len())
                    .unwrap_or(u16::MAX);

            let collateral_amount = Wit::from_nanowits(if dr_state.data_request.collateral == 0 {
                self.chain_state
                    .chain_info
                    .as_ref()
                    .unwrap()
                    .consensus_constants
                    .collateral_minimum
            } else {
                dr_state.data_request.collateral
            });

            let block_number_limit = self
                .chain_state
                .block_number()
                .saturating_sub(collateral_age);
            if protocol_version >= V2_0 {
                // In 2_x protocol, check if we have enough stake before attempting retrieval
                if collateral_amount > available_stake - minimum_stake {
                    log::debug!(
                        "Mining data request: Insufficient stake, the data request needs {collateral_amount} Wit but we have {available_stake} left"
                    );
                    continue;
                }
            } else {
                // In 1_x protocol, check if we have enough collateralizable unspent outputs before
                // attempting retrieval
                if !check_commit_collateral(
                    collateral_amount,
                    &self.chain_state.own_utxos,
                    own_pkh,
                    &self.chain_state.unspent_outputs_pool,
                    timestamp,
                    // The block number must be lower than this limit
                    block_number_limit,
                ) {
                    log::debug!(
                        "Mining data request: Insufficient collateral, the data request needs {collateral_amount} mature wits"
                    );
                    continue;
                }
            }

            // This is the message that will be covered by the VRF.
            // In V2_X, the message includes the withdrawer address in order to eventually
            // distinguish eligibility for stake entries belonging to multiple withdrawers but
            // operated by a single validator acting as a delegatee.
            let vrf_message = if protocol_version >= V2_0 {
                VrfMessage::data_request_v2(dr_vrf_input, dr_pointer, first_withdrawer_address)
            } else {
                VrfMessage::data_request_v1(dr_vrf_input, dr_pointer)
            };

            signature_mngr::vrf_prove(vrf_message)
                .map(move |res|
                    res.map_err(move |e| {
                        log::error!(
                            "Couldn't create VRF proof for data request {dr_pointer}: {e}"
                        )
                    })
                        .map(move |(vrf_proof, vrf_proof_hash)| {
                            // invalid: vrf_hash > target_hash
                            let proof_invalid = vrf_proof_hash > target_hash;

                            log::debug!(
                                "{num_witnesses} witnesses and {num_backup_witnesses} backup witnesses"
                            );
                            log::debug!(
                                "Probability to be eligible for this data request: {:.6}%",
                                probability * 100.0
                            );
                            log::trace!("[DR] Target hash: {target_hash}");
                            log::trace!("[DR] Our proof:   {vrf_proof_hash}");
                            if proof_invalid {
                                log::debug!("No eligibility for data request {dr_pointer}");
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
                )
                .flatten_err()
                .into_actor(self)
                // Refrain from trying to resolve any more requests if we have already hit the limit
                // of retrievals per epoch.
                .and_then(move |vrf_proof, act, _| {
                    let mut start_retrieval_count = cloned_retrieval_count.load(atomic::Ordering::Relaxed);
                    let mut final_retrieval_count = start_retrieval_count.saturating_add(added_retrieval_count);

                    // Keep track of how many times we are eligible for witnessing, regardless of
                    //  being able to satisfy stake or collateral requirements.
                    // The actual number of completed witnessing acts is tracked separately and
                    //  updated later on.
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
                            let internal_retrieval_count_result = cloned_retrieval_count.compare_exchange_weak(
                                start_retrieval_count,
                                final_retrieval_count,
                                atomic::Ordering::Relaxed,
                                atomic::Ordering::Relaxed,
                            );

                            match internal_retrieval_count_result {
                                Ok(_) => {
                                    // The counter update was updated successfully, we can move on.
                                    break actix::fut::ok(vrf_proof);
                                }
                                Err(internal_retrieval_count) => {
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
                    }
                })
                // V1_X:
                // Collect outputs to be used as input for collateralized commitment,
                //  as well as outputs for change.
                // V2_X:
                // Check that we have enough stake.
                .and_then(move |vrf_proof, act, _| {
                    if protocol_version >= V2_0 {
                        let after_balance = available_stake - collateral_amount;
                        if after_balance > minimum_stake {
                            // `available_stake` is mutated here so the next cycle of this iterator
                            //  takes into account can acknowledge depletion of the available stake.
                            available_stake = after_balance;
                            actix::fut::ok((vrf_proof, Default::default()))
                        } else {
                            log::warn!("Not enough available stake for witnessing data request {}: Available balance: {}, Required stake: {}",
                                Yellow.bold().paint(dr_pointer.to_string()),
                                available_stake,
                                collateral_amount,
                            );
                            actix::fut::err(())
                        }
                    } else {
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
                            block_number_limit,
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
                                log::error!("Unexpected error when trying to select UTXOs to be used for collateral in data request {dr_pointer}: {e}");
                                actix::fut::err(())
                            }
                        }
                    }
                })
                .and_then(move |(vrf_proof, collateral), act, _| {
                    let rad_request = dr_state.data_request.data_request.clone();

                    // Send ResolveRA message to RADManager
                    let active_wips = ActiveWips {
                        active_wips: act.chain_state.tapi_engine.wip_activation.clone(),
                        block_epoch: current_epoch,
                    };
                    let rad_manager_addr = RadManager::from_registry();
                    rad_manager_addr
                        .send(ResolveRA {
                            rad_request,
                            timeout: data_request_timeout,
                            active_wips,
                            too_many_witnesses: false,
                        })
                        .map(move |res|
                            res.map(move |result| match result {
                                    Ok(value) => {
                                if let RadonTypes::RadonError(error) = &value.result {
                                    if error.inner() == &RadError::InconsistentSource {
                                        log::warn!("Refraining not to commit to data request {dr_pointer} because the sources are apparently inconsistent");
                                        return Err(())
                                    }
                                }

                                Ok((vrf_proof, collateral, value))
                            },
                                    Err(e) => {
                                        log::error!("Couldn't resolve rad request {dr_pointer}: {e}");
                                        Err(())
                                    }
                                })
                                .map_err(move |e| {
                                    // If resolving a data request results in a panic in the
                                    // message handler, ignore this data request
                                    log::error!("Couldn't resolve rad request {dr_pointer}: {e}")
                                })
                        )
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
                            if active_wips.wip0026() {
                                log::warn!("Couldn't encode reveal value to bytes, will commit RadError::EncodeReveal instead: {e}");
                                let reveal_bytes: Vec<u8> = RadonTypes::RadonError(RadonError::try_from(RadError::EncodeReveal).unwrap()).encode().unwrap();
                                actix::fut::ok((reveal_bytes, vrf_proof_dr, collateral))
                            } else {
                                log::error!("Couldn't encode reveal value to bytes: {e}");
                                actix::fut::err(())
                            }
                        }
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

                    async move {
                        let reveal_signatures = signature_mngr::sign_transaction(&reveal_body, 1)
                            .await
                            .map_err(|e| log::error!("Couldn't sign reveal body: {e}"))?;

                        // Commitment is the hash of the RevealTransaction signature
                        // that will be published later
                        let commitment = reveal_signatures[0].signature.hash();
                        let (inputs, outputs) = collateral;
                        let commit_body =
                            CommitTransactionBody::new(dr_pointer, commitment, vrf_proof_dr, inputs, outputs, bn256_public_key);

                        signature_mngr::sign_transaction_hash(commit_body.hash(), 1)
                            .map(|res| res
                                .map(|commit_signatures| {
                                    let commit_transaction =
                                        CommitTransaction::new(commit_body, commit_signatures);
                                    let reveal_transaction =
                                        RevealTransaction::new(reveal_body, reveal_signatures);
                                    (commit_transaction, reveal_transaction)
                                })
                                .map_err(|e| log::error!("Couldn't sign commit body: {e}")))
                            .await
                    }
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
                .map(|_res: Result<(), ()>, _act, _ctx| ())
                .spawn(ctx);
        }

        Ok(())
    }

    #[allow(clippy::needless_collect)]
    fn create_tally_transactions(
        &mut self,
    ) -> impl Future<Output = Result<Vec<TallyTransaction>, ()>> + use<> {
        let block_epoch = self.current_epoch.unwrap();
        let data_request_pool = &self.chain_state.data_request_pool;
        let collateral_minimum = self
            .chain_state
            .chain_info
            .as_ref()
            .unwrap()
            .consensus_constants
            .collateral_minimum;

        let active_wips = ActiveWips {
            active_wips: self.chain_state.tapi_engine.wip_activation.clone(),
            block_epoch,
        };

        let validator_count = self.chain_state.stakes.validator_count();

        let dr_reveals = data_request_pool
            .get_all_reveals(&active_wips, Some(validator_count), Some(block_epoch))
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
                    let active_wips_inside_move = active_wips.clone();

                    async move {
                        log::debug!("Building tally for data request {dr_pointer}");

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
                            &active_wips_inside_move
                        );

                        let min_consensus_ratio =
                            f64::from(dr_state.data_request.min_consensus_percentage) / 100.0;

                        let committers: HashSet<PublicKeyHash> =
                            dr_state.info.commits.keys().cloned().collect();
                        let commits_count = committers.len();

                        let rad_manager_addr = RadManager::from_registry();

                        let too_many_witnesses = data_request_has_too_many_witnesses(
                            &dr_state.data_request,
                            validator_count,
                            Some(dr_state.epoch),
                        );

                        // The result of `RunTally` will be published as tally
                        let tally_result = rad_manager_addr
                            .send(RunTally {
                                min_consensus_ratio,
                                reports: reports.clone(),
                                script: dr_state.data_request.data_request.tally.clone(),
                                commits_count,
                                active_wips: active_wips_inside_move.clone(),
                                too_many_witnesses,
                            })
                            .await
                            .unwrap_or_else(|e| {
                                // If RunTally results in a panic, the result of the tally should be
                                // RadError::Unknown
                                // This is because this block must have a tally transaction to be
                                // considered valid
                                log::warn!("Couldn't run tally: {e}");
                                if after_second_hard_fork(block_epoch, get_environment()) {
                                    radon_report_from_error(RadError::Unknown, reveals.len())
                                } else {
                                    RadonReport::from_result(Err(RadError::Unknown), &ReportContext::default())
                                }
                            });

                        let tally = create_tally(
                            dr_pointer,
                            &dr_state.data_request,
                            dr_state.pkh,
                            &tally_result,
                            reveals.iter().map(|r| r.body.pkh).collect(),
                            committers,
                            collateral_minimum,
                            tally_bytes_on_encode_error(),
                            &active_wips_inside_move,
                            get_protocol_version(Some(dr_state.epoch)),
                        );

                        log::info!(
                            "{} Created Tally for Data Request {} with result: {}\n{}",
                            Yellow.bold().paint("[Data Request]"),
                            Yellow.bold().paint(dr_pointer.to_string()),
                            Yellow
                                .bold()
                                .paint(format!("{}", &tally_result.into_inner())),
                            White.bold().paint(reports.into_iter().fold(
                                String::from("Reveals:"),
                                |acc, item| format!(
                                    "{}\n\t* {}",
                                    acc,
                                    item.into_inner()
                                )
                            )),
                        );

                        Result::<_, ()>::Ok(tally)
                    }
                    // This future should always return Ok because join_all short-circuits on the
                    // first Err, and we want to keep creating tallies after the first error
                    // Map Result<T, E> to Result<Option<T>, ()>
                    .then(|x| future::ready(Ok(x.ok())))
                });

        async {
            let res = try_join_all(future_tally_transactions).await;
            // Map Option<Vec<T>> to Vec<T>, this returns all the non-error results
            res.map(|x| x.into_iter().flatten().collect())
        }
    }
}

/// Build a new Block using the supplied leadership proof and by filling transactions from the
/// `transaction_pool`
/// Returns an unsigned block!
/// TODO: simplify function signature, e.g. through merging multiple related fields into new data structures.
#[allow(clippy::too_many_arguments)]
pub fn build_block(
    pools_ref: (
        &mut TransactionsPool,
        &UnspentOutputsPool,
        &mut DataRequestPool,
    ),
    max_vt_weight: u32,
    max_dr_weight: u32,
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
    initial_block_reward: u64,
    checkpoint_zero_timestamp: i64,
    halving_period: u32,
    tapi_signals: u32,
    active_wips: &ActiveWips,
    validator_count: Option<usize>,
    stakes: &StakesTracker,
    consensus_constants_wit2: &ConsensusConstantsWit2,
) -> (BlockHeader, BlockTransactions) {
    let validator_count = validator_count.unwrap_or(DEFAULT_VALIDATOR_COUNT_FOR_TESTS);
    let (transactions_pool, unspent_outputs_pool, dr_pool) = pools_ref;
    let epoch = beacon.checkpoint;
    let mut utxo_diff = UtxoDiff::new(unspent_outputs_pool, block_number);

    // Get all the unspent transactions and calculate the sum of their fees
    let mut transaction_fees: u64 = 0;
    let mut vt_weight: u32 = 0;
    let mut dr_weight: u32 = 0;
    let mut st_weight: u32 = 0;
    let mut ut_weight: u32 = 0;
    let mut value_transfer_txns = Vec::new();
    let mut data_request_txns = Vec::new();
    let mut tally_txns = Vec::new();
    let mut stake_txns = Vec::new();
    let mut unstake_txns = Vec::new();

    // Calculate the base weight for different types of transactions, to know when to give up trying to fit more
    // transactions into a block
    let min_vt_weight =
        VTTransactionBody::new(vec![Input::default()], vec![ValueTransferOutput::default()])
            .weight();
    let min_st_weight =
        StakeTransactionBody::new(vec![Input::default()], Default::default(), None).weight();
    let min_ut_weight =
        UnstakeTransactionBody::new(PublicKeyHash::default(), Default::default(), 0, 0).weight();

    for vt_tx in transactions_pool.vt_iter() {
        let transaction_weight = vt_tx.weight();
        let transaction_fee = match vt_transaction_fee(vt_tx, &utxo_diff, epoch, epoch_constants) {
            Ok(x) => x,
            Err(e) => {
                log::warn!("Error when calculating transaction fee for transaction: {e}");
                continue;
            }
        };

        let new_vt_weight = vt_weight.saturating_add(transaction_weight);
        if new_vt_weight <= max_vt_weight {
            update_utxo_diff(
                &mut utxo_diff,
                vt_tx.body.inputs.iter(),
                vt_tx.body.outputs.iter(),
                vt_tx.hash(),
                epoch,
                epoch_constants,
                checkpoint_zero_timestamp,
            );
            value_transfer_txns.push(vt_tx.clone());
            transaction_fees = transaction_fees.saturating_add(transaction_fee);
            vt_weight = new_vt_weight;
        }

        // The condition to stop is if the free space in the block for VTTransactions
        // is less than the minimum value transfer transaction weight
        if vt_weight > max_vt_weight.saturating_sub(min_vt_weight) {
            break;
        }
    }

    for ta_tx in tally_transactions {
        if let Some(dr_state) = dr_pool.data_request_state(&ta_tx.dr_pointer) {
            tally_txns.push(ta_tx.clone());
            let commits_count = dr_state.info.commits.len();
            let reveals_count = dr_state.info.reveals.len();

            let (liars_count, errors_count) = calculate_liars_and_errors_count_from_tally(ta_tx);

            let collateral = if dr_state.data_request.collateral == 0 {
                collateral_minimum
            } else {
                dr_state.data_request.collateral
            };

            // Remainder collateral goes to the miner
            let (_, extra_tally_fee) = if after_second_hard_fork(epoch, get_environment()) {
                calculate_witness_reward(
                    commits_count,
                    liars_count,
                    errors_count,
                    dr_state.data_request.witness_reward,
                    collateral,
                    active_wips.wip0023(),
                )
            } else {
                calculate_witness_reward_before_second_hard_fork(
                    commits_count,
                    reveals_count,
                    liars_count,
                    errors_count,
                    dr_state.data_request.witness_reward,
                    collateral,
                )
            };
            transaction_fees += extra_tally_fee;
        } else {
            log::warn!(
                "Data Request pointed by tally transaction doesn't exist in DataRequestPool"
            );
        }
    }

    let (commit_txns, commits_fees, solved_dr_pointers) = transactions_pool.remove_commits(dr_pool);
    transaction_fees += commits_fees;

    let (reveal_txns, reveals_fees) = transactions_pool.get_reveals(dr_pool);
    let reveal_txns: Vec<RevealTransaction> = reveal_txns.into_iter().cloned().collect();
    transaction_fees += reveals_fees;

    // Calculate data request not solved weight
    let mut dr_pointers: HashSet<Hash> = dr_pool
        .get_dr_output_pointers_by_epoch(epoch)
        .into_iter()
        .collect();
    for dr in solved_dr_pointers {
        dr_pointers.remove(&dr);
    }

    for dr in dr_pointers {
        let unsolved_dro = dr_pool.get_dr_output(&dr);
        if let Some(dro) = unsolved_dro {
            dr_weight = dr_weight
                .saturating_add(dro.weight())
                .saturating_add(dro.extra_weight());
        }
    }

    let dro = DataRequestOutput {
        witnesses: 1,
        ..DataRequestOutput::default()
    };
    let min_dr_weight = DRTransactionBody::new(vec![Input::default()], dro, vec![]).weight();
    for dr_tx in transactions_pool.dr_iter() {
        let transaction_weight = dr_tx.weight();
        let transaction_fee = match dr_transaction_fee(dr_tx, &utxo_diff, epoch, epoch_constants) {
            Ok(x) => x,
            Err(e) => {
                log::warn!("Error when calculating transaction fee for transaction: {e}");
                continue;
            }
        };

        let new_dr_weight = dr_weight.saturating_add(transaction_weight);
        if new_dr_weight <= max_dr_weight {
            update_utxo_diff(
                &mut utxo_diff,
                dr_tx.body.inputs.iter(),
                dr_tx.body.outputs.iter(),
                dr_tx.hash(),
                epoch,
                epoch_constants,
                checkpoint_zero_timestamp,
            );

            // Number of data request witnesses is limited by a fraction of the number of validators
            // If the requested witnesses exceed this fraction, resolve the data request with an error
            if data_request_has_too_many_witnesses(
                &dr_tx.body.dr_output,
                validator_count,
                Some(epoch),
            ) {
                log::debug!("Data request {} has too many witnesses", dr_tx.hash());

                // Temporarily insert the data request into the dr_pool
                if let Err(e) = dr_pool.process_data_request(dr_tx, epoch, None) {
                    log::error!("Error adding data request to the data request pool: {e}");
                    continue;
                }
                if let Some(dr_state) = dr_pool.data_request_state_mutable(&dr_tx.hash()) {
                    dr_state.update_stage(0, true);
                } else {
                    log::error!("Could not find data request state");
                    continue;
                }

                // The result of `RunTally` will be published as tally
                let tally_script = RADTally {
                    filters: vec![],
                    reducer: 0,
                };
                let tally_result = run_tally(
                    vec![], // reveals
                    &tally_script,
                    0.0, // minimum consensus percentage
                    0,   // commit count
                    active_wips,
                    true, // too many witnesses
                );

                let pkh = dr_tx.signatures[0].public_key.pkh();
                tally_txns.push(create_tally(
                    dr_tx.hash(),
                    &dr_tx.body.dr_output,
                    pkh,
                    &tally_result,
                    vec![],         // No revealers
                    HashSet::new(), // No committers
                    collateral_minimum,
                    tally_bytes_on_encode_error(),
                    active_wips,
                    get_protocol_version(Some(epoch)),
                ));
            }

            data_request_txns.push(dr_tx.clone());
            transaction_fees = transaction_fees.saturating_add(transaction_fee);
            dr_weight = new_dr_weight;
        }

        // The condition to stop is if the free space in the block for DRTransactions
        // is less than the minimum data request transaction weight
        if dr_weight > max_dr_weight.saturating_sub(min_dr_weight) {
            break;
        }
    }

    let protocol_version = ProtocolVersion::from_epoch(epoch);

    if protocol_version > V1_7 {
        let max_st_weight = consensus_constants_wit2.get_maximum_stake_block_weight(epoch);
        let mut invalid_stake_transactions = Vec::<Hash>::new();
        let mut included_validators = HashSet::<PublicKeyHash>::new();
        for st_tx in transactions_pool.st_iter() {
            let validator_pkh = st_tx.body.output.authorization.public_key.pkh();
            if included_validators.contains(&validator_pkh) {
                log::debug!(
                    "Cannot include more than one stake transaction for {validator_pkh} in a single block"
                );
                continue;
            }

            // Revalidate stake transaction since concurrent stake transactions could have invalidated the transaction
            // we want to include here which would result in producing an invalid block.
            let min_stake = consensus_constants_wit2.get_validator_min_stake_nanowits(epoch);
            let max_stake = consensus_constants_wit2.get_validator_max_stake_nanowits(epoch);
            if let Err(e) = validate_stake_transaction(
                st_tx,
                &utxo_diff,
                epoch,
                epoch_constants,
                &mut vec![],
                stakes,
                min_stake,
                max_stake,
            ) {
                log::warn!(
                    "Cannot build block with stake transaction {}: {}",
                    st_tx.hash(),
                    e
                );
                invalid_stake_transactions.push(st_tx.hash());
                continue;
            }

            let transaction_weight = st_tx.weight();
            let transaction_fee =
                match st_transaction_fee(st_tx, &utxo_diff, epoch, epoch_constants) {
                    Ok(x) => x,
                    Err(e) => {
                        log::warn!("Error when calculating transaction fee for transaction: {e}");
                        continue;
                    }
                };

            let new_st_weight = st_weight.saturating_add(transaction_weight);
            if new_st_weight <= max_st_weight {
                update_utxo_diff(
                    &mut utxo_diff,
                    st_tx.body.inputs.iter(),
                    st_tx.body.change.iter(),
                    st_tx.hash(),
                    epoch,
                    epoch_constants,
                    checkpoint_zero_timestamp,
                );
                stake_txns.push(st_tx.clone());
                transaction_fees = transaction_fees.saturating_add(transaction_fee);
                st_weight = new_st_weight;
            }

            // The condition to stop is if the free space in the block for VTTransactions
            // is less than the minimum stake transaction weight
            if st_weight > max_st_weight.saturating_sub(min_st_weight) {
                break;
            }

            included_validators.insert(validator_pkh);
        }

        transactions_pool.remove_invalid_stake_transactions(invalid_stake_transactions);
    } else {
        transactions_pool.clear_stake_transactions();
    }

    if protocol_version > V1_8 {
        let mut included_validators = HashSet::<PublicKeyHash>::new();
        let mut invalid_unstake_transactions = Vec::<Hash>::new();
        let max_ut_weight = consensus_constants_wit2.get_maximum_unstake_block_weight(epoch);
        for ut_tx in transactions_pool.ut_iter() {
            let validator_pkh = ut_tx.body.operator;
            if included_validators.contains(&validator_pkh) {
                log::debug!(
                    "Cannot include more than one unstake transaction for {validator_pkh} in a single block"
                );
                continue;
            }

            // Revalidate unstake transaction since concurrent unstake transactions could have invalidated the transaction
            // we want to include here which would result in producing an invalid block.
            let min_stake = consensus_constants_wit2.get_validator_min_stake_nanowits(epoch);
            let unstake_delay = consensus_constants_wit2.get_unstaking_delay_seconds(epoch);
            if let Err(e) =
                validate_unstake_transaction(ut_tx, epoch, stakes, min_stake, unstake_delay)
            {
                log::warn!(
                    "Cannot build block with unstake transaction {}: {}",
                    ut_tx.hash(),
                    e
                );
                invalid_unstake_transactions.push(ut_tx.hash());
                continue;
            }

            let transaction_weight = ut_tx.weight();
            let new_ut_weight = ut_weight.saturating_add(transaction_weight);
            if new_ut_weight <= max_ut_weight {
                update_utxo_diff(
                    &mut utxo_diff,
                    vec![],
                    vec![&ut_tx.body.withdrawal],
                    ut_tx.hash(),
                    epoch,
                    epoch_constants,
                    checkpoint_zero_timestamp,
                );
                unstake_txns.push(ut_tx.clone());
                transaction_fees = transaction_fees.saturating_add(ut_tx.body.fee);
                ut_weight = new_ut_weight;
            }

            // The condition to stop is if the free space in the block for VTTransactions
            // is less than the minimum stake transaction weight
            if ut_weight > max_ut_weight.saturating_sub(min_ut_weight) {
                break;
            }

            included_validators.insert(validator_pkh);
        }

        transactions_pool.remove_invalid_unstake_transactions(invalid_unstake_transactions);
        log::info!(
            "There are {} unstake transactions in the transactions pool",
            transactions_pool.ut_len()
        );
    } else {
        transactions_pool.clear_unstake_transactions();
    }

    // Include Mint Transaction by miner
    let mint = if protocol_version == V2_0 {
        let mut mint = MintTransaction::default();
        mint.epoch = epoch;

        mint
    } else {
        let reward = block_reward(epoch, initial_block_reward, halving_period) + transaction_fees;
        MintTransaction::with_external_address(
            epoch,
            reward,
            own_pkh,
            external_address,
            external_percentage,
        )
    };

    // Compute `hash_merkle_root` and build block header
    let vt_hash_merkle_root = merkle_tree_root(&value_transfer_txns);
    let dr_hash_merkle_root = merkle_tree_root(&data_request_txns);
    let commit_hash_merkle_root = merkle_tree_root(&commit_txns);
    let reveal_hash_merkle_root = merkle_tree_root(&reveal_txns);
    let tally_hash_merkle_root = merkle_tree_root(&tally_txns);

    let stake_hash_merkle_root = if protocol_version == V1_7 {
        log::debug!("Legacy protocol: the default stake hash merkle root will be used");
        Hash::default()
    } else {
        log::debug!("Pseudo-2.0 protocol: a merkle tree will be built for the stake transactions");
        merkle_tree_root(&stake_txns)
    };

    let unstake_hash_merkle_root = if protocol_version < V2_0 {
        Hash::default()
    } else {
        merkle_tree_root(&unstake_txns)
    };

    let merkle_roots = BlockMerkleRoots {
        mint_hash: mint.hash(),
        vt_hash_merkle_root,
        dr_hash_merkle_root,
        commit_hash_merkle_root,
        reveal_hash_merkle_root,
        tally_hash_merkle_root,
        stake_hash_merkle_root,
        unstake_hash_merkle_root,
    };

    let block_header = BlockHeader {
        signals: tapi_signals,
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
        stake_txns,
        unstake_txns,
    };

    (block_header, txns)
}

#[cfg(test)]
mod tests {
    use std::{collections::HashMap, convert::TryInto};

    use witnet_crypto::secp256k1::{
        PublicKey as Secp256k1_PublicKey, SecretKey as Secp256k1_SecretKey,
    };
    use witnet_crypto::signature::{sign, verify};
    use witnet_data_structures::{chain::*, transaction::*, vrf::VrfCtx};
    use witnet_protected::Protected;
    use witnet_validations::validations::validate_block_signature;

    use crate::actors::chain_manager::verify_signatures;

    use super::*;

    const CHECKPOINT_ZERO_TIMESTAMP: i64 = 1_602_666_000;
    const INITIAL_BLOCK_REWARD: u64 = 250 * 1_000_000_000;
    const HALVING_PERIOD: u32 = 3_500_000;

    #[test]
    fn build_empty_block() {
        // Initialize transaction_pool with 1 transaction
        let mut transaction_pool = TransactionsPool::default();
        let transaction = Transaction::ValueTransfer(VTTransaction::new(
            VTTransactionBody::new(vec![Input::default()], vec![ValueTransferOutput::default()]),
            vec![],
        ));
        transaction_pool.insert(transaction.clone(), 0);

        let unspent_outputs_pool = UnspentOutputsPool::default();
        let mut dr_pool = DataRequestPool::default();

        // Set `max_vt_weight` and `max_dr_weight` to zero (no transaction should be included)
        let max_vt_weight = 0;
        let max_dr_weight = 0;

        // Fields required to mine a block
        let block_beacon = CheckpointBeacon::default();
        let block_proof = BlockEligibilityClaim::default();
        let block_number = 1;
        let collateral_minimum = 1_000_000_000;
        let active_wips = ActiveWips::default();

        // Build empty block (because max weight is zero)
        let (block_header, txns) = build_block(
            (&mut transaction_pool, &unspent_outputs_pool, &mut dr_pool),
            max_vt_weight,
            max_dr_weight,
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
            INITIAL_BLOCK_REWARD,
            CHECKPOINT_ZERO_TIMESTAMP,
            HALVING_PERIOD,
            0,
            &active_wips,
            None,
            &StakesTracker::default(),
            &ConsensusConstantsWit2::default(),
        );
        let block = Block::new(block_header, KeyedSignature::default(), txns);

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
        let transaction = Transaction::ValueTransfer(VTTransaction::new(
            VTTransactionBody::new(vec![Input::default()], vec![ValueTransferOutput::default()]),
            vec![],
        ));
        transaction_pool.insert(transaction, 0);

        let unspent_outputs_pool = UnspentOutputsPool::default();
        let mut dr_pool = DataRequestPool::default();

        // Set `max_vt_weight` and `max_dr_weight` to zero (no transaction should be included)
        let max_vt_weight = 0;
        let max_dr_weight = 0;

        // Fields required to mine a block
        let block_beacon = CheckpointBeacon::default();

        let vrf_input = CheckpointVRF {
            hash_prev_vrf: LAST_VRF_INPUT.parse().unwrap(),
            checkpoint: 0,
        };

        // Add valid vrf proof
        let vrf = &mut VrfCtx::secp256k1().unwrap();
        let secret_key = SecretKey {
            bytes: Protected::from(vec![0xcd; 32]),
        };
        let block_proof = BlockEligibilityClaim::create(vrf, &secret_key, vrf_input).unwrap();
        let block_number = 1;
        let collateral_minimum = 1_000_000_000;

        let active_wips: ActiveWips = ActiveWips {
            active_wips: HashMap::default(),
            block_epoch: 0,
        };

        // Build empty block (because max weight is zero)

        let (block_header, txns) = build_block(
            (&mut transaction_pool, &unspent_outputs_pool, &mut dr_pool),
            max_vt_weight,
            max_dr_weight,
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
            INITIAL_BLOCK_REWARD,
            CHECKPOINT_ZERO_TIMESTAMP,
            HALVING_PERIOD,
            0,
            &active_wips,
            None,
            &StakesTracker::default(),
            &ConsensusConstantsWit2::default(),
        );

        // Create a KeyedSignature
        let Hash::SHA256(data) = block_header.versioned_hash(get_protocol_version(Some(0)));
        let secret_key =
            Secp256k1_SecretKey::from_slice(&[0xcd; 32]).expect("32 bytes, within curve order");
        let public_key = Secp256k1_PublicKey::from_secret_key_global(&secret_key);
        let signature = sign(secret_key, &data).unwrap();
        let witnet_pk = PublicKey::from(public_key);
        let witnet_signature = Signature::from(signature);

        let block = Block::new(
            block_header,
            KeyedSignature {
                signature: witnet_signature,
                public_key: witnet_pk,
            },
            txns,
        );

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
        let mut signatures_to_verify = vec![];
        assert!(validate_block_signature(&block, &mut signatures_to_verify).is_ok());

        assert!(verify_signatures(signatures_to_verify.clone(), vrf).is_ok());
    }

    static MILLION_TX_OUTPUT: &str =
        "0f0f000000000000000000000000000000000000000000000000000000000000:0";

    static MY_PKH_1: &str = "wit18cfejmk3305y9kw5xqa59rwnpjzahr57us48vm";

    #[test]
    fn build_block_with_vt_transactions() {
        let output1_pointer: OutputPointer = MILLION_TX_OUTPUT.parse().unwrap();
        let input = vec![Input::new(output1_pointer)];
        let vto1 = ValueTransferOutput {
            value: 1,
            ..Default::default()
        };
        let vto2 = ValueTransferOutput {
            value: 2,
            ..Default::default()
        };
        let vto3 = ValueTransferOutput {
            value: 3,
            ..Default::default()
        };
        let one_output = vec![vto1.clone()];
        let two_outputs = vec![vto1.clone(), vto2];
        let two_outputs2 = vec![vto1, vto3];

        let vt_body_one_output = VTTransactionBody::new(input.clone(), one_output);
        let vt_body_two_outputs1 = VTTransactionBody::new(input.clone(), two_outputs);
        let vt_body_two_outputs2 = VTTransactionBody::new(input, two_outputs2);

        // Build sample transactions
        let vt_tx1 = VTTransaction::new(vt_body_one_output, vec![]);
        let vt_tx2 = VTTransaction::new(vt_body_two_outputs1, vec![]);
        let vt_tx3 = VTTransaction::new(vt_body_two_outputs2, vec![]);

        let transaction_1 = Transaction::ValueTransfer(vt_tx1.clone());
        let transaction_2 = Transaction::ValueTransfer(vt_tx2);
        let transaction_3 = Transaction::ValueTransfer(vt_tx3);

        // Set `max_vt_weight` to fit only `transaction_1` weight
        let max_vt_weight = vt_tx1.weight();
        let max_dr_weight = 0;

        // Insert transactions into `transactions_pool`
        let mut transaction_pool = TransactionsPool::default();
        transaction_pool.insert(transaction_1, 1);
        transaction_pool.insert(transaction_2, 10);
        transaction_pool.insert(transaction_3, 10);
        assert_eq!(transaction_pool.vt_len(), 3);

        let mut unspent_outputs_pool = UnspentOutputsPool::default();
        let output1 = ValueTransferOutput {
            time_lock: 0,
            pkh: MY_PKH_1.parse().unwrap(),
            value: 1_000_000,
        };
        unspent_outputs_pool.insert(output1_pointer, output1, 0);
        assert!(unspent_outputs_pool.contains_key(&output1_pointer));

        let mut dr_pool = DataRequestPool::default();

        // Fields required to mine a block
        let block_beacon = CheckpointBeacon::default();
        let block_proof = BlockEligibilityClaim::default();
        let block_number = 1;
        let collateral_minimum = 1_000_000_000;

        let active_wips: ActiveWips = ActiveWips {
            active_wips: HashMap::default(),
            block_epoch: 0,
        };

        // Build block with

        let (block_header, txns) = build_block(
            (&mut transaction_pool, &unspent_outputs_pool, &mut dr_pool),
            max_vt_weight,
            max_dr_weight,
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
            INITIAL_BLOCK_REWARD,
            CHECKPOINT_ZERO_TIMESTAMP,
            HALVING_PERIOD,
            0,
            &active_wips,
            None,
            &StakesTracker::default(),
            &ConsensusConstantsWit2::default(),
        );
        let block = Block::new(block_header, KeyedSignature::default(), txns);

        // Check if block contains only 2 transactions (Mint Transaction + 1 included transaction)
        assert_eq!(block.txns.len(), 2);

        // Check that exist Mint Transaction
        assert!(!block.txns.mint.is_empty());

        // Check that the included transaction is the only one that fits the `max_block_weight`
        assert_eq!(block.txns.value_transfer_txns[0], vt_tx1);
    }

    #[test]
    fn build_block_with_vt_transactions_prioritizied() {
        let output1_pointer: OutputPointer = MILLION_TX_OUTPUT.parse().unwrap();
        let input = vec![Input::new(output1_pointer)];
        let vto1 = ValueTransferOutput {
            value: 1,
            ..Default::default()
        };
        let vto2 = ValueTransferOutput {
            value: 2,
            ..Default::default()
        };
        let vto3 = ValueTransferOutput {
            value: 3,
            ..Default::default()
        };
        let two_outputs1 = vec![vto1.clone(), vto2.clone()];
        let two_outputs2 = vec![vto1, vto3.clone()];
        let two_outputs3 = vec![vto2, vto3];

        let vt_body1 = VTTransactionBody::new(input.clone(), two_outputs1);
        let vt_body2 = VTTransactionBody::new(input.clone(), two_outputs2);
        let vt_body3 = VTTransactionBody::new(input, two_outputs3);

        // Build sample transactions
        let vt_tx1 = VTTransaction::new(vt_body1, vec![]);
        let vt_tx2 = VTTransaction::new(vt_body2, vec![]);
        let vt_tx3 = VTTransaction::new(vt_body3, vec![]);
        assert_eq!(vt_tx1.weight(), vt_tx2.weight());
        assert_eq!(vt_tx1.weight(), vt_tx3.weight());

        let transaction_1 = Transaction::ValueTransfer(vt_tx1);
        let transaction_2 = Transaction::ValueTransfer(vt_tx2.clone());
        let transaction_3 = Transaction::ValueTransfer(vt_tx3);

        // Set `max_vt_weight` to fit only 1 transaction weight
        let max_vt_weight = vt_tx2.weight();
        let max_dr_weight = 0;

        // Insert transactions into `transactions_pool`
        let mut transaction_pool = TransactionsPool::default();
        transaction_pool.insert(transaction_1, 1);
        transaction_pool.insert(transaction_2, 25);
        transaction_pool.insert(transaction_3, 10);
        assert_eq!(transaction_pool.vt_len(), 3);

        let mut unspent_outputs_pool = UnspentOutputsPool::default();
        let output1 = ValueTransferOutput {
            time_lock: 0,
            pkh: MY_PKH_1.parse().unwrap(),
            value: 1_000_000,
        };
        unspent_outputs_pool.insert(output1_pointer, output1, 0);
        assert!(unspent_outputs_pool.contains_key(&output1_pointer));

        let mut dr_pool = DataRequestPool::default();

        // Fields required to mine a block
        let block_beacon = CheckpointBeacon::default();
        let block_proof = BlockEligibilityClaim::default();
        let block_number = 1;
        let collateral_minimum = 1_000_000_000;

        let active_wips: ActiveWips = ActiveWips {
            active_wips: HashMap::default(),
            block_epoch: 0,
        };

        // Build block with

        let (block_header, txns) = build_block(
            (&mut transaction_pool, &unspent_outputs_pool, &mut dr_pool),
            max_vt_weight,
            max_dr_weight,
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
            INITIAL_BLOCK_REWARD,
            CHECKPOINT_ZERO_TIMESTAMP,
            HALVING_PERIOD,
            0,
            &active_wips,
            None,
            &StakesTracker::default(),
            &ConsensusConstantsWit2::default(),
        );
        let block = Block::new(block_header, KeyedSignature::default(), txns);

        // Check if block contains only 2 transactions (Mint Transaction + 1 included transaction)
        assert_eq!(block.txns.len(), 2);

        // Check that exist Mint Transaction
        assert!(!block.txns.mint.is_empty());

        // Check that the included transaction is the only one that fits the `max_block_weight`
        assert_eq!(block.txns.value_transfer_txns[0], vt_tx2);
    }

    fn example_request() -> RADRequest {
        RADRequest {
            retrieve: vec![RADRetrieve {
                url: "http://127.0.0.1:8000".to_string(),
                script: vec![128],
                ..Default::default()
            }],
            aggregate: RADAggregate {
                filters: vec![],
                reducer: 3,
            },
            tally: RADTally {
                filters: vec![],
                reducer: 3,
            },
            time_lock: 0,
        }
    }

    #[test]
    fn build_block_with_dr_transactions() {
        let output1_pointer: OutputPointer = MILLION_TX_OUTPUT.parse().unwrap();
        let input = vec![Input::new(output1_pointer)];
        let dr1 = DataRequestOutput {
            witnesses: 1,
            commit_and_reveal_fee: 1,
            witness_reward: 1,
            min_consensus_percentage: 51,
            data_request: example_request(),
            collateral: 1_000_000_000,
        };
        let mut dr2 = dr1.clone();
        dr2.witnesses = 2;
        let mut dr3 = dr1.clone();
        dr3.witnesses = 3;

        let dr_body_one_output1 = DRTransactionBody::new(input.clone(), dr1, vec![]);
        let dr_body_one_output2 = DRTransactionBody::new(input.clone(), dr2, vec![]);
        let dr_body_one_output3 = DRTransactionBody::new(input, dr3, vec![]);

        // Build sample transactions
        let dr_tx1 = DRTransaction::new(dr_body_one_output1, vec![]);
        let dr_tx2 = DRTransaction::new(dr_body_one_output2, vec![]);
        let dr_tx3 = DRTransaction::new(dr_body_one_output3, vec![]);

        let transaction_1 = Transaction::DataRequest(dr_tx1.clone());
        let transaction_2 = Transaction::DataRequest(dr_tx2);
        let transaction_3 = Transaction::DataRequest(dr_tx3);

        // Set `max_vt_weight` to fit only `transaction_1` weight
        let max_vt_weight = 0;
        let max_dr_weight = dr_tx1.weight();

        // Insert transactions into `transactions_pool`
        let mut transaction_pool = TransactionsPool::default();
        transaction_pool.insert(transaction_1, 2);
        transaction_pool.insert(transaction_2, 25);
        transaction_pool.insert(transaction_3, 10);
        assert_eq!(transaction_pool.dr_len(), 3);

        let mut unspent_outputs_pool = UnspentOutputsPool::default();
        let output1 = ValueTransferOutput {
            time_lock: 0,
            pkh: MY_PKH_1.parse().unwrap(),
            value: 1_000_000,
        };
        unspent_outputs_pool.insert(output1_pointer, output1, 0);
        assert!(unspent_outputs_pool.contains_key(&output1_pointer));

        let mut dr_pool = DataRequestPool::default();

        // Fields required to mine a block
        let block_beacon = CheckpointBeacon::default();
        let block_proof = BlockEligibilityClaim::default();
        let block_number = 1;
        let collateral_minimum = 1_000_000_000;

        let active_wips: ActiveWips = ActiveWips {
            active_wips: HashMap::default(),
            block_epoch: 0,
        };

        // Build block with

        let (block_header, txns) = build_block(
            (&mut transaction_pool, &unspent_outputs_pool, &mut dr_pool),
            max_vt_weight,
            max_dr_weight,
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
            INITIAL_BLOCK_REWARD,
            CHECKPOINT_ZERO_TIMESTAMP,
            HALVING_PERIOD,
            0,
            &active_wips,
            None,
            &StakesTracker::default(),
            &ConsensusConstantsWit2::default(),
        );
        let block = Block::new(block_header, KeyedSignature::default(), txns);

        // Check if block contains only 2 transactions (Mint Transaction + 1 included transaction)
        assert_eq!(block.txns.len(), 2);

        // Check that exist Mint Transaction
        assert!(!block.txns.mint.is_empty());

        // Check that the included transaction is the only one that fits the `max_block_weight`
        assert_eq!(block.txns.data_request_txns[0], dr_tx1);
    }

    #[test]
    fn build_block_with_dr_transactions_prioritizied() {
        let output1_pointer: OutputPointer = MILLION_TX_OUTPUT.parse().unwrap();
        let input = vec![Input::new(output1_pointer)];
        let dr1 = DataRequestOutput {
            witnesses: 1,
            commit_and_reveal_fee: 1,
            witness_reward: 1,
            min_consensus_percentage: 51,
            data_request: example_request(),
            collateral: 1_000_000_000,
        };
        let mut dr2 = dr1.clone();
        dr2.commit_and_reveal_fee = 2;
        let mut dr3 = dr1.clone();
        dr3.commit_and_reveal_fee = 3;

        let dr_body_one_output1 = DRTransactionBody::new(input.clone(), dr1, vec![]);
        let dr_body_one_output2 = DRTransactionBody::new(input.clone(), dr2, vec![]);
        let dr_body_one_output3 = DRTransactionBody::new(input, dr3, vec![]);

        // Build sample transactions
        let dr_tx1 = DRTransaction::new(dr_body_one_output1, vec![]);
        let dr_tx2 = DRTransaction::new(dr_body_one_output2, vec![]);
        let dr_tx3 = DRTransaction::new(dr_body_one_output3, vec![]);
        assert_eq!(dr_tx1.weight(), dr_tx2.weight());
        assert_eq!(dr_tx1.weight(), dr_tx3.weight());

        let transaction_1 = Transaction::DataRequest(dr_tx1);
        let transaction_2 = Transaction::DataRequest(dr_tx2.clone());
        let transaction_3 = Transaction::DataRequest(dr_tx3);

        // Set `max_vt_weight` to fit only `transaction_1` weight
        let max_vt_weight = 0;
        let max_dr_weight = dr_tx2.weight();

        // Insert transactions into `transactions_pool`
        let mut transaction_pool = TransactionsPool::default();
        transaction_pool.insert(transaction_1, 2);
        transaction_pool.insert(transaction_2, 25);
        transaction_pool.insert(transaction_3, 10);
        assert_eq!(transaction_pool.dr_len(), 3);

        let mut unspent_outputs_pool = UnspentOutputsPool::default();
        let output1 = ValueTransferOutput {
            time_lock: 0,
            pkh: MY_PKH_1.parse().unwrap(),
            value: 1_000_000,
        };
        unspent_outputs_pool.insert(output1_pointer, output1, 0);
        assert!(unspent_outputs_pool.contains_key(&output1_pointer));

        let mut dr_pool = DataRequestPool::default();

        // Fields required to mine a block
        let block_beacon = CheckpointBeacon::default();
        let block_proof = BlockEligibilityClaim::default();
        let block_number = 1;
        let collateral_minimum = 1_000_000_000;

        let active_wips: ActiveWips = ActiveWips {
            active_wips: HashMap::default(),
            block_epoch: 0,
        };

        // Build block with

        let (block_header, txns) = build_block(
            (&mut transaction_pool, &unspent_outputs_pool, &mut dr_pool),
            max_vt_weight,
            max_dr_weight,
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
            INITIAL_BLOCK_REWARD,
            CHECKPOINT_ZERO_TIMESTAMP,
            HALVING_PERIOD,
            0,
            &active_wips,
            None,
            &StakesTracker::default(),
            &ConsensusConstantsWit2::default(),
        );
        let block = Block::new(block_header, KeyedSignature::default(), txns);

        // Check if block contains only 2 transactions (Mint Transaction + 1 included transaction)
        assert_eq!(block.txns.len(), 2);

        // Check that exist Mint Transaction
        assert!(!block.txns.mint.is_empty());

        // Check that the included transaction is the only one that fits the `max_block_weight`
        assert_eq!(block.txns.data_request_txns[0], dr_tx2);
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

        let public_key = Secp256k1_PublicKey::from_secret_key_global(&sk);

        let data = [
            0xca, 0x18, 0xf5, 0xad, 0xc2, 0x18, 0x45, 0x25, 0x0e, 0x88, 0x14, 0x18, 0x1f, 0xf7,
            0x8c, 0x5b, 0x83, 0x68, 0x5c, 0x0c, 0xda, 0x55, 0x62, 0xda, 0x30, 0xc1, 0x95, 0x8d,
            0x84, 0x9e, 0xc6, 0xb9,
        ];

        let signature = sign(sk, &data).unwrap();
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

    #[test]
    fn encode_error_can_be_encoded() {
        let _reveal_bytes: Vec<u8> =
            RadonTypes::RadonError(RadonError::try_from(RadError::EncodeReveal).unwrap())
                .encode()
                .unwrap();
    }
}
