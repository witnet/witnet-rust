use std::{
    collections::{BTreeMap, HashMap, HashSet, VecDeque},
    convert::{TryFrom, TryInto},
    future,
    net::SocketAddr,
    time::Duration,
};

use actix::{ActorFutureExt, WrapFuture, prelude::*};
use futures::future::Either;

use itertools::Itertools;
use witnet_data_structures::{
    chain::{
        Block, ChainState, CheckpointBeacon, ConsensusConstantsWit2, DataRequestInfo, Epoch, Hash,
        Hashable, NodeStats, PublicKeyHash, SuperBlockVote, SupplyInfo, SupplyInfo2,
        ValueTransferOutput, tapi::ActiveWips,
    },
    error::{ChainInfoError, TransactionError::DataRequestNotFound},
    get_protocol_info, get_protocol_version, get_protocol_version_activation_epoch,
    proto::versioning::ProtocolVersion,
    refresh_protocol_version,
    staking::{
        prelude::{Power, StakeKey},
        stakes::QueryStakesKey,
    },
    transaction::{
        DRTransaction, StakeTransaction, Transaction, UnstakeTransaction, VTTransaction,
    },
    transaction_factory::{self, NodeBalance, NodeBalance2},
    types::LastBeacon,
    utxo_pool::{UtxoDiff, UtxoInfo, get_utxo_info},
    wit::Wit,
};
use witnet_util::timestamp::get_timestamp;
use witnet_validations::validations::{
    block_reward, total_block_reward, validate_rad_request, validate_stake_transaction,
    validate_unstake_transaction,
};

use crate::{
    actors::{
        chain_manager::{BlockCandidate, handlers::BlockBatches::*},
        messages::{
            AddBlocks, AddCandidates, AddCommitReveal, AddSuperBlock, AddSuperBlockVote,
            AddTransaction, Broadcast, BuildDrt, BuildStake, BuildUnstake, BuildVtt,
            EpochNotification, EstimatePriority, GetBalance, GetBalance2, GetBalanceTarget,
            GetBlocksEpochRange, GetDataRequestInfo, GetHighestCheckpointBeacon,
            GetMemoryTransaction, GetMempool, GetMempoolResult, GetNodeStats, GetProtocolInfo,
            GetReputation, GetReputationResult, GetSignalingInfo, GetState, GetSuperBlockVotes,
            GetSupplyInfo, GetSupplyInfo2, GetUtxoInfo, IsConfirmedBlock, PeersBeacons,
            QueryStakes, QueryStakesOrderByOptions, QueryStakingPowers, ReputationStats, Rewind,
            SendLastBeacon, SessionUnitResult, SetLastBeacon, SetPeersLimits, SignalingInfo,
            SnapshotExport, SnapshotImport, TryMineBlock, try_do_magic_into_pkh,
        },
        sessions_manager::SessionsManager,
    },
    config_mngr, signature_mngr, storage_mngr,
    utils::mode_consensus,
};

use super::{ChainManager, ChainManagerError, StateMachine, SyncTarget};

pub const SYNCED_BANNER: &str = r"
███████╗██╗   ██╗███╗   ██╗ ██████╗███████╗██████╗ ██╗
██╔════╝╚██╗ ██╔╝████╗  ██║██╔════╝██╔════╝██╔══██╗██║
███████╗ ╚████╔╝ ██╔██╗ ██║██║     █████╗  ██║  ██║██║
╚════██║  ╚██╔╝  ██║╚██╗██║██║     ██╔══╝  ██║  ██║╚═╝
███████║   ██║   ██║ ╚████║╚██████╗███████╗██████╔╝██╗
╚══════╝   ╚═╝   ╚═╝  ╚═══╝ ╚═════╝╚══════╝╚═════╝ ╚═╝
╔════════════════════════════════════════════════════╗
║ This node has finished bootstrapping and is now    ║
║ working at full steam in validating transactions,  ║
║ proposing blocks and resolving data requests.      ║
╟────────────────────────────────────────────────────╢
║ You can now sit back and enjoy Witnet.             ║
╟────────────────────────────────────────────────────╢
║ Wait... Are you still there? You want more fun?    ║
║ Go to https://docs.witnet.io/node-operators/cli/   ║
║ to learn how to monitor the progress of your node  ║
║ (balance, reputation, proposed blocks, etc.)       ║
╚════════════════════════════════════════════════════╝";

////////////////////////////////////////////////////////////////////////////////////////
// ACTOR MESSAGE HANDLERS
////////////////////////////////////////////////////////////////////////////////////////

/// Payload for the notification for all epochs
#[derive(Clone, Debug)]
pub struct EveryEpochPayload;

/// Handler for EpochNotification<EveryEpochPayload>
impl Handler<EpochNotification<EveryEpochPayload>> for ChainManager {
    type Result = ();

    #[allow(clippy::cognitive_complexity)]
    fn handle(&mut self, msg: EpochNotification<EveryEpochPayload>, ctx: &mut Context<Self>) {
        log::debug!("Periodic epoch notification received {:?}", msg.checkpoint);
        let current_timestamp = get_timestamp();
        log::debug!(
            "Timestamp diff: {}, Epoch timestamp: {}. Current timestamp: {}",
            current_timestamp - msg.timestamp,
            msg.timestamp,
            current_timestamp
        );

        let last_checked_epoch = self.current_epoch;
        let current_epoch = msg.checkpoint;
        self.current_epoch = Some(current_epoch);

        log::debug!(
            "EpochNotification received while StateMachine is in state {:?}",
            self.sm_state
        );
        let chain_beacon = self.get_chain_beacon();
        log::debug!(
            "Chain state ---> checkpoint: {}, hash_prev_block: {}",
            chain_beacon.checkpoint,
            chain_beacon.hash_prev_block
        );

        // Clear pending transactions HashSet
        self.seen_transactions.clear();

        // Handle case consensus not achieved
        if !self.peers_beacons_received {
            log::warn!("No beacon messages received from peers. Moving to WaitingConsensus state");
            self.update_state_machine(StateMachine::WaitingConsensus, ctx);
            // Clear candidates
            self.candidates.clear();
            self.seen_candidates.clear();
        }

        if let Some(last_checked_epoch) = last_checked_epoch {
            if msg.checkpoint - last_checked_epoch != 1 {
                log::warn!("Missed epoch notification {}.", last_checked_epoch + 1);
                self.update_state_machine(StateMachine::WaitingConsensus, ctx);
            }
        }

        self.peers_beacons_received = false;
        // The best candidate must be cleared on every epoch
        let best_candidate = self.best_candidate.take();

        // Update the global protocol version state if necessary
        let expected_protocol_version = get_protocol_version(self.current_epoch);
        let current_protocol_version = get_protocol_version(None);
        if expected_protocol_version != current_protocol_version {
            refresh_protocol_version(current_epoch);
        }

        match self.sm_state {
            StateMachine::WaitingConsensus => {
                if let Some(chain_info) = &self.chain_state.chain_info {
                    // Send last beacon because otherwise the network cannot bootstrap
                    let sessions_manager = SessionsManager::from_registry();
                    let last_beacon = LastBeacon {
                        highest_block_checkpoint: chain_info.highest_block_checkpoint,
                        highest_superblock_checkpoint: self.get_superblock_beacon(),
                    };
                    sessions_manager.do_send(SetLastBeacon {
                        beacon: last_beacon.clone(),
                    });
                    sessions_manager.do_send(Broadcast {
                        command: SendLastBeacon { last_beacon },
                        only_inbound: true,
                    });
                }
            }
            StateMachine::Synchronizing => {}
            StateMachine::AlmostSynced | StateMachine::Synced => {
                match self.chain_state {
                    ChainState {
                        reputation_engine: Some(_),
                        ..
                    } => {
                        if self.epoch_constants.is_none() || self.vrf_ctx.is_none() {
                            log::error!("{}", ChainManagerError::ChainNotReady);
                            return;
                        }

                        // Consolidate the best candidate
                        if let Some(BlockCandidate {
                            block,
                            utxo_diff,
                            reputation: _,
                            vrf_proof: _,
                            priorities,
                        }) = best_candidate
                        {
                            // Persist block and update ChainState
                            self.consolidate_block(ctx, block, utxo_diff, priorities, false);
                        }

                        // Send last beacon on block consolidation
                        let sessions_manager = SessionsManager::from_registry();
                        let beacon = self.get_chain_beacon();
                        let superblock_beacon = self.get_superblock_beacon();
                        let last_beacon = LastBeacon {
                            highest_block_checkpoint: beacon,
                            highest_superblock_checkpoint: superblock_beacon,
                        };
                        sessions_manager.do_send(SetLastBeacon {
                            beacon: last_beacon.clone(),
                        });
                        sessions_manager.do_send(Broadcast {
                            command: SendLastBeacon { last_beacon },
                            only_inbound: true,
                        });

                        // TODO: Review time since commits are clear and new ones are received before to mining
                        // Remove commits because they expire every epoch
                        self.transactions_pool.clear_commits();

                        // Mining
                        if self.mining_enabled && self.sm_state == StateMachine::Synced {
                            // Block mining is now triggered by SessionsManager on peers beacon timeout
                            // Data request mining MUST finish BEFORE the block has been mined!!!!
                            // The transactions must be included into this block, both the transactions from
                            // our node and the transactions from other nodes
                            if let Err(e) = self.try_mine_data_request(ctx) {
                                match e {
                                    // Lack of eligibility is logged as debug
                                    e @ ChainManagerError::NotEligible => {
                                        log::debug!("{e}");
                                    }
                                    // Any other errors are logged as warning (considering that this is a best-effort
                                    // method)
                                    e => {
                                        log::warn!("{e}");
                                    }
                                }
                            }
                        }

                        // Clear candidates
                        self.candidates.clear();
                        self.seen_candidates.clear();

                        // Periodically revalidate pending stake transactions since they can become invalid
                        if get_protocol_version(Some(current_epoch)) >= ProtocolVersion::V1_8
                            && current_epoch % 10 == 0
                        {
                            let utxo_diff = UtxoDiff::new(
                                &self.chain_state.unspent_outputs_pool,
                                self.chain_state.block_number(),
                            );
                            let min_stake = self
                                .consensus_constants_wit2
                                .get_validator_min_stake_nanowits(current_epoch);
                            let max_stake = self
                                .consensus_constants_wit2
                                .get_validator_max_stake_nanowits(current_epoch);

                            let mut invalid_stake_transactions = Vec::<Hash>::new();
                            for st_tx in self.transactions_pool.st_iter() {
                                if let Err(e) = validate_stake_transaction(
                                    st_tx,
                                    &utxo_diff,
                                    current_epoch,
                                    self.epoch_constants.unwrap(),
                                    &mut vec![],
                                    &self.chain_state.stakes,
                                    min_stake,
                                    max_stake,
                                ) {
                                    log::debug!(
                                        "Removing stake transaction {} as it became invalid: {}",
                                        st_tx.hash(),
                                        e
                                    );
                                    invalid_stake_transactions.push(st_tx.hash());
                                    continue;
                                }
                            }

                            self.transactions_pool
                                .remove_invalid_stake_transactions(invalid_stake_transactions);
                        }

                        // Periodically revalidate pending unstake transactions since they can become invalid
                        if get_protocol_version(Some(current_epoch)) >= ProtocolVersion::V2_0
                            && current_epoch % 10 == 0
                        {
                            let min_stake = self
                                .consensus_constants_wit2
                                .get_validator_min_stake_nanowits(current_epoch);
                            let unstake_delay = self
                                .consensus_constants_wit2
                                .get_unstaking_delay_seconds(current_epoch);

                            let mut invalid_unstake_transactions = Vec::<Hash>::new();
                            for ut_tx in self.transactions_pool.ut_iter() {
                                if let Err(e) = validate_unstake_transaction(
                                    ut_tx,
                                    current_epoch,
                                    &self.chain_state.stakes,
                                    min_stake,
                                    unstake_delay,
                                ) {
                                    log::debug!(
                                        "Removing unstake transaction {} as it became invalid: {}",
                                        ut_tx.hash(),
                                        e
                                    );
                                    invalid_unstake_transactions.push(ut_tx.hash());
                                    continue;
                                }
                            }

                            self.transactions_pool
                                .remove_invalid_unstake_transactions(invalid_unstake_transactions);
                        }

                        log::debug!(
                            "Transactions pool size: {} value transfer, {} data request, {} stake, {} unstake",
                            self.transactions_pool.vt_len(),
                            self.transactions_pool.dr_len(),
                            self.transactions_pool.st_len(),
                            self.transactions_pool.ut_len()
                        );
                    }

                    _ => {
                        log::error!("No ChainInfo loaded in ChainManager");
                    }
                }
            }
        }

        // After block consolidation, reveals that arrived in an incorrect moment
        // are processed now
        for reveal in self.temp_reveals.drain(..) {
            ctx.notify(AddTransaction {
                transaction: Transaction::Reveal(reveal),
                broadcast_flag: true,
            });
        }

        // Include value transfers and data requests that were recovered from a rewind
        if !self.temp_vts_and_drs.is_empty() && self.sm_state == StateMachine::Synced {
            let max_txs = std::cmp::min(
                self.max_reinserted_transactions,
                self.temp_vts_and_drs.len(),
            );
            for transaction in self.temp_vts_and_drs.drain(..max_txs) {
                ctx.notify(AddTransaction {
                    transaction,
                    broadcast_flag: false,
                });
            }
        }

        self.peers_beacons_received = false;
    }
}

/// Handler for GetHighestBlockCheckpoint message
impl Handler<GetHighestCheckpointBeacon> for ChainManager {
    type Result = Result<CheckpointBeacon, anyhow::Error>;

    fn handle(
        &mut self,
        _msg: GetHighestCheckpointBeacon,
        _ctx: &mut Context<Self>,
    ) -> Self::Result {
        if let Some(chain_info) = &self.chain_state.chain_info {
            Ok(chain_info.highest_block_checkpoint)
        } else {
            log::error!("No ChainInfo loaded in ChainManager");

            Err(ChainInfoError::ChainInfoNotFound.into())
        }
    }
}

/// Handler for GetSuperBlockVotes message
impl Handler<GetSuperBlockVotes> for ChainManager {
    type Result = Result<HashSet<SuperBlockVote>, anyhow::Error>;

    fn handle(&mut self, _msg: GetSuperBlockVotes, _ctx: &mut Context<Self>) -> Self::Result {
        Ok(self
            .chain_state
            .superblock_state
            .get_current_superblock_votes())
    }
}

/// Handler for GetNodeStats message
impl Handler<GetNodeStats> for ChainManager {
    type Result = Result<NodeStats, anyhow::Error>;

    fn handle(&mut self, _msg: GetNodeStats, _ctx: &mut Context<Self>) -> Self::Result {
        Ok(self.chain_state.node_stats.clone())
    }
}

/// Handler for AddBlocks message
impl Handler<AddBlocks> for ChainManager {
    type Result = ResponseActFuture<Self, ()>;

    fn handle(&mut self, msg: AddBlocks, _ctx: &mut Context<Self>) -> ResponseActFuture<Self, ()> {
        let fut = actix::fut::ok(()).into_actor(self).and_then(|(), act, ctx| -> ResponseActFuture<Self, Result<(), ()>> {
            log::debug!(
                "AddBlocks received while StateMachine is in state {:?}",
                act.sm_state
            );

            let consensus_constants = act.consensus_constants();
            let sender = msg.sender;

            match act.sm_state {
                StateMachine::WaitingConsensus | StateMachine::AlmostSynced => {
                    // In WaitingConsensus state, only allow AddBlocks when the argument is
                    // the genesis block
                    if msg.blocks.len() == 1 && msg.blocks[0].hash() == consensus_constants.genesis_hash
                    {
                        let block = msg.blocks.into_iter().next().unwrap();
                        match act.process_requested_block(ctx, block, false) {
                            Ok(()) => {
                                log::debug!("Successfully consolidated genesis block");

                                // Set last beacon because otherwise the network cannot bootstrap
                                let sessions_manager = SessionsManager::from_registry();
                                let last_beacon = LastBeacon {
                                    highest_block_checkpoint: act.get_chain_beacon(),
                                    highest_superblock_checkpoint: act.get_superblock_beacon(),
                                };
                                sessions_manager.do_send(SetLastBeacon {
                                    beacon: last_beacon,
                                });
                            }
                            Err(e) => log::error!("Failed to consolidate genesis block: {e}"),
                        }
                    } else {
                        log::debug!("Unhandled AddBlocks message");
                    }
                }
                StateMachine::Synced => {
                    log::debug!("Unhandled AddBlocks message");
                }
                StateMachine::Synchronizing => {
                    if act.sync_target.is_none() {
                        log::warn!("Target Beacon is None");
                        return Box::pin(actix::fut::err(()));
                    }

                    if let Some(block) = msg.blocks.first() {
                        let chain_tip = act.get_chain_beacon();
                        if block.block_header.beacon.checkpoint > chain_tip.checkpoint
                            && block.block_header.beacon.hash_prev_block != chain_tip.hash_prev_block
                        {
                            // During synchronization, if you receive a block that doesn't match with
                            // your chain tip, you could be forked,so a good practice it is restore
                            // from storage
                            log::warn!("Your chain is probably forked");

                            // Clean all outbounds to avoid possible forked outbounds
                            act.drop_all_outbounds();

                            act.update_state_machine(StateMachine::WaitingConsensus, ctx);
                            act.initialize_from_storage(ctx);
                            log::info!("Restored chain state from storage");

                            return Box::pin(actix::fut::err(()));
                        }
                    } else {
                        log::debug!("Received an empty AddBlocks message");
                        act.update_state_machine(StateMachine::WaitingConsensus, ctx);
                        return Box::pin(actix::fut::err(()));
                    }

                    let sync_target = act
                        .sync_target
                        .expect("The sync target should be defined for synchronizing");

                    let sync_superblock =
                        act.sync_superblock
                            .as_ref()
                            .and_then(|(hash, superblock)| {
                                if hash == &sync_target.superblock.hash_prev_block {
                                    Some(superblock.clone())
                                } else {
                                    None
                                }
                            });

                    if sync_superblock.is_none() && sync_target.superblock.checkpoint != 0 {
                        log::debug!("Received blocks before superblock");
                        // Received the `AddBlocks` message before the `AddSuperBlock` message.
                        // We cannot finish the synchronization without the sync_superblock, so in that
                        // case, ask for the sync_superblock again, and try to handle the `AddBlocks`
                        // message later
                        act.request_sync_target_superblock(ctx, sync_target.superblock);
                        ctx.notify_later(msg, Duration::from_secs(6));
                        return Box::pin(actix::fut::err(()));
                    }

                    let superblock_period = u32::from(consensus_constants.superblock_period);

                    // Split received blocks into batches according to 3 different cases:
                    // 1. TargetNotReached: superblock target not reachable with the received block batch.
                    // 2. SyncWithoutCandidate: target superblock needs to be consolidated, but no
                    // candidate superblock should be built.
                    // 3. SyncWithCandidate: target superblock needs to be consolidated, and a candidate
                    // superblock should be built.
                    let block_batches = split_blocks_batch_at_target(
                        |b| b.block_header.beacon.checkpoint,
                        msg.blocks,
                        act.current_epoch.unwrap(),
                        &sync_target,
                        superblock_period,
                    );

                    match block_batches {
                        // TargetNotReached:
                        // 1. process blocks (not yet ready for consolidation)
                        // 2. requests block batch -> revert to WaitingConsensus
                        Ok(TargetNotReached(blocks)) => {
                            let (batch_succeeded, num_processed_blocks) =
                                act.process_first_batch(ctx, &sync_target, &blocks);
                            if !batch_succeeded {
                                act.drop_all_outbounds_and_ice_sender(sender);

                                return Box::pin(actix::fut::err(()));
                            }

                            // Persist blocks batch when target not reached
                            act.persist_blocks_batch(ctx, blocks);
                            let to_be_stored =
                                act.chain_state.data_request_pool.finished_data_requests();
                            act.persist_data_requests(ctx, to_be_stored);

                            log::debug!("TargetNotReached: superblock target #{} not reached, requesting more blocks. ({} processed blocks)",
                            sync_target.superblock.checkpoint, num_processed_blocks);
                            act.request_blocks_batch(ctx);

                            // Copy current chain state into previous chain state, and persist it
                            return act.persist_chain_state(None);
                        }
                        // SyncWithoutCandidate:
                        // 1. process blocks
                        // 2. construct consolidated superblock (if needed)
                        // 3. handle remaining blocks
                        Ok(SyncWithoutCandidate(consolidate_blocks, remainig_blocks)) => {
                            let (batch_succeeded, num_processed_blocks) =
                                act.process_first_batch(ctx, &sync_target, &consolidate_blocks);
                            if !batch_succeeded {
                                act.drop_all_outbounds_and_ice_sender(sender);

                                return Box::pin(actix::fut::err(()));
                            }
                            log_sync_progress(
                                &sync_target,
                                &consolidate_blocks,
                                num_processed_blocks,
                                "SyncWithoutCandidate(consolidation)",
                            );

                            if let Some(consolidate_epoch) = act.superblock_consolidation_is_needed(&sync_target, superblock_period) {
                                // We need to persist blocks in order to be able to construct the
                                // superblock
                                act.persist_blocks_batch(ctx, consolidate_blocks);
                                let to_be_stored =
                                    act.chain_state.data_request_pool.finished_data_requests();
                                act.persist_data_requests(ctx, to_be_stored);
                                // Create superblocks while synchronizing but do not broadcast them
                                // This is needed to ensure that we can validate the received superblocks later on
                                log::debug!("Will construct superblock during synchronization. Superblock index: {} Epoch {}", sync_target.superblock.checkpoint, consolidate_epoch);
                                Either::Left(
                                    act.try_consolidate_superblock(consolidate_epoch, sync_target, sync_superblock)
                                )
                            } else {
                                // No need to construct a superblock again,
                                Either::Right(actix::fut::ok(()))
                            }
                                .and_then(move |(), act, ctx| {
                                    act.update_state_machine(StateMachine::WaitingConsensus, ctx);
                                    // Process remaining blocks
                                    let (batch_succeeded, num_processed_blocks) = act.process_blocks_batch(ctx, &sync_target, &remainig_blocks);
                                    if !batch_succeeded {
                                        act.drop_all_outbounds_and_ice_sender(sender);

                                        return actix::fut::err(());
                                    }
                                    log_sync_progress(&sync_target, &remainig_blocks, num_processed_blocks, "SyncWithoutCandidate(remaining)");

                                    actix::fut::ok(())
                                })
                                .map(|_res: Result<(), ()>, _act, _ctx| ())
                                .wait(ctx);
                        }
                        // SyncWithCandidate:
                        // 1. process blocks
                        // 2. construct consolidated superblock (if needed)
                        // 3. build and vote new candidate superblock
                        // 4. process remaining blocks
                        Ok(SyncWithCandidate(
                               consolidate_blocks,
                               candidate_blocks,
                               remaining_blocks,
                           )) => {
                            let (batch_succeeded, num_processed_blocks) =
                                act.process_first_batch(ctx, &sync_target, &consolidate_blocks);
                            if !batch_succeeded {
                                act.drop_all_outbounds_and_ice_sender(sender);

                                return Box::pin(actix::fut::err(()));
                            }
                            log_sync_progress(
                                &sync_target,
                                &consolidate_blocks,
                                num_processed_blocks,
                                "SyncWithCandidate(consolidation)",
                            );

                            if let Some(consolidate_superblock_epoch) = act.superblock_consolidation_is_needed(&sync_target, superblock_period) {
                                // We need to persist blocks in order to be able to construct the
                                // superblock
                                act.persist_blocks_batch(ctx, consolidate_blocks);
                                let to_be_stored =
                                    act.chain_state.data_request_pool.finished_data_requests();
                                act.persist_data_requests(ctx, to_be_stored);
                                // Create superblocks while synchronizing but do not broadcast them
                                // This is needed to ensure that we can validate the received superblocks later on
                                log::debug!("Will construct superblock during synchronization. Superblock index: {} Epoch {}", sync_target.superblock.checkpoint, consolidate_superblock_epoch);
                                Either::Left(
                                    act.try_consolidate_superblock(consolidate_superblock_epoch, sync_target, sync_superblock)
                                )
                            } else {
                                // No need to construct a superblock again,
                                Either::Right(actix::fut::ok(()))
                            }
                                .and_then({
                                    move |(), act, ctx| {
                                        // Process remaining blocks
                                        let (batch_succeeded, num_processed_blocks) = act.process_blocks_batch(ctx, &sync_target, &candidate_blocks);
                                        if !batch_succeeded {
                                            act.drop_all_outbounds_and_ice_sender(sender);

                                            act.update_state_machine(StateMachine::WaitingConsensus, ctx);

                                            return actix::fut::err(());
                                        }
                                        log_sync_progress(&sync_target, &candidate_blocks, num_processed_blocks, "SyncWithCandidate(candidate)");

                                        // Update ARS if there were no blocks right before the epoch during
                                        // which we should construct the target superblock
                                        let candidate_superblock_checkpoint = act.current_epoch.unwrap() / superblock_period;

                                        // We need to persist blocks in order to be able to construct the
                                        // superblock
                                        act.persist_blocks_batch(ctx, candidate_blocks);
                                        let to_be_stored =
                                            act.chain_state.data_request_pool.finished_data_requests();
                                        act.persist_data_requests(ctx, to_be_stored);

                                        log::info!("Block sync target achieved");
                                        // Target achieved, go back to state 1
                                        act.update_state_machine(StateMachine::WaitingConsensus, ctx);

                                        // We must construct the second superblock in order to be able
                                        // to validate the votes for this superblock later
                                        log::debug!("Will construct the second superblock during synchronization. Superblock index: {} Epoch {}", sync_target.superblock.checkpoint + 1, candidate_superblock_checkpoint * superblock_period);

                                        actix::fut::ok(candidate_superblock_checkpoint)
                                    }
                                })
                                .and_then(move |candidate_superblock_checkpoint, act, _ctx| {
                                    if let Some(candidate_superblock_epoch) = act.superblock_candidate_is_needed(candidate_superblock_checkpoint, superblock_period) {
                                        Either::Left(act.build_and_vote_candidate_superblock(candidate_superblock_epoch).map_ok(move |_, act, _| {
                                            let superblock_index = candidate_superblock_epoch / superblock_period;
                                            // Copy current chain state into previous chain state, but do not persist it yet
                                            act.move_chain_state_forward(superblock_index);
                                        }))
                                    }
                                    else{
                                        Either::Right(actix::fut::ok(()))
                                    }
                                })
                                .and_then(move |_, act, ctx| {
                                    // Process remaining blocks
                                    let (batch_succeeded, num_processed_blocks) = act.process_blocks_batch(ctx, &sync_target, &remaining_blocks);
                                    if !batch_succeeded {
                                        act.drop_all_outbounds_and_ice_sender(sender);

                                        log::error!("Received invalid blocks batch...");
                                        act.update_state_machine(StateMachine::WaitingConsensus, ctx);
                                        act.sync_waiting_for_add_blocks_since = None;

                                        return actix::fut::err(());
                                    }
                                    log_sync_progress(&sync_target, &remaining_blocks, num_processed_blocks, "SyncWithCandidate(remaining)");
                                    log::info!("Block sync target achieved");
                                    // Target achieved, go back to state 1
                                    act.update_state_machine(StateMachine::WaitingConsensus, ctx);
                                    actix::fut::ok(())
                                })
                                .map(|_res: Result<(), ()>, _act, _ctx| ())
                                .wait(ctx);
                        }
                        Err(ChainManagerError::WrongBlocksForSuperblock {
                                wrong_index,
                                consolidated_superblock_index,
                                current_superblock_index,
                            }) => {
                            log::warn!("Received unexpected block {wrong_index} for superblock index (consolidated: {consolidated_superblock_index}, current {current_superblock_index}). Delaying synchronization until next epoch.",

                        );
                            act.update_state_machine(StateMachine::WaitingConsensus, ctx);
                            act.sync_waiting_for_add_blocks_since = None;
                        }
                        Err(e) => {
                            log::error!("Unexpected error while splitting received blocks {e:?}")
                        }
                    };
                }
            };

            // TODO: check when `sync_waiting_for_add_blocks_since` is set
            // If we are not synchronizing, forget about when we started synchronizing
            if act.sm_state != StateMachine::Synchronizing {
                act.sync_waiting_for_add_blocks_since = None;
            }

            Box::pin(actix::fut::err(()))
        }).and_then(|(), act, _ctx| {
            // TODO: check when `sync_waiting_for_add_blocks_since` is set
            // If we are not synchronizing, forget about when we started synchronizing
            if act.sm_state != StateMachine::Synchronizing {
                act.sync_waiting_for_add_blocks_since = None;
            }

            actix::fut::ok(())
        })
            .map(|_res: Result<(), ()>, _act, _ctx| ());

        Box::pin(fut)
    }
}

fn log_sync_progress(
    sync_target: &SyncTarget,
    blocks: &[Block],
    num_processed_blocks: usize,
    stage: &str,
) {
    if num_processed_blocks == 0 {
        log::debug!("{stage}: sync done, 0 blocks processed");
    } else {
        let last_processed_block = &blocks[num_processed_blocks - 1];
        let epoch_of_the_last_block = last_processed_block.block_header.beacon.checkpoint;
        log::debug!(
            "{}: sync done up to block #{} (last checkpoint of superblock #{} reached)",
            stage,
            epoch_of_the_last_block,
            sync_target.superblock.checkpoint
        );
    }
}

/// Handler for AddCandidates message
impl Handler<AddCandidates> for ChainManager {
    type Result = SessionUnitResult;

    fn handle(&mut self, msg: AddCandidates, _ctx: &mut Context<Self>) {
        // AddCandidates is needed in all states
        for block in msg.blocks {
            self.process_candidate(block);
        }
    }
}

/// Handler for AddSuperBlockVote message
impl Handler<AddSuperBlockVote> for ChainManager {
    type Result = Result<(), anyhow::Error>;

    fn handle(
        &mut self,
        AddSuperBlockVote { superblock_vote }: AddSuperBlockVote,
        ctx: &mut Context<Self>,
    ) -> Self::Result {
        self.add_superblock_vote(superblock_vote, ctx);

        Ok(())
    }
}

/// Handler for AddTransaction message
impl Handler<AddTransaction> for ChainManager {
    type Result = ResponseActFuture<Self, Result<(), anyhow::Error>>;

    fn handle(&mut self, msg: AddTransaction, _ctx: &mut Context<Self>) -> Self::Result {
        let timestamp_now = get_timestamp();
        self.add_transaction(msg, timestamp_now)
    }
}

/// Handler for GetBlocksEpochRange
impl Handler<GetBlocksEpochRange> for ChainManager {
    type Result = Result<Vec<(Epoch, Hash)>, ChainManagerError>;

    fn handle(&mut self, msg: GetBlocksEpochRange, _ctx: &mut Context<Self>) -> Self::Result {
        Ok(self.get_blocks_epoch_range(msg))
    }
}

impl PeersBeacons {
    /// Pretty-print a map {beacon: [peers]}
    pub fn pretty_format(&self) -> String {
        let mut beacon_peers_map: BTreeMap<String, Vec<String>> = BTreeMap::new();
        for (k, v) in self.pb.iter() {
            let v = v
                .as_ref()
                .map(|x| {
                    format!(
                        "Block #{}: {} - Superblock #{}: {}",
                        x.highest_block_checkpoint.checkpoint,
                        x.highest_block_checkpoint.hash_prev_block,
                        x.highest_superblock_checkpoint.checkpoint,
                        x.highest_superblock_checkpoint.hash_prev_block
                    )
                })
                .unwrap_or_else(|| "NO BEACON".to_string());
            beacon_peers_map.entry(v).or_default().push(k.to_string());
        }

        format!("{beacon_peers_map:?}")
    }

    /// Run the consensus on the beacons, will return the most common beacon.
    /// We do not take into account our beacon to calculate the consensus.
    /// The beacons are `Option<CheckpointBeacon>`, so peers that have not
    /// sent us a beacon are counted as `None`. Keeping that in mind, we
    /// reach a superblock consensus as long as consensus_threshold % of peers agree.
    /// It also returns a boolean to indicate if there is a block consensus,
    /// it means a consensus_threshold % of peers agree in superblock and block.
    pub fn superblock_consensus(&self, consensus_threshold: usize) -> Option<(LastBeacon, bool)> {
        // We need to add `num_missing_peers` times NO BEACON, to take into account
        // missing outbound peers.
        let num_missing_peers = self.outbound_limit
            .map(|outbound_limit| {
                // TODO: is it possible to receive more than outbound_limit beacons?
                // (it shouldn't be possible)
                assert!(self.pb.len() <= outbound_limit as usize, "Received more beacons than the outbound_limit. Check the code for race conditions.");
                usize::from(outbound_limit) - self.pb.len()
            })
            // The outbound limit is set when the SessionsManager actor is initialized, so here it
            // cannot be None. But if it is None, set num_missing_peers to 0 in order to calculate
            // consensus with the existing beacons.
            .unwrap_or(0);

        mode_consensus(
            self.pb
                .iter()
                .map(|(_p, b)| {
                    b.as_ref()
                        .map(|last_beacon| last_beacon.highest_superblock_checkpoint)
                })
                .chain(std::iter::repeat_n(None, num_missing_peers)),
            consensus_threshold,
        )
        // Flatten result:
        // None (consensus % below threshold) should be treated the same way as
        // Some(None) (most of the peers did not send a beacon)
        .and_then(|x| x)
        .map(|superblock_consensus| {
            // If there is superblock consensus, we also need to set the block consensus.
            // We will use all the beacons that set superblock_beacon to superblock_consensus
            // There are 3 cases:
            // * A majority of beacons agree on a block. We will use this block as the
            // block_consensus and unregister all the peers that voted a different block
            // * There is a majority of beacons but it is below consensus_threshold. We will use
            // this majority as the block_consensus, and we will unregister the peers that voted on
            // a different superblock
            // * There is a tie. Will use any block as the consensus. So if there are 4 votes for
            // A, 4 votes for B, and 1 vote for C, the consensus can be A, B, or C.
            let block_beacons: Vec<_> = self
                .pb
                .iter()
                .map(|(_p, b)| {
                    b.as_ref().and_then(|last_beacon| {
                        if last_beacon.highest_superblock_checkpoint == superblock_consensus {
                            Some(last_beacon.highest_block_checkpoint)
                        } else {
                            None
                        }
                    })
                })
                .collect();

            let block_consensus_mode = mode_consensus(block_beacons.iter(), consensus_threshold);
            let (block_consensus, is_there_block_consensus) = match block_consensus_mode {
                // Case 1
                Some(Some(x)) => (*x, true),
                _ => {
                    // Case 2
                    let block_beacons_flatten: Vec<CheckpointBeacon> =
                        block_beacons.into_iter().flatten().collect();
                    let first = block_beacons_flatten[0];
                    match mode_consensus(block_beacons_flatten.iter(), 0) {
                        Some(x) => (*x, false),
                        None => {
                            // Case 3: In case of tie, we can choose one random different to None
                            (first, false)
                        }
                    }
                }
            };

            (
                LastBeacon {
                    highest_superblock_checkpoint: superblock_consensus,
                    highest_block_checkpoint: block_consensus,
                },
                is_there_block_consensus,
            )
        })
    }

    /// Collects the peers to unregister based on the beacon they reported and the beacon to be compared it with
    pub fn decide_peers_to_unregister(&self, beacon: CheckpointBeacon) -> Vec<SocketAddr> {
        // Unregister peers which have a different beacon
        self.pb
            .iter()
            .filter_map(|(p, b)| {
                if b.as_ref()
                    .map(|last_beacon| last_beacon.highest_block_checkpoint != beacon)
                    .unwrap_or(true)
                {
                    Some(*p)
                } else {
                    None
                }
            })
            .collect()
    }

    /// Collects the peers to unregister based on the beacon they reported and the beacon to be compared it with
    pub fn decide_peers_to_unregister_s(&self, superbeacon: CheckpointBeacon) -> Vec<SocketAddr> {
        // Unregister peers which have a different beacon
        self.pb
            .iter()
            .filter_map(|(p, b)| {
                if b.as_ref()
                    .map(|last_beacon| last_beacon.highest_superblock_checkpoint != superbeacon)
                    .unwrap_or(true)
                {
                    Some(*p)
                } else {
                    None
                }
            })
            .collect()
    }

    /// Ignore beacons that are old or has a different hash if is the same super epoch
    pub fn ignore_no_consensus_beacons(&mut self, superbeacon: CheckpointBeacon) {
        for (addr, last_beacon_opt) in &mut self.pb {
            if let Some(last_beacon) = last_beacon_opt {
                // Ignore beacons that point to an old superblock checkpoint
                if last_beacon.highest_superblock_checkpoint.checkpoint < superbeacon.checkpoint
                    // Ignore beacons that do not point to the right consensus superblock
                    || (last_beacon.highest_superblock_checkpoint.checkpoint
                        == superbeacon.checkpoint
                        && last_beacon.highest_superblock_checkpoint != superbeacon)
                {
                    // Peers out of consensus will be treated the same as peers from which we receive no beacon
                    log::debug!("LastBeacon from: {} was set to NO_BEACON", *addr);
                    *last_beacon_opt = None;
                }
            }
        }
    }

    /// Collects the peers that have not sent us a beacon
    pub fn peers_with_no_beacon(&self) -> Vec<SocketAddr> {
        // Unregister peers which have not sent us a beacon
        self.pb
            .iter()
            .filter_map(|(p, b)| if b.is_none() { Some(*p) } else { None })
            .collect()
    }
}

impl Handler<PeersBeacons> for ChainManager {
    type Result = <PeersBeacons as Message>::Result;

    // FIXME(#676): Remove clippy skip error
    #[allow(clippy::cognitive_complexity)]
    fn handle(&mut self, mut peers_beacons: PeersBeacons, ctx: &mut Context<Self>) -> Self::Result {
        log::debug!(
            "PeersBeacons received while StateMachine is in state {:?}",
            self.sm_state
        );

        log::debug!("Received beacons: {}", peers_beacons.pretty_format());

        // Activate peers beacons index to continue synced
        if !peers_beacons.pb.is_empty() {
            self.peers_beacons_received = true;
        }

        // Remove beacons that not are compatible with the last superblock consensus
        if let Some(last_superblock_consensus) = self.last_superblock_consensus {
            peers_beacons.ignore_no_consensus_beacons(last_superblock_consensus);
        }

        if self.current_epoch.is_none() {
            return Err(());
        }

        // Calculate the consensus, or None if there is no consensus
        let consensus_threshold = self.consensus_c as usize;
        let beacon_consensus = peers_beacons.superblock_consensus(consensus_threshold);
        let outbound_limit = peers_beacons.outbound_limit;
        let pb_len = peers_beacons.pb.len();
        self.last_received_beacons.clone_from(&peers_beacons.pb);
        let peers_needed_for_consensus = outbound_limit
            .map(|x| {
                // ceil(x * consensus_threshold / 100)
                (usize::from(x) * consensus_threshold).div_ceil(100)
            })
            .unwrap_or(1);

        let peers_with_no_beacon = peers_beacons.peers_with_no_beacon();
        // Ice peers with a beacon that does not point to last consensus superblock, or provided no beacon at all
        for peer in &peers_with_no_beacon {
            self.ice_peer(Some(*peer));
        }

        let peers_to_unregister = if let Some((consensus, is_there_block_consensus)) =
            beacon_consensus.as_ref()
        {
            if *is_there_block_consensus {
                peers_beacons.decide_peers_to_unregister(consensus.highest_block_checkpoint)
            } else {
                peers_beacons.decide_peers_to_unregister_s(consensus.highest_superblock_checkpoint)
            }
        } else if pb_len < peers_needed_for_consensus {
            // Not enough outbound peers, do not unregister any peers
            log::debug!(
                "Got {pb_len} peers but need at least {peers_needed_for_consensus} to calculate the consensus"
            );
            vec![]
        } else {
            // No consensus: if state is AlmostSynced unregister those that are not coincident with ours.
            // Else, unregister all peers
            if self.sm_state == StateMachine::AlmostSynced || self.sm_state == StateMachine::Synced
            {
                log::warn!(
                    "Lack of peer consensus while state is {:?}: peers that do not coincide with our last beacon will be unregistered",
                    self.sm_state
                );
                peers_beacons.decide_peers_to_unregister(self.get_chain_beacon())
            } else {
                log::warn!("Lack of peer consensus: all peers will be unregistered");
                peers_beacons.pb.into_iter().map(|(p, _b)| p).collect()
            }
        };

        let peers_to_unregister = match self.sm_state {
            StateMachine::WaitingConsensus => {
                // As soon as there is consensus, we set the target beacon to the consensus
                // and set the state to Synchronizing
                match beacon_consensus {
                    Some((
                        LastBeacon {
                            highest_superblock_checkpoint: superblock_consensus,
                            highest_block_checkpoint: consensus_beacon,
                        },
                        _,
                    )) => {
                        let target = SyncTarget {
                            block: consensus_beacon,
                            superblock: superblock_consensus,
                        };
                        self.sync_target = Some(target);
                        log::info!(
                            "Synchronization target has been set ({}: {})",
                            target.block.checkpoint,
                            target.block.hash_prev_block
                        );
                        log::debug!("{target:#?}");

                        let our_beacon = self.get_chain_beacon();
                        log::debug!(
                            "Consensus beacon: {:?} Our beacon {:?}",
                            (consensus_beacon, superblock_consensus),
                            our_beacon
                        );

                        let consensus_constants = self.consensus_constants();

                        // Check if we are already synchronized
                        // TODO: use superblock beacon
                        let next_state = if consensus_beacon.hash_prev_block
                            == consensus_constants.bootstrap_hash
                        {
                            log::debug!("The consensus is that there is no genesis block yet");

                            StateMachine::WaitingConsensus
                        } else if our_beacon == consensus_beacon {
                            StateMachine::AlmostSynced
                        } else if our_beacon.checkpoint == consensus_beacon.checkpoint
                            && our_beacon.hash_prev_block != consensus_beacon.hash_prev_block
                        {
                            // Fork case
                            log::warn!(
                                "[CONSENSUS]: The local chain is apparently forked.\n\
                                We are on {our_beacon:?} but the network is on {consensus_beacon:?}.\n\
                                The node will automatically try to recover from this forked situation by restoring the chain state from the storage."
                            );

                            self.initialize_from_storage(ctx);
                            log::info!(
                                "The chain state has been restored from storage.\n\
                            Now the node will try to resynchronize."
                            );

                            StateMachine::WaitingConsensus
                        } else {
                            // Review candidates
                            let consensus_block_hash = consensus_beacon.hash_prev_block;
                            let candidates = self
                                .candidates
                                .remove(&consensus_block_hash)
                                .unwrap_or_default();
                            // Clear candidates, as they are only valid for one epoch
                            self.candidates.clear();
                            self.seen_candidates.clear();

                            if candidates.len() > 1 {
                                log::warn!(
                                    "There are {} block candidates with the same hash",
                                    candidates.len()
                                );
                            }
                            let mut consolidated_consensus_candidate = false;
                            for consensus_block in candidates {
                                match self.process_requested_block(ctx, consensus_block, false) {
                                    Ok(()) => {
                                        consolidated_consensus_candidate = true;
                                        log::info!(
                                            "Consolidate consensus candidate. AlmostSynced state"
                                        );
                                        break;
                                    }
                                    Err(e) => {
                                        log::debug!(
                                            "Failed to consolidate consensus candidate: {e}"
                                        );
                                    }
                                }
                            }

                            if consolidated_consensus_candidate {
                                StateMachine::AlmostSynced
                            } else {
                                self.request_blocks_batch(ctx);

                                StateMachine::Synchronizing
                            }
                        };

                        self.update_state_machine(next_state, ctx);

                        Ok(peers_to_unregister)
                    }
                    // No consensus: unregister all peers
                    None => Ok(peers_to_unregister),
                }
            }
            StateMachine::Synchronizing => {
                match beacon_consensus {
                    Some((
                        LastBeacon {
                            highest_superblock_checkpoint: superblock_consensus,
                            highest_block_checkpoint: consensus_beacon,
                        },
                        _,
                    )) => {
                        self.sync_target = Some(SyncTarget {
                            block: consensus_beacon,
                            superblock: superblock_consensus,
                        });

                        let our_beacon = self.get_chain_beacon();

                        // Check if we are already synchronized
                        let next_state = if our_beacon == consensus_beacon {
                            StateMachine::AlmostSynced
                        } else if our_beacon.checkpoint == consensus_beacon.checkpoint
                            && our_beacon.hash_prev_block != consensus_beacon.hash_prev_block
                        {
                            // Fork case
                            log::warn!(
                                "[CONSENSUS]: We are on {our_beacon:?} but the network is on {consensus_beacon:?}"
                            );

                            self.initialize_from_storage(ctx);
                            log::info!("Restored chain state from storage");

                            StateMachine::WaitingConsensus
                        } else {
                            StateMachine::Synchronizing
                        };

                        self.update_state_machine(next_state, ctx);

                        Ok(peers_to_unregister)
                    }
                    // No consensus: unregister all peers
                    None => {
                        self.update_state_machine(StateMachine::WaitingConsensus, ctx);

                        Ok(peers_to_unregister)
                    }
                }
            }
            StateMachine::AlmostSynced | StateMachine::Synced => {
                let our_beacon = self.get_chain_beacon();
                match beacon_consensus {
                    Some((
                        LastBeacon {
                            highest_block_checkpoint: consensus_beacon,
                            ..
                        },
                        is_there_block_consensus,
                    )) if consensus_beacon == our_beacon => {
                        if self.sm_state == StateMachine::AlmostSynced && is_there_block_consensus {
                            // This is the only point in the whole base code for the state
                            // machine to move into `Synced` state.
                            self.update_state_machine(StateMachine::Synced, ctx);
                        }
                        Ok(peers_to_unregister)
                    }
                    Some((
                        LastBeacon {
                            highest_block_checkpoint: consensus_beacon,
                            ..
                        },
                        _,
                    )) => {
                        // We are out of consensus!
                        log::warn!(
                            "[CONSENSUS]: We are on {our_beacon:?} but the network is on {consensus_beacon:?}",
                        );

                        // We will move to AlmostSynced to disable mining while preserving network
                        // stability, as superblock votes should still be produced and broadcast
                        self.update_state_machine(StateMachine::AlmostSynced, ctx);

                        // We will remove those peers that either signaled beacons differing from
                        // the last consensus or did not send any beacon

                        Ok(peers_with_no_beacon)
                    }
                    None => {
                        // If we are synced and the consensus beacon is not the same as our beacon, then
                        // we need to rewind one epoch
                        if pb_len == 0 {
                            log::warn!(
                                "[CONSENSUS]: We have not received any beacons for this epoch"
                            );
                        } else {
                            // There is no consensus because of a tie, do not rewind?
                            // For example this could happen when each peer reports a different beacon...
                            log::warn!(
                                "[CONSENSUS]: We are on {our_beacon:?} but the network has no consensus"
                            );
                        }

                        // We will move to AlmostSynced to do not allow mining and preserve network stability
                        self.update_state_machine(StateMachine::AlmostSynced, ctx);

                        // We will remove those that are different from the last consensus or
                        // peers that has a block different to us

                        Ok(peers_with_no_beacon)
                    }
                }
            }
        };

        if self.sm_state == StateMachine::Synchronizing {
            // Request target superblock from a random peer
            let superblock_consensus = self
                .sync_target
                .as_ref()
                .expect("sync_target must be always set in Synchronizing state")
                .superblock;
            self.request_sync_target_superblock(ctx, superblock_consensus);

            if let Some(sync_start_epoch) = self.sync_waiting_for_add_blocks_since {
                let current_epoch = self.current_epoch.unwrap();
                let how_many_epochs_are_we_willing_to_wait_for_one_block_batch = 10;
                if current_epoch - sync_start_epoch
                    >= how_many_epochs_are_we_willing_to_wait_for_one_block_batch
                {
                    log::warn!("Timeout for waiting for blocks achieved. Requesting blocks again.");
                    self.request_blocks_batch(ctx);
                }
            }
        } else if self.sm_state == StateMachine::AlmostSynced
            || self.sm_state == StateMachine::Synced
        {
            // Create and broadcast a superblock in case of superblock period.
            // Everyone creates superblocks, but only ARS members sign and broadcast them
            let superblock_period = u32::from(self.consensus_constants().superblock_period);
            let current_epoch = self.current_epoch.unwrap();
            // During epoch 0 there is no need to create the superblock 0
            if current_epoch != 0 && current_epoch % superblock_period == 0 {
                self.create_and_broadcast_superblock(ctx, current_epoch);
            }
        }

        peers_to_unregister
    }
}

impl Handler<BuildVtt> for ChainManager {
    type Result = ResponseActFuture<Self, <BuildVtt as Message>::Result>;

    fn handle(&mut self, msg: BuildVtt, _ctx: &mut Self::Context) -> Self::Result {
        if self.sm_state != StateMachine::Synced {
            return Box::pin(actix::fut::err(
                ChainManagerError::NotSynced {
                    current_state: self.sm_state,
                }
                .into(),
            ));
        }
        let timestamp = u64::try_from(get_timestamp()).unwrap();
        let max_vt_weight = self.consensus_constants().max_vt_weight;
        match transaction_factory::build_vtt(
            msg.vto,
            msg.fee,
            &mut self.chain_state.own_utxos,
            self.own_pkh.unwrap(),
            &self.chain_state.unspent_outputs_pool,
            timestamp,
            self.tx_pending_timeout,
            &msg.utxo_strategy,
            max_vt_weight,
            msg.dry_run,
        ) {
            Err(e) => {
                log::error!("Error when building value transfer transaction: {e}");
                Box::pin(actix::fut::err(e.into()))
            }
            Ok(vtt) => {
                let fut = signature_mngr::sign_transaction_hash(vtt.hash(), vtt.inputs.len())
                    .into_actor(self)
                    .then(move |s, act, _ctx| match s {
                        Ok(signatures) => {
                            let vtt = VTTransaction::new(vtt, signatures);

                            if msg.dry_run {
                                Either::Right(actix::fut::result(Ok(vtt)))
                            } else {
                                let transaction = Transaction::ValueTransfer(vtt.clone());
                                Either::Left(
                                    act.add_transaction(
                                        AddTransaction {
                                            transaction,
                                            broadcast_flag: true,
                                        },
                                        get_timestamp(),
                                    )
                                    .map_ok(move |_, _, _| vtt),
                                )
                            }
                        }
                        Err(e) => {
                            log::error!("Failed to sign value transfer transaction: {e}");
                            Either::Right(actix::fut::result(Err(e)))
                        }
                    });

                Box::pin(fut)
            }
        }
    }
}

impl Handler<BuildStake> for ChainManager {
    type Result = ResponseActFuture<Self, <BuildStake as Message>::Result>;

    fn handle(&mut self, msg: BuildStake, _ctx: &mut Self::Context) -> Self::Result {
        if !msg.dry_run && self.sm_state != StateMachine::Synced {
            return Box::pin(actix::fut::err(
                ChainManagerError::NotSynced {
                    current_state: self.sm_state,
                }
                .into(),
            ));
        }
        let timestamp = u64::try_from(get_timestamp()).unwrap();
        let current_epoch = self.current_epoch.unwrap();
        let maximum_stake_block_weight = self
            .consensus_constants_wit2
            .get_maximum_stake_block_weight(current_epoch);
        match transaction_factory::build_st(
            msg.stake_output,
            msg.fee,
            &mut self.chain_state.own_utxos,
            self.own_pkh.unwrap(),
            &self.chain_state.unspent_outputs_pool,
            timestamp,
            self.tx_pending_timeout,
            &msg.utxo_strategy,
            maximum_stake_block_weight,
            msg.dry_run,
        ) {
            Err(e) => {
                log::error!("Error when building stake transaction: {e}");
                Box::pin(actix::fut::err(e.into()))
            }
            Ok(st) => {
                let fut = signature_mngr::sign_transaction_hash(st.hash(), st.inputs.len())
                    .into_actor(self)
                    .then(move |s, act, _ctx| match s {
                        Ok(signatures) => {
                            let st = StakeTransaction::new(st, signatures);

                            if msg.dry_run {
                                Either::Right(actix::fut::result(Ok(st)))
                            } else {
                                let transaction = Transaction::Stake(st.clone());
                                Either::Left(
                                    act.add_transaction(
                                        AddTransaction {
                                            transaction,
                                            broadcast_flag: true,
                                        },
                                        get_timestamp(),
                                    )
                                    .map_ok(move |_, _, _| st),
                                )
                            }
                        }
                        Err(e) => {
                            log::error!("Failed to sign stake transaction: {e}");
                            Either::Right(actix::fut::result(Err(e)))
                        }
                    });

                Box::pin(fut)
            }
        }
    }
}

impl Handler<BuildUnstake> for ChainManager {
    type Result = ResponseActFuture<Self, <BuildUnstake as Message>::Result>;

    fn handle(&mut self, msg: BuildUnstake, _ctx: &mut Self::Context) -> Self::Result {
        if !msg.dry_run && self.sm_state != StateMachine::Synced {
            return Box::pin(actix::fut::err(
                ChainManagerError::NotSynced {
                    current_state: self.sm_state,
                }
                .into(),
            ));
        }

        match self.chain_state.stakes.query_nonce(StakeKey {
            validator: msg.operator,
            withdrawer: self.own_pkh.unwrap(),
        }) {
            Err(e) => {
                log::error!("Error when building unstake transaction: {e}");
                Box::pin(actix::fut::err(e.into()))
            }
            Ok(nonce) => {
                let current_epoch = self.current_epoch.unwrap();
                let unstaking_delay = self
                    .consensus_constants_wit2
                    .get_unstaking_delay_seconds(current_epoch);
                let withdrawal = ValueTransferOutput {
                    time_lock: unstaking_delay,
                    pkh: self.own_pkh.unwrap(),
                    value: msg.value,
                };
                match transaction_factory::build_ut(msg.operator, withdrawal, msg.fee, nonce) {
                    Err(e) => {
                        log::error!("Error when building unstake transaction: {e}");
                        Box::pin(actix::fut::err(e.into()))
                    }
                    Ok(ut) => {
                        let fut = signature_mngr::sign_transaction_hash(ut.hash(), 1)
                            .into_actor(self)
                            .then(move |s, act, _ctx| match s {
                                Ok(signature) => {
                                    let ut = UnstakeTransaction::new(
                                        ut,
                                        signature.first().unwrap().clone(),
                                    );

                                    if msg.dry_run {
                                        Either::Right(actix::fut::result(Ok(ut)))
                                    } else {
                                        let transaction = Transaction::Unstake(ut.clone());
                                        Either::Left(
                                            act.add_transaction(
                                                AddTransaction {
                                                    transaction,
                                                    broadcast_flag: true,
                                                },
                                                get_timestamp(),
                                            )
                                            .map_ok(move |_, _, _| ut),
                                        )
                                    }
                                }
                                Err(e) => {
                                    log::error!("Failed to sign unstake transaction: {e}");
                                    Either::Right(actix::fut::result(Err(e)))
                                }
                            });

                        Box::pin(fut)
                    }
                }
            }
        }
    }
}

impl Handler<QueryStakes> for ChainManager {
    type Result = <QueryStakes as Message>::Result;

    fn handle(&mut self, msg: QueryStakes, _ctx: &mut Self::Context) -> Self::Result {
        let filter = msg.filter.unwrap_or_default();
        let params = msg.params.unwrap_or_default();
        let since = params.since.unwrap_or_default();
        let order = params.order.unwrap_or_default();
        let order_desc = order.reverse.unwrap_or_default();
        let distinct = params.distinct.unwrap_or_default();
        let offset = params.offset.unwrap_or_default();
        let limit = params.limit.unwrap_or(u16::MAX) as usize;

        // fetch filtered stake entries from current chain state:
        let mut stakes = self.chain_state.stakes.query_stakes(filter.clone())?;

        // filter out stake entries having a nonce, last mining or witnessing epochs (depending
        // on actual ordering field) older than calculated `since_epoch`:
        let since_epoch: u64 = if since >= 0 {
            since.try_into().inspect_err(|&e| {
                log::warn!("Invalid 'since' limit on QueryStakes: {e}");
            })?
        } else {
            (i64::from(self.current_epoch.unwrap()) + since)
                .try_into()
                .unwrap_or_default()
        };
        if since_epoch > 0 {
            match order.by {
                QueryStakesOrderByOptions::Coins | QueryStakesOrderByOptions::Nonce => {
                    stakes.retain(|stake| stake.value.nonce >= since_epoch);
                }
                QueryStakesOrderByOptions::Mining => {
                    stakes.retain(|stake| {
                        u64::from(stake.value.epochs.mining) >= since_epoch
                            && u64::from(stake.value.epochs.mining) >= stake.value.nonce
                    });
                }
                QueryStakesOrderByOptions::Witnessing => {
                    stakes.retain(|stake| {
                        u64::from(stake.value.epochs.witnessing) >= since_epoch
                            && u64::from(stake.value.epochs.witnessing) >= stake.value.nonce
                    });
                }
            }
        }

        // sort stake entries by specified field, worst case costing n * log (n) ...
        match order.by {
            QueryStakesOrderByOptions::Coins => {
                stakes.sort_by(|a, b| {
                    if order_desc {
                        b.value.coins.cmp(&a.value.coins)
                    } else {
                        a.value.coins.cmp(&b.value.coins)
                    }
                });
            }
            QueryStakesOrderByOptions::Nonce => {
                stakes.sort_by(|a, b| {
                    if order_desc {
                        b.value.nonce.cmp(&a.value.nonce)
                    } else {
                        a.value.nonce.cmp(&b.value.nonce)
                    }
                });
            }
            QueryStakesOrderByOptions::Mining => {
                stakes.sort_by(|a, b| {
                    if order_desc {
                        b.value.epochs.mining.cmp(&a.value.epochs.mining)
                    } else {
                        a.value.epochs.mining.cmp(&b.value.epochs.mining)
                    }
                });
            }
            QueryStakesOrderByOptions::Witnessing => {
                stakes.sort_by(|a, b| {
                    if order_desc {
                        b.value.epochs.witnessing.cmp(&a.value.epochs.witnessing)
                    } else {
                        a.value.epochs.witnessing.cmp(&b.value.epochs.witnessing)
                    }
                });
            }
        }

        // if `distinct` is required, retain first appearence per validator:
        let stakes = if distinct {
            stakes
                .into_iter()
                .unique_by(|stake| stake.key.validator)
                .collect()
        } else {
            stakes
        };

        // applies `offset`, if any, and limits number of returned records to `limit``
        Ok(stakes.into_iter().skip(offset).take(limit).collect())
    }
}

impl Handler<QueryStakingPowers> for ChainManager {
    type Result = <QueryStakingPowers as Message>::Result;

    fn handle(&mut self, msg: QueryStakingPowers, _ctx: &mut Self::Context) -> Self::Result {
        let current_epoch = self.current_epoch.unwrap();
        let limit = msg.limit.unwrap_or(u16::MAX) as usize;
        let offset = msg.offset.unwrap_or_default();

        // Enumerate power entries order by specified capability,
        // mapping into a record containing: the ranking, the stake key and current power
        let powers = self
            .chain_state
            .stakes
            .by_rank(msg.order_by, current_epoch)
            .enumerate()
            .map(|(index, record)| (index + 1, record.0, record.1));

        // Skip first `offset` entries and take next `limit`.
        // If distinct is specified, retain just first appearance per validator.
        let powers: Vec<(usize, StakeKey<PublicKeyHash>, Power)> = if msg.distinct.unwrap_or(false)
        {
            powers
                .unique_by(|(_, key, _)| key.validator)
                .skip(offset)
                .take(limit)
                .collect()
        } else {
            powers.skip(offset).take(limit).collect()
        };

        powers
    }
}

impl Handler<BuildDrt> for ChainManager {
    type Result = ResponseActFuture<Self, Result<DRTransaction, anyhow::Error>>;

    fn handle(&mut self, msg: BuildDrt, _ctx: &mut Self::Context) -> Self::Result {
        if self.sm_state != StateMachine::Synced {
            return Box::pin(actix::fut::err(
                ChainManagerError::NotSynced {
                    current_state: self.sm_state,
                }
                .into(),
            ));
        }

        let active_wips = ActiveWips {
            active_wips: self.chain_state.tapi_engine.wip_activation.clone(),
            block_epoch: self.current_epoch.unwrap(),
        };

        let dr_output = msg.dro;
        if let Err(e) = validate_rad_request(&dr_output.data_request, &active_wips) {
            return Box::pin(actix::fut::err(e));
        }
        let timestamp = u64::try_from(get_timestamp()).unwrap();
        let max_dr_weight = self.consensus_constants().max_dr_weight;
        match transaction_factory::build_drt(
            dr_output,
            msg.fee,
            &mut self.chain_state.own_utxos,
            self.own_pkh.unwrap(),
            &self.chain_state.unspent_outputs_pool,
            timestamp,
            self.tx_pending_timeout,
            max_dr_weight,
            msg.dry_run,
        ) {
            Err(e) => {
                log::error!("Error when building data request transaction: {e}");
                Box::pin(actix::fut::err(e.into()))
            }
            Ok(drt) => {
                log::debug!("Created drt:\n{drt:?}");
                let fut = signature_mngr::sign_transaction_hash(drt.hash(), drt.inputs.len())
                    .into_actor(self)
                    .then(move |s, act, _ctx| match s {
                        Ok(signatures) => {
                            let drt = DRTransaction::new(drt, signatures);

                            if msg.dry_run {
                                Either::Right(actix::fut::result(Ok(drt)))
                            } else {
                                let transaction = Transaction::DataRequest(drt.clone());
                                Either::Left(
                                    act.add_transaction(
                                        AddTransaction {
                                            transaction,
                                            broadcast_flag: true,
                                        },
                                        get_timestamp(),
                                    )
                                    .map_ok(move |_, _, _| drt),
                                )
                            }
                        }
                        Err(e) => {
                            log::error!("Failed to sign data request transaction: {e}");
                            Either::Right(actix::fut::result(Err(e)))
                        }
                    });

                Box::pin(fut)
            }
        }
    }
}

impl Handler<GetState> for ChainManager {
    type Result = <GetState as Message>::Result;

    fn handle(&mut self, _msg: GetState, _ctx: &mut Self::Context) -> Self::Result {
        Ok(self.sm_state)
    }
}

impl Handler<GetDataRequestInfo> for ChainManager {
    type Result = ResponseFuture<Result<DataRequestInfo, anyhow::Error>>;

    fn handle(&mut self, msg: GetDataRequestInfo, _ctx: &mut Self::Context) -> Self::Result {
        let dr_pointer = msg.dr_pointer;

        // First, try to get it from memory
        if let Some(dr_info) = self
            .chain_state
            .data_request_pool
            .data_request_pool
            .get(&dr_pointer)
            .map(|dr_state| dr_state.info.clone())
        {
            Box::pin(future::ready(Ok(dr_info)))
        } else {
            let dr_pointer_string = format!("DR-REPORT-{dr_pointer}");
            // Otherwise, try to get it from storage
            let fut = async move {
                let dr_info = storage_mngr::get::<_, DataRequestInfo>(&dr_pointer_string).await?;

                match dr_info {
                    Some(x) => Ok(x),
                    None => Err(DataRequestNotFound { hash: dr_pointer }.into()),
                }
            };

            Box::pin(fut)
        }
    }
}

#[allow(clippy::cast_sign_loss)]
impl Handler<GetBalance2> for ChainManager {
    type Result = Result<NodeBalance2, anyhow::Error>;

    fn handle(&mut self, msg: GetBalance2, _ctx: &mut Self::Context) -> Self::Result {
        if self.sm_state != StateMachine::Synced {
            return Err(ChainManagerError::NotSynced {
                current_state: self.sm_state,
            }
            .into());
        }
        let now = get_timestamp() as u64;
        let balance = match msg {
            GetBalance2::All(limits) => {
                let mut balances: HashMap<PublicKeyHash, NodeBalance2> = HashMap::new();
                for (_output_pointer, (vto, _)) in self.chain_state.unspent_outputs_pool.iter() {
                    balances
                        .entry(vto.pkh)
                        .or_insert(NodeBalance2::One {
                            locked: 0,
                            staked: 0,
                            unlocked: 0,
                        })
                        .add_utxo_value(vto.value, vto.time_lock > now);
                }
                let stakes = self.chain_state.stakes.query_stakes(QueryStakesKey::All);
                if let Ok(stakes) = stakes {
                    stakes.iter().for_each(|entry| {
                        balances
                            .entry(entry.key.withdrawer)
                            .or_insert(NodeBalance2::One {
                                locked: 0,
                                staked: 0,
                                unlocked: 0,
                            })
                            .add_staked(entry.value.coins.into())
                    });
                }
                balances.retain(|_pkh, balance| {
                    balance.get_total().unwrap_or_default()
                        >= Wit::from_nanowits(limits.min_balance)
                        && balance.get_total().unwrap_or_default()
                            <= Wit::from_nanowits(limits.max_balance.unwrap_or(u64::MAX))
                });

                NodeBalance2::Many(balances)
            }
            GetBalance2::Address(pkh) => {
                let pkh = try_do_magic_into_pkh(pkh.clone()).map_err(|e| {
                    log::warn!("Failed to convert {pkh:?} into a valid public key hash: {e}");
                    e
                })?;
                let mut balance = transaction_factory::get_utxos_balance(
                    &self.chain_state.unspent_outputs_pool,
                    pkh,
                    now,
                );
                let stakes = self
                    .chain_state
                    .stakes
                    .query_stakes(QueryStakesKey::Withdrawer(pkh));
                if let Ok(stakes) = stakes {
                    balance.add_staked(stakes.iter().fold(0u64, |staked: u64, entry| {
                        staked.saturating_add(entry.value.coins.nanowits())
                    }));
                }

                balance
            }
            GetBalance2::Sum(pkhs) => {
                let (total_locked, total_staked, total_unlocked) = pkhs.iter().fold(
                    Default::default(),
                    |(total_locked, total_staked, total_unlocked), pkh| {
                        if let Ok(NodeBalance2::One {
                            locked,
                            staked,
                            unlocked,
                        }) = self.handle(GetBalance2::Address(pkh.clone()), _ctx)
                        {
                            (
                                total_locked + locked,
                                total_staked + staked,
                                total_unlocked + unlocked,
                            )
                        } else {
                            (total_locked, total_staked, total_unlocked)
                        }
                    },
                );

                NodeBalance2::One {
                    locked: total_locked,
                    staked: total_staked,
                    unlocked: total_unlocked,
                }
            }
        };

        Ok(balance)
    }
}

impl Handler<GetBalance> for ChainManager {
    type Result = Result<NodeBalance, anyhow::Error>;

    fn handle(
        &mut self,
        GetBalance { target, simple }: GetBalance,
        _ctx: &mut Self::Context,
    ) -> Self::Result {
        if self.sm_state != StateMachine::Synced {
            return Err(ChainManagerError::NotSynced {
                current_state: self.sm_state,
            }
            .into());
        }

        let own_pkh = self.own_pkh.unwrap_or_default();
        let target = target.resolve_own(own_pkh);

        let balance = match target {
            GetBalanceTarget::All => {
                let mut balances: HashMap<PublicKeyHash, NodeBalance> = HashMap::new();

                for (_output_pointer, (vto, _)) in self.chain_state.unspent_outputs_pool.iter() {
                    balances
                        .entry(vto.pkh)
                        .or_insert(NodeBalance::One {
                            total: 0,
                            confirmed: None,
                        })
                        .add_value(vto.value);
                }

                NodeBalance::Many(balances)
            }
            GetBalanceTarget::Own => {
                if simple {
                    // Calculate balance using OwnUnspentOutputsPool, which is much faster but only works
                    // when using the node pkh, and does only return unconfirmed balance.
                    let total = self
                        .chain_state
                        .own_utxos
                        .get_balance(&self.chain_state.unspent_outputs_pool);

                    NodeBalance::One {
                        confirmed: None,
                        total,
                    }
                } else {
                    transaction_factory::get_total_balance(
                        &self.chain_state.unspent_outputs_pool,
                        own_pkh,
                        simple,
                    )
                }
            }
            GetBalanceTarget::Address(pkh) => transaction_factory::get_total_balance(
                &self.chain_state.unspent_outputs_pool,
                pkh,
                simple,
            ),
        };

        Ok(balance)
    }
}

impl Handler<GetSupplyInfo> for ChainManager {
    type Result = Result<SupplyInfo, anyhow::Error>;

    fn handle(&mut self, _msg: GetSupplyInfo, _ctx: &mut Self::Context) -> Self::Result {
        if self.sm_state != StateMachine::Synced {
            return Err(ChainManagerError::NotSynced {
                current_state: self.sm_state,
            }
            .into());
        }

        let chain_info = self.chain_state.chain_info.as_ref().unwrap();
        let halving_period = chain_info.consensus_constants.halving_period;
        let initial_block_reward = chain_info.consensus_constants.initial_block_reward;
        let collateral_minimum = chain_info.consensus_constants.collateral_minimum;

        let current_epoch = self.current_epoch.unwrap();
        let current_time = u64::try_from(get_timestamp()).unwrap();

        let mut current_unlocked_supply = 0;
        let mut current_locked_supply = 0;
        for (_output_pointer, value_transfer_output) in self.chain_state.unspent_outputs_pool.iter()
        {
            if value_transfer_output.0.time_lock <= current_time {
                current_unlocked_supply += value_transfer_output.0.value;
            } else {
                current_locked_supply += value_transfer_output.0.value;
            }
        }

        let in_flight_requests = self
            .chain_state
            .data_request_pool
            .data_request_pool
            .len()
            .try_into()
            .unwrap();
        let locked_wits_by_requests = self
            .chain_state
            .data_request_pool
            .locked_wits_by_requests(collateral_minimum);

        let (mut blocks_minted, mut blocks_minted_reward) = (0, 0);
        let (mut blocks_missing, mut blocks_missing_reward) = (0, 0);
        for epoch in 1..current_epoch {
            let block_reward = block_reward(epoch, initial_block_reward, halving_period);
            // If the blockchain contains an epoch, a block was minted in that epoch, add the reward to blocks_minted_reward
            if self.chain_state.block_chain.contains_key(&epoch) {
                blocks_minted += 1;
                blocks_minted_reward += block_reward;
                // Otherwise, a block was rolled back or no block was proposed, add the reward to blocks_missing_reward
            } else {
                blocks_missing += 1;
                blocks_missing_reward += block_reward;
            }
        }

        let genesis_amount =
            current_locked_supply + current_unlocked_supply + locked_wits_by_requests
                - blocks_minted_reward;
        let maximum_block_reward = total_block_reward(initial_block_reward, halving_period);
        let maximum_supply = genesis_amount + maximum_block_reward;

        Ok(SupplyInfo {
            epoch: current_epoch,
            current_time,
            blocks_minted,
            blocks_minted_reward,
            blocks_missing,
            blocks_missing_reward,
            in_flight_requests,
            locked_wits_by_requests,
            current_unlocked_supply,
            current_locked_supply,
            maximum_supply,
        })
    }
}

impl Handler<GetSupplyInfo2> for ChainManager {
    type Result = Result<SupplyInfo2, anyhow::Error>;

    fn handle(&mut self, _msg: GetSupplyInfo2, _ctx: &mut Self::Context) -> Self::Result {
        if self.sm_state != StateMachine::Synced {
            return Err(ChainManagerError::NotSynced {
                current_state: self.sm_state,
            }
            .into());
        }

        let chain_info = self.chain_state.chain_info.as_ref().unwrap();
        let current_epoch = self.current_epoch.unwrap();
        let current_time = u64::try_from(get_timestamp()).unwrap();
        let current_staked_supply = self.chain_state.stakes.total_staked().nanowits();

        let wit1_block_reward = chain_info.consensus_constants.initial_block_reward;
        let wit2_activated = get_protocol_version(None) == ProtocolVersion::V2_0;
        let wit2_activation_epoch = get_protocol_version_activation_epoch(ProtocolVersion::V2_0);
        let wit2_block_reward =
            ConsensusConstantsWit2::default().get_validator_block_reward(current_epoch);

        let collateral_minimum = chain_info.consensus_constants.collateral_minimum;

        let requests_in_flight_collateral = self
            .chain_state
            .data_request_pool
            .locked_wits_by_requests(collateral_minimum);

        let mut current_locked_supply = requests_in_flight_collateral;
        let mut current_unlocked_supply = 0;
        for (_output_pointer, value_transfer_output) in self.chain_state.unspent_outputs_pool.iter()
        {
            if value_transfer_output.0.time_lock <= current_time {
                current_unlocked_supply += value_transfer_output.0.value;
            } else {
                current_locked_supply += value_transfer_output.0.value;
            }
        }

        let (mut blocks_minted, mut blocks_minted_reward) = (0, 0);
        for epoch in 1..current_epoch {
            // If the blockchain contains an epoch, a block was minted in that epoch, add the reward to blocks_minted_reward
            if self.chain_state.block_chain.contains_key(&epoch) {
                blocks_minted += 1;
                blocks_minted_reward += if wit2_activated && epoch >= wit2_activation_epoch {
                    wit2_block_reward
                } else {
                    wit1_block_reward
                }
            }
        }

        let burnt_supply = self
            .initial_supply
            .saturating_add(blocks_minted_reward)
            .saturating_sub(current_locked_supply)
            .saturating_sub(current_staked_supply)
            .saturating_sub(current_unlocked_supply);

        Ok(SupplyInfo2 {
            epoch: current_epoch,
            current_time,
            blocks_minted,
            blocks_minted_reward,
            burnt_supply,
            current_locked_supply,
            current_staked_supply,
            current_unlocked_supply,
            initial_supply: self.initial_supply,
            requests_in_flight_collateral,
        })
    }
}

impl Handler<GetUtxoInfo> for ChainManager {
    type Result = Result<UtxoInfo, anyhow::Error>;

    fn handle(
        &mut self,
        GetUtxoInfo { pkh }: GetUtxoInfo,
        _ctx: &mut Self::Context,
    ) -> Self::Result {
        if self.sm_state != StateMachine::Synced {
            return Err(ChainManagerError::NotSynced {
                current_state: self.sm_state,
            }
            .into());
        }

        let chain_info = self.chain_state.chain_info.as_ref().unwrap();
        let active_wips = ActiveWips {
            active_wips: self.chain_state.tapi_engine.wip_activation.clone(),
            block_epoch: self.current_epoch.unwrap(),
        };
        let collateral_age = self
            .consensus_constants_wit2
            .get_collateral_age(&active_wips);
        let block_number_limit = self
            .chain_state
            .block_number()
            .saturating_sub(collateral_age);

        let pkh = if self.own_pkh == Some(pkh) {
            None
        } else {
            Some(pkh)
        };

        Ok(get_utxo_info(
            pkh,
            &self.chain_state.own_utxos,
            &self.chain_state.unspent_outputs_pool,
            chain_info.consensus_constants.collateral_minimum,
            block_number_limit,
        ))
    }
}

impl Handler<GetReputation> for ChainManager {
    type Result = Result<GetReputationResult, anyhow::Error>;

    fn handle(
        &mut self,
        GetReputation { pkh, all }: GetReputation,
        _ctx: &mut Self::Context,
    ) -> Self::Result {
        if self.sm_state != StateMachine::Synced {
            return Err(ChainManagerError::NotSynced {
                current_state: self.sm_state,
            }
            .into());
        }

        let rep_eng = match self.chain_state.reputation_engine.as_ref() {
            Some(x) => x,
            None => return Err(ChainManagerError::ChainNotReady.into()),
        };

        let identities = if all {
            // Add identities with reputation > 0
            let mut identities: Vec<_> = rep_eng.trs().identities().map(|(k, _v)| k).collect();
            // Add identitities active but with 0 reputation
            for pkh in rep_eng.ars().active_identities() {
                if rep_eng.trs().get(pkh).0 == 0 {
                    identities.push(pkh);
                }
            }
            identities
        } else {
            vec![&pkh]
        };

        let total_reputation = rep_eng.total_active_reputation();
        let reputation_hm = identities
            .into_iter()
            .map(|pkh| {
                let reputation = rep_eng.trs().get(pkh);
                let eligibility = rep_eng.get_eligibility(pkh) + 1;
                let is_active = rep_eng.ars().contains(pkh);

                let rep_stats = ReputationStats {
                    reputation,
                    eligibility,
                    is_active,
                };

                (*pkh, rep_stats)
            })
            .collect();

        let result = GetReputationResult {
            stats: reputation_hm,
            total_reputation,
        };

        Ok(result)
    }
}

impl Handler<TryMineBlock> for ChainManager {
    type Result = ();

    fn handle(&mut self, _msg: TryMineBlock, ctx: &mut Self::Context) -> Self::Result {
        if let Err(e) = self.try_mine_block(ctx) {
            match e {
                // Lack of eligibility is logged as debug
                e @ ChainManagerError::NotEligible => {
                    log::debug!("{e}");
                }
                // Any other errors are logged as warning (considering that this is a best-effort
                // method)
                e => {
                    log::warn!("{e}");
                }
            }
        }
    }
}

impl Handler<AddCommitReveal> for ChainManager {
    type Result = ResponseActFuture<Self, Result<(), anyhow::Error>>;

    fn handle(
        &mut self,
        AddCommitReveal {
            commit_transaction,
            reveal_transaction,
        }: AddCommitReveal,
        _ctx: &mut Self::Context,
    ) -> Self::Result {
        let dr_pointer = commit_transaction.body.dr_pointer;
        // Hold reveal transaction under "waiting_for_reveal" field of data requests pool
        self.chain_state
            .data_request_pool
            .insert_reveal(dr_pointer, reveal_transaction);

        // Send AddTransaction message to self
        // And broadcast it to all of peers
        Box::pin(
            self.add_transaction(
                AddTransaction {
                    transaction: Transaction::Commit(commit_transaction),
                    broadcast_flag: true,
                },
                get_timestamp(),
            )
            .map_err(|e, _, _| {
                log::warn!("Failed to add commit transaction: {e}");
                e
            }),
        )
    }
}

impl Handler<GetMemoryTransaction> for ChainManager {
    type Result = Result<Transaction, ()>;

    fn handle(&mut self, msg: GetMemoryTransaction, _ctx: &mut Self::Context) -> Self::Result {
        self.transactions_pool.get(&msg.hash).ok_or(())
    }
}

impl Handler<GetMempool> for ChainManager {
    type Result = Result<GetMempoolResult, anyhow::Error>;

    fn handle(&mut self, _msg: GetMempool, _ctx: &mut Self::Context) -> Self::Result {
        let res = GetMempoolResult {
            value_transfer: self.transactions_pool.vt_iter().map(|t| t.hash()).collect(),
            data_request: self.transactions_pool.dr_iter().map(|t| t.hash()).collect(),
            stake: self.transactions_pool.st_iter().map(|t| t.hash()).collect(),
            unstake: self.transactions_pool.ut_iter().map(|t| t.hash()).collect(),
        };

        Ok(res)
    }
}

impl Handler<AddSuperBlock> for ChainManager {
    type Result = ();

    fn handle(&mut self, msg: AddSuperBlock, _ctx: &mut Self::Context) -> Self::Result {
        let received_superblock_hash = msg.superblock.hash();
        if let Some(sync_target) = &self.sync_target {
            let target_superblock_hash = sync_target.superblock.hash_prev_block;

            if target_superblock_hash == received_superblock_hash {
                self.sync_superblock = Some((received_superblock_hash, msg.superblock));
            } else {
                log::debug!(
                    "Received superblock {received_superblock_hash} when expecting superblock {target_superblock_hash}"
                );
            }
        } else {
            log::debug!(
                "Received superblock {received_superblock_hash} when expecting no superblock"
            );
        }
    }
}

impl Handler<IsConfirmedBlock> for ChainManager {
    type Result = Result<bool, anyhow::Error>;

    fn handle(&mut self, msg: IsConfirmedBlock, _ctx: &mut Self::Context) -> Self::Result {
        let superblock_period = self
            .chain_state
            .chain_info
            .as_ref()
            .ok_or(ChainManagerError::ChainNotReady)?
            .consensus_constants
            .superblock_period;
        let superblock_beacon = self.get_superblock_beacon();
        // Superblock 1 confirms blocks 0..=9
        let last_confirmed_block =
            (superblock_beacon.checkpoint * u32::from(superblock_period)).saturating_sub(1);

        if msg.block_epoch <= last_confirmed_block {
            if self.chain_state.block_chain.get(&msg.block_epoch) == Some(&msg.block_hash) {
                // Block hash matches, good
                Ok(true)
            } else {
                // This is a forked block that will never be valid
                Ok(false)
            }
        } else {
            // block_epoch > last_confirmed_block, this block is not confirmed yet
            Ok(false)
        }
    }
}

impl Handler<Rewind> for ChainManager {
    type Result = Result<bool, anyhow::Error>;

    fn handle(&mut self, msg: Rewind, ctx: &mut Self::Context) -> Self::Result {
        // Save list of blocks that are known to be valid
        let old_block_chain: VecDeque<(Epoch, Hash)> = self
            .chain_state
            .block_chain
            .range(0..=msg.epoch)
            .map(|(k, v)| (*k, *v))
            .collect();

        self.delete_chain_state_and_reinitialize()
            .map(|_res, act, ctx| {
                // Set outbound limit to 0
                // This will avoid receiving any messages that could interfere with the
                // resynchronization.
                let sessions_manager = SessionsManager::from_registry();
                sessions_manager
                    .send(SetPeersLimits {
                        inbound: 0,
                        outbound: 0,
                    })
                    .into_actor(act)
                    .map(|_res, _act, _ctx| ())
                    .spawn(ctx);
                act.resync_from_storage(old_block_chain, ctx, |act, ctx| {
                    // After the resync is done:
                    // Persist chain state to storage
                    ctx.wait(
                        act.persist_chain_state(None)
                            .map(|_res: Result<(), ()>, _act, _ctx| ()),
                    );
                    // Set outbound limit back to the old value
                    async {
                        let config = config_mngr::get().await.expect("failed to read config");
                        let sessions_manager = SessionsManager::from_registry();
                        sessions_manager
                            .send(SetPeersLimits {
                                inbound: config.connections.inbound_limit,
                                outbound: config.connections.outbound_limit,
                            })
                            .await
                            .expect("failed to set peers limits");
                    }
                    .into_actor(act)
                    .spawn(ctx);
                });
            })
            .wait(ctx);

        Ok(true)
    }
}

impl Handler<GetSignalingInfo> for ChainManager {
    type Result = Result<SignalingInfo, anyhow::Error>;

    fn handle(&mut self, _msg: GetSignalingInfo, _ctx: &mut Self::Context) -> Self::Result {
        let active_upgrades = self.chain_state.tapi_engine.wip_activation.clone();
        let pending_upgrades = self
            .chain_state
            .tapi_engine
            .bit_tapi_counter
            .info(&active_upgrades);
        let epoch = self.chain_state.tapi_engine.bit_tapi_counter.last_epoch();
        Ok(SignalingInfo {
            active_upgrades,
            pending_upgrades,
            epoch,
        })
    }
}

impl Handler<EstimatePriority> for ChainManager {
    type Result = <EstimatePriority as Message>::Result;

    fn handle(&mut self, _msg: EstimatePriority, _ctx: &mut Self::Context) -> Self::Result {
        // Prevent estimating priority if chain is not synchronized
        match self.sm_state {
            StateMachine::Synced => {
                let seconds_per_epoch = Duration::from_secs(
                    self.epoch_constants
                        .ok_or(ChainManagerError::ChainNotReady)?
                        .checkpoints_period
                        .into(),
                );

                Ok(self.priority_engine.estimate_priority(seconds_per_epoch)?)
            }
            current_state => Err(ChainManagerError::NotSynced { current_state }.into()),
        }
    }
}

impl Handler<GetProtocolInfo> for ChainManager {
    type Result = <GetProtocolInfo as Message>::Result;

    fn handle(&mut self, _msg: GetProtocolInfo, _ctx: &mut Self::Context) -> Self::Result {
        Ok(Some(get_protocol_info()))
    }
}

impl Handler<SnapshotExport> for ChainManager {
    type Result = ResponseActFuture<Self, <SnapshotExport as Message>::Result>;

    fn handle(
        &mut self,
        SnapshotExport { path }: SnapshotExport,
        _ctx: &mut Self::Context,
    ) -> Self::Result {
        let fut = Self::static_snapshot_export(self.sm_state, self.chain_state.clone(), path);

        Box::pin(actix::fut::wrap_future(fut))
    }
}

impl Handler<SnapshotImport> for ChainManager {
    type Result = ResponseActFuture<Self, <SnapshotImport as Message>::Result>;

    fn handle(
        &mut self,
        SnapshotImport { path }: SnapshotImport,
        _ctx: &mut Self::Context,
    ) -> Self::Result {
        // Put the import info into place, and trigger the import
        self.put_import_from_path(path);
        let fut = self.snapshot_import();

        Box::pin(fut)
    }
}

#[derive(Debug, Eq, PartialEq)]
pub enum BlockBatches<T> {
    TargetNotReached(Vec<T>),
    SyncWithoutCandidate(Vec<T>, Vec<T>),
    SyncWithCandidate(Vec<T>, Vec<T>, Vec<T>),
}

fn split_blocks_batch_at_target<T, F>(
    key: F,
    blocks: Vec<T>,
    current_epoch: u32,
    sync_target: &SyncTarget,
    superblock_period: u32,
) -> Result<BlockBatches<T>, ChainManagerError>
where
    F: Fn(&T) -> u32 + Copy,
{
    use BlockBatches::*;

    let current_superblock_index = current_epoch / superblock_period;
    assert!(
        current_superblock_index >= sync_target.superblock.checkpoint,
        "Provided a sync target (epoch: {}, superblock index: {}) that is in the future (epoch: {}, superblock index: {})",
        current_epoch,
        current_superblock_index,
        sync_target.block.checkpoint,
        sync_target.superblock.checkpoint,
    );

    // If the chain reverted, this function cannot receive blocks from between the reverted epochs
    let first_valid_block = (current_superblock_index
        - ((current_superblock_index - sync_target.superblock.checkpoint) % 2))
        * superblock_period;

    let wrong_index = blocks.iter().position(|block| {
        key(block) >= sync_target.superblock.checkpoint * superblock_period
            && key(block) < first_valid_block
    });

    if let Some(wrong_index) = wrong_index {
        // We received blocks that do not match the current epoch and the last consolidated superblock.
        // As an example, if the last consolidated superblock has the block 9 inside, and we are in epoch 50,
        // it means we reverted somehow.
        // Sync target 0 is excluded from this validation
        if sync_target.superblock.checkpoint != 0 {
            return Err(ChainManagerError::WrongBlocksForSuperblock {
                wrong_index: key(&blocks[wrong_index]),
                consolidated_superblock_index: sync_target.superblock.checkpoint,
                current_superblock_index,
            });
        }
    }

    // The case where blocks is an empty array
    let last_epoch = blocks.last().map(key).unwrap_or(0);

    if last_epoch < ((sync_target.superblock.checkpoint * superblock_period).saturating_sub(1))
        && last_epoch < sync_target.block.checkpoint
    {
        return Ok(TargetNotReached(blocks));
    }

    if (current_superblock_index - sync_target.superblock.checkpoint) % 2 == 0 {
        let consolidated_blocks_target = sync_target.superblock.checkpoint * superblock_period;
        let mut consolidated_blocks = blocks;
        let mut remaining_blocks = vec![];
        let split_position = consolidated_blocks
            .iter()
            .position(|block| key(block) >= consolidated_blocks_target);
        if let Some(split_position) = split_position {
            remaining_blocks = consolidated_blocks.split_off(split_position);
        }

        return Ok(SyncWithoutCandidate(consolidated_blocks, remaining_blocks));
    }

    let (consolidated_blocks_target, candidate_blocks_target) = (
        sync_target.superblock.checkpoint * superblock_period,
        current_superblock_index * superblock_period,
    );
    let mut consolidated_blocks = blocks;
    let candidate_split_position = consolidated_blocks
        .iter()
        .position(|block| key(block) >= consolidated_blocks_target);

    let mut candidate_blocks = vec![];
    let mut remaining_blocks = vec![];

    if let Some(candidate_split_position) = candidate_split_position {
        candidate_blocks = consolidated_blocks.split_off(candidate_split_position);
    }

    let remaining_split_position = candidate_blocks
        .iter()
        .position(|block| key(block) >= candidate_blocks_target);
    if let Some(remaining_split_position) = remaining_split_position {
        remaining_blocks = candidate_blocks.split_off(remaining_split_position);
    }

    Ok(SyncWithCandidate(
        consolidated_blocks,
        candidate_blocks,
        remaining_blocks,
    ))
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use super::*;

    #[test]
    fn peers_beacons_consensus_less_peers_than_outbound() {
        let beacon1 = LastBeacon {
            highest_block_checkpoint: CheckpointBeacon {
                checkpoint: 1,
                hash_prev_block: Hash::default(),
            },
            highest_superblock_checkpoint: CheckpointBeacon {
                checkpoint: 0,
                hash_prev_block: "6b86b273ff34fce19d6b804eff5a3f5747ada4eaa22f1d49c01e52ddb7875b4b"
                    .parse()
                    .unwrap(),
            },
        };
        let beacon2 = LastBeacon {
            highest_block_checkpoint: CheckpointBeacon {
                checkpoint: 1,
                hash_prev_block: Hash::default(),
            },
            highest_superblock_checkpoint: CheckpointBeacon {
                checkpoint: 0,
                hash_prev_block: "d4735e3a265e16eee03f59718b9b5d03019c07d8b6c51f90da3a666eec13ab35"
                    .parse()
                    .unwrap(),
            },
        };

        // 0 peers
        let peers_beacons = PeersBeacons {
            pb: vec![],
            outbound_limit: Some(4),
        };
        assert_eq!(peers_beacons.superblock_consensus(60), None);

        // 1 peer
        let peers_beacons = PeersBeacons {
            pb: vec![("127.0.0.1:10001".parse().unwrap(), Some(beacon1.clone()))],
            outbound_limit: Some(4),
        };
        assert_eq!(peers_beacons.superblock_consensus(60), None);

        // 2 peers
        let peers_beacons = PeersBeacons {
            pb: vec![
                ("127.0.0.1:10001".parse().unwrap(), Some(beacon1.clone())),
                ("127.0.0.1:10002".parse().unwrap(), Some(beacon1.clone())),
            ],
            outbound_limit: Some(4),
        };
        assert_eq!(peers_beacons.superblock_consensus(60), None);

        // 3 peers and 2 agree
        let peers_beacons = PeersBeacons {
            pb: vec![
                ("127.0.0.1:10001".parse().unwrap(), Some(beacon1.clone())),
                ("127.0.0.1:10002".parse().unwrap(), Some(beacon1.clone())),
                ("127.0.0.1:10003".parse().unwrap(), Some(beacon2.clone())),
            ],
            outbound_limit: Some(4),
        };
        // Note that the consensus % includes the missing peers,
        // so in this case it is 2/4 (50%), not 2/3 (66%), so there is no consensus for 60%
        assert_eq!(peers_beacons.superblock_consensus(60), None);

        // 3 peers and 3 agree
        let peers_beacons = PeersBeacons {
            pb: vec![
                ("127.0.0.1:10001".parse().unwrap(), Some(beacon1.clone())),
                ("127.0.0.1:10002".parse().unwrap(), Some(beacon1.clone())),
                ("127.0.0.1:10003".parse().unwrap(), Some(beacon1.clone())),
            ],
            outbound_limit: Some(4),
        };
        assert_eq!(
            peers_beacons.superblock_consensus(60),
            Some((beacon1.clone(), true))
        );

        // 4 peers and 2 agree
        let peers_beacons = PeersBeacons {
            pb: vec![
                ("127.0.0.1:10001".parse().unwrap(), Some(beacon1.clone())),
                ("127.0.0.1:10002".parse().unwrap(), Some(beacon1.clone())),
                ("127.0.0.1:10003".parse().unwrap(), Some(beacon2.clone())),
                ("127.0.0.1:10004".parse().unwrap(), Some(beacon2.clone())),
            ],
            outbound_limit: Some(4),
        };
        assert_eq!(peers_beacons.superblock_consensus(60), None);

        // 4 peers and 3 agree
        let peers_beacons = PeersBeacons {
            pb: vec![
                ("127.0.0.1:10001".parse().unwrap(), Some(beacon1.clone())),
                ("127.0.0.1:10002".parse().unwrap(), Some(beacon1.clone())),
                ("127.0.0.1:10003".parse().unwrap(), Some(beacon1.clone())),
                ("127.0.0.1:10004".parse().unwrap(), Some(beacon2)),
            ],
            outbound_limit: Some(4),
        };
        assert_eq!(
            peers_beacons.superblock_consensus(60),
            Some((beacon1.clone(), true))
        );

        // 4 peers and 4 agree
        let peers_beacons = PeersBeacons {
            pb: vec![
                ("127.0.0.1:10001".parse().unwrap(), Some(beacon1.clone())),
                ("127.0.0.1:10002".parse().unwrap(), Some(beacon1.clone())),
                ("127.0.0.1:10003".parse().unwrap(), Some(beacon1.clone())),
                ("127.0.0.1:10004".parse().unwrap(), Some(beacon1.clone())),
            ],
            outbound_limit: Some(4),
        };
        assert_eq!(
            peers_beacons.superblock_consensus(60),
            Some((beacon1, true))
        );
    }

    #[test]
    fn test_superblock_consensus() {
        let hash_1 =
            Hash::from_str("1111111111111111111111111111111111111111111111111111111111111111")
                .unwrap();
        let hash_2 =
            Hash::from_str("1111111111111111111111111111111111111111111111111111111111111112")
                .unwrap();
        let beacon1 = LastBeacon {
            highest_block_checkpoint: CheckpointBeacon {
                checkpoint: 1,
                hash_prev_block: hash_1,
            },
            highest_superblock_checkpoint: CheckpointBeacon {
                checkpoint: 0,
                hash_prev_block: "6b86b273ff34fce19d6b804eff5a3f5747ada4eaa22f1d49c01e52ddb7875b4b"
                    .parse()
                    .unwrap(),
            },
        };
        let beacon2 = LastBeacon {
            highest_block_checkpoint: CheckpointBeacon {
                checkpoint: 1,
                hash_prev_block: hash_2,
            },
            highest_superblock_checkpoint: CheckpointBeacon {
                checkpoint: 0,
                hash_prev_block: "6b86b273ff34fce19d6b804eff5a3f5747ada4eaa22f1d49c01e52ddb7875b4b"
                    .parse()
                    .unwrap(),
            },
        };

        let beacon3 = LastBeacon {
            highest_block_checkpoint: CheckpointBeacon {
                checkpoint: 1,
                hash_prev_block: Hash::default(),
            },
            highest_superblock_checkpoint: CheckpointBeacon {
                checkpoint: 0,
                hash_prev_block: "d4735e3a265e16eee03f59718b9b5d03019c07d8b6c51f90da3a666eec13ab35"
                    .parse()
                    .unwrap(),
            },
        };

        // 12 peers (SB 12/12, B 8/12)
        let peers_beacons = PeersBeacons {
            pb: vec![
                ("127.0.0.1:10001".parse().unwrap(), Some(beacon1.clone())),
                ("127.0.0.1:10002".parse().unwrap(), Some(beacon1.clone())),
                ("127.0.0.1:10003".parse().unwrap(), Some(beacon1.clone())),
                ("127.0.0.1:10004".parse().unwrap(), Some(beacon1.clone())),
                ("127.0.0.1:10005".parse().unwrap(), Some(beacon1.clone())),
                ("127.0.0.1:10006".parse().unwrap(), Some(beacon1.clone())),
                ("127.0.0.1:10007".parse().unwrap(), Some(beacon1.clone())),
                ("127.0.0.1:10008".parse().unwrap(), Some(beacon1.clone())),
                ("127.0.0.1:10009".parse().unwrap(), Some(beacon2.clone())),
                ("127.0.0.1:10010".parse().unwrap(), Some(beacon2.clone())),
                ("127.0.0.1:10011".parse().unwrap(), Some(beacon2.clone())),
                ("127.0.0.1:10012".parse().unwrap(), Some(beacon2.clone())),
            ],
            outbound_limit: Some(12),
        };
        assert_eq!(
            peers_beacons.superblock_consensus(60),
            Some((beacon1.clone(), true))
        );

        // 12 peers (SB 12/12, B 7/12)
        let peers_beacons = PeersBeacons {
            pb: vec![
                ("127.0.0.1:10001".parse().unwrap(), Some(beacon1.clone())),
                ("127.0.0.1:10002".parse().unwrap(), Some(beacon1.clone())),
                ("127.0.0.1:10003".parse().unwrap(), Some(beacon1.clone())),
                ("127.0.0.1:10004".parse().unwrap(), Some(beacon1.clone())),
                ("127.0.0.1:10005".parse().unwrap(), Some(beacon1.clone())),
                ("127.0.0.1:10006".parse().unwrap(), Some(beacon1.clone())),
                ("127.0.0.1:10007".parse().unwrap(), Some(beacon1.clone())),
                ("127.0.0.1:10008".parse().unwrap(), Some(beacon2.clone())),
                ("127.0.0.1:10009".parse().unwrap(), Some(beacon2.clone())),
                ("127.0.0.1:10010".parse().unwrap(), Some(beacon2.clone())),
                ("127.0.0.1:10011".parse().unwrap(), Some(beacon2.clone())),
                ("127.0.0.1:10012".parse().unwrap(), Some(beacon2.clone())),
            ],
            outbound_limit: Some(12),
        };
        assert_eq!(
            peers_beacons.superblock_consensus(60),
            Some((beacon1.clone(), false))
        );

        // 12 peers (SB 12/12, B 7/12 with 1 missing)
        let peers_beacons = PeersBeacons {
            pb: vec![
                ("127.0.0.1:10001".parse().unwrap(), Some(beacon1.clone())),
                ("127.0.0.1:10002".parse().unwrap(), Some(beacon1.clone())),
                ("127.0.0.1:10003".parse().unwrap(), Some(beacon1.clone())),
                ("127.0.0.1:10004".parse().unwrap(), Some(beacon1.clone())),
                ("127.0.0.1:10005".parse().unwrap(), Some(beacon1.clone())),
                ("127.0.0.1:10006".parse().unwrap(), Some(beacon1.clone())),
                ("127.0.0.1:10007".parse().unwrap(), Some(beacon1.clone())),
                ("127.0.0.1:10008".parse().unwrap(), Some(beacon2.clone())),
                ("127.0.0.1:10009".parse().unwrap(), Some(beacon2.clone())),
                ("127.0.0.1:10010".parse().unwrap(), Some(beacon2.clone())),
                ("127.0.0.1:10011".parse().unwrap(), Some(beacon2)),
                ("127.0.0.1:10012".parse().unwrap(), None),
            ],
            outbound_limit: Some(12),
        };
        assert_eq!(
            peers_beacons.superblock_consensus(60),
            Some((beacon1.clone(), false))
        );

        // 12 peers (SB 7/12, B 7/12)
        let peers_beacons = PeersBeacons {
            pb: vec![
                ("127.0.0.1:10001".parse().unwrap(), Some(beacon1.clone())),
                ("127.0.0.1:10002".parse().unwrap(), Some(beacon1.clone())),
                ("127.0.0.1:10003".parse().unwrap(), Some(beacon1.clone())),
                ("127.0.0.1:10004".parse().unwrap(), Some(beacon1.clone())),
                ("127.0.0.1:10005".parse().unwrap(), Some(beacon1.clone())),
                ("127.0.0.1:10006".parse().unwrap(), Some(beacon1.clone())),
                ("127.0.0.1:10007".parse().unwrap(), Some(beacon1)),
                ("127.0.0.1:10008".parse().unwrap(), Some(beacon3.clone())),
                ("127.0.0.1:10009".parse().unwrap(), Some(beacon3.clone())),
                ("127.0.0.1:10010".parse().unwrap(), Some(beacon3.clone())),
                ("127.0.0.1:10011".parse().unwrap(), Some(beacon3.clone())),
                ("127.0.0.1:10012".parse().unwrap(), Some(beacon3)),
            ],
            outbound_limit: Some(12),
        };
        assert_eq!(peers_beacons.superblock_consensus(60), None);
    }

    #[test]
    fn test_unregister_peers() {
        let beacon1 = LastBeacon {
            highest_block_checkpoint: CheckpointBeacon {
                checkpoint: 1,
                hash_prev_block: "6b86b273ff34fce19d6b804eff5a3f5747ada4eaa22f1d49c01e52ddb7875b4b"
                    .parse()
                    .unwrap(),
            },
            highest_superblock_checkpoint: CheckpointBeacon {
                checkpoint: 0,
                hash_prev_block: Hash::default(),
            },
        };
        let beacon2 = LastBeacon {
            highest_block_checkpoint: CheckpointBeacon {
                checkpoint: 1,
                hash_prev_block: "d4735e3a265e16eee03f59718b9b5d03019c07d8b6c51f90da3a666eec13ab35"
                    .parse()
                    .unwrap(),
            },
            highest_superblock_checkpoint: CheckpointBeacon {
                checkpoint: 0,
                hash_prev_block: Hash::default(),
            },
        };

        // 0 peers
        let mut peers_beacons = PeersBeacons {
            pb: vec![],
            outbound_limit: Some(4),
        };
        assert_eq!(
            peers_beacons.decide_peers_to_unregister(beacon1.highest_block_checkpoint),
            []
        );

        // 1 peer in consensus
        peers_beacons = PeersBeacons {
            pb: vec![("127.0.0.1:10001".parse().unwrap(), Some(beacon1.clone()))],
            outbound_limit: Some(4),
        };
        assert_eq!(
            peers_beacons.decide_peers_to_unregister(beacon1.highest_block_checkpoint),
            []
        );

        // 1 peer out of consensus
        peers_beacons = PeersBeacons {
            pb: vec![("127.0.0.1:10001".parse().unwrap(), Some(beacon1.clone()))],
            outbound_limit: Some(4),
        };
        assert_eq!(
            peers_beacons.decide_peers_to_unregister(beacon2.highest_block_checkpoint),
            ["127.0.0.1:10001".parse().unwrap()]
        );

        peers_beacons = PeersBeacons {
            pb: vec![
                ("127.0.0.1:10001".parse().unwrap(), Some(beacon1.clone())),
                ("127.0.0.1:10002".parse().unwrap(), Some(beacon1.clone())),
                ("127.0.0.1:10003".parse().unwrap(), Some(beacon2.clone())),
                ("127.0.0.1:10004".parse().unwrap(), Some(beacon2.clone())),
            ],
            outbound_limit: Some(4),
        };
        assert_eq!(
            peers_beacons.decide_peers_to_unregister(beacon2.highest_block_checkpoint),
            [
                "127.0.0.1:10001".parse().unwrap(),
                "127.0.0.1:10002".parse().unwrap()
            ]
        );

        peers_beacons = PeersBeacons {
            pb: vec![
                ("127.0.0.1:10001".parse().unwrap(), Some(beacon1.clone())),
                ("127.0.0.1:10002".parse().unwrap(), Some(beacon1.clone())),
                ("127.0.0.1:10003".parse().unwrap(), None),
                ("127.0.0.1:10004".parse().unwrap(), None),
            ],
            outbound_limit: Some(4),
        };
        assert_eq!(
            peers_beacons.decide_peers_to_unregister(beacon1.highest_block_checkpoint),
            [
                "127.0.0.1:10003".parse().unwrap(),
                "127.0.0.1:10004".parse().unwrap()
            ]
        );

        peers_beacons = PeersBeacons {
            pb: vec![
                ("127.0.0.1:10001".parse().unwrap(), None),
                ("127.0.0.1:10002".parse().unwrap(), None),
                ("127.0.0.1:10003".parse().unwrap(), None),
                ("127.0.0.1:10004".parse().unwrap(), None),
            ],
            outbound_limit: Some(4),
        };
        assert_eq!(
            peers_beacons.decide_peers_to_unregister(beacon1.highest_block_checkpoint),
            [
                "127.0.0.1:10001".parse().unwrap(),
                "127.0.0.1:10002".parse().unwrap(),
                "127.0.0.1:10003".parse().unwrap(),
                "127.0.0.1:10004".parse().unwrap()
            ]
        );
    }
    #[test]
    fn test_split_blocks_batch() {
        use BlockBatches::*;
        let mut sync_target = SyncTarget {
            block: Default::default(),
            superblock: Default::default(),
        };
        let superblock_period = 10;

        let test_split_batch = |provided_blocks, epoch, sync_target: &SyncTarget| {
            split_blocks_batch_at_target(
                |x| *x,
                provided_blocks,
                epoch,
                &sync_target.clone(),
                superblock_period,
            )
        };

        assert_eq!(
            test_split_batch(vec![], 1, &sync_target),
            Ok(SyncWithoutCandidate(vec![], vec![]))
        );
        assert_eq!(
            test_split_batch(vec![0], 1, &sync_target),
            Ok(SyncWithoutCandidate(vec![], vec![0]))
        );
        assert_eq!(
            test_split_batch(vec![0, 8], 9, &sync_target),
            Ok(SyncWithoutCandidate(vec![], vec![0, 8]))
        );
        assert_eq!(
            test_split_batch(vec![0, 9], 11, &sync_target),
            Ok(SyncWithCandidate(vec![], vec![0, 9], vec![]))
        );

        assert_eq!(
            test_split_batch(vec![0, 10], 11, &sync_target),
            Ok(SyncWithCandidate(vec![], vec![0], vec![10]))
        );

        sync_target.superblock.checkpoint = 1;

        assert_eq!(
            test_split_batch(vec![0, 9], 21, &sync_target),
            Ok(SyncWithCandidate(vec![0, 9], vec![], vec![]))
        );
        assert_eq!(
            test_split_batch(vec![0, 10], 21, &sync_target),
            Ok(SyncWithCandidate(vec![0], vec![10], vec![]))
        );
        assert_eq!(
            test_split_batch(vec![0, 8, 11], 21, &sync_target),
            Ok(SyncWithCandidate(vec![0, 8], vec![11], vec![]))
        );
        assert_eq!(
            test_split_batch(vec![0, 9, 10, 18, 26], 29, &sync_target),
            Ok(SyncWithCandidate(vec![0, 9], vec![10, 18], vec![26]))
        );
        assert_eq!(
            test_split_batch(vec![0, 9, 10, 19], 21, &sync_target,),
            Ok(SyncWithCandidate(vec![0, 9], vec![10, 19], vec![]))
        );
        assert_eq!(
            test_split_batch(vec![0, 10, 20], 21, &sync_target),
            Ok(SyncWithCandidate(vec![0], vec![10], vec![20]))
        );
        assert_eq!(
            test_split_batch(vec![0, 9, 10, 19, 20, 21], 22, &sync_target,),
            Ok(SyncWithCandidate(vec![0, 9], vec![10, 19], vec![20, 21]))
        );

        sync_target.superblock.checkpoint = 2;
        assert_eq!(
            test_split_batch(vec![100], 101, &sync_target),
            Ok(SyncWithoutCandidate(vec![], vec![100]))
        );

        assert_eq!(
            test_split_batch(vec![110], 111, &sync_target),
            Ok(SyncWithCandidate(vec![], vec![], vec![110]))
        );

        assert_eq!(
            test_split_batch(vec![105, 110], 111, &sync_target),
            Ok(SyncWithCandidate(vec![], vec![105], vec![110]))
        );

        assert_eq!(
            test_split_batch(vec![], 111, &sync_target),
            Ok(SyncWithCandidate(vec![], vec![], vec![]))
        );

        assert_eq!(
            test_split_batch(vec![], 111, &sync_target),
            Ok(SyncWithCandidate(vec![], vec![], vec![]))
        );

        assert_eq!(
            test_split_batch(vec![1, 8, 18, 108, 110], 111, &sync_target),
            Ok(SyncWithCandidate(vec![1, 8, 18], vec![108], vec![110]))
        );

        sync_target.superblock.checkpoint = 3;
        assert_eq!(
            test_split_batch(vec![1, 8, 18, 70, 100], 101, &sync_target),
            (Err(ChainManagerError::WrongBlocksForSuperblock {
                wrong_index: 70,
                consolidated_superblock_index: 3,
                current_superblock_index: 10
            }))
        );

        sync_target.superblock.checkpoint = 10;
        sync_target.block.checkpoint = 99;

        assert_eq!(
            test_split_batch(vec![1, 8, 18], 101, &sync_target),
            Ok(TargetNotReached(vec![1, 8, 18]))
        );
    }
}
