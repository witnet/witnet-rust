use actix::{fut::WrapFuture, prelude::*};
use futures::future::Future;
use std::{
    collections::BTreeMap,
    collections::{HashMap, HashSet},
    convert::TryFrom,
    net::SocketAddr,
};

use witnet_data_structures::{
    chain::{
        get_utxo_info, ChainState, CheckpointBeacon, DataRequestInfo, DataRequestReport, Epoch,
        Hash, Hashable, NodeStats, PublicKeyHash, Reputation, SuperBlockVote, UtxoInfo,
    },
    error::{ChainInfoError, TransactionError::DataRequestNotFound},
    transaction::{DRTransaction, Transaction, VTTransaction},
    types::LastBeacon,
};
use witnet_util::timestamp::get_timestamp;
use witnet_validations::validations::validate_rad_request;

use super::{
    show_sync_progress, transaction_factory, ChainManager, ChainManagerError, StateMachine,
    SyncTarget,
};
use crate::{
    actors::{
        chain_manager::BlockCandidate,
        messages::{
            AddBlocks, AddCandidates, AddCommitReveal, AddSuperBlockVote, AddTransaction,
            Broadcast, BuildDrt, BuildVtt, EpochNotification, GetBalance, GetBlocksEpochRange,
            GetDataRequestReport, GetHighestCheckpointBeacon, GetMemoryTransaction, GetMempool,
            GetMempoolResult, GetNodeStats, GetReputation, GetReputationAll, GetReputationStatus,
            GetReputationStatusResult, GetState, GetSuperBlockVotes, GetUtxoInfo, PeersBeacons,
            SendLastBeacon, SessionUnitResult, SetLastBeacon, TryMineBlock,
        },
        sessions_manager::SessionsManager,
    },
    storage_mngr,
    utils::mode_consensus,
};

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
            current_timestamp as i64 - msg.timestamp as i64,
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
        self.transactions_pool.clear_pending_transactions();

        // Handle case consensus not achieved
        if !self.peers_beacons_received {
            log::warn!("No beacon messages received from peers. Moving to WaitingConsensus state");
            self.sm_state = StateMachine::WaitingConsensus;
            // Clear candidates
            self.candidates.clear();
            self.seen_candidates.clear();
        }

        if let Some(last_checked_epoch) = last_checked_epoch {
            if msg.checkpoint - last_checked_epoch != 1 {
                log::warn!(
                    "Missed epoch notification {}. Moving to WaitingConsensus state",
                    last_checked_epoch + 1
                );
                self.sm_state = StateMachine::WaitingConsensus;
            }
        }

        self.peers_beacons_received = false;
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
                        if self.epoch_constants.is_none()
                            || self.vrf_ctx.is_none()
                            || self.secp.is_none()
                        {
                            log::error!("{}", ChainManagerError::ChainNotReady);
                            return;
                        }

                        // Consolidate the best candidate
                        if let Some(BlockCandidate {
                            block,
                            utxo_diff,
                            reputation: _,
                            vrf_proof: _,
                        }) = self.best_candidate.take()
                        {
                            // Persist block and update ChainState
                            self.consolidate_block(ctx, block, utxo_diff);
                        } else if msg.checkpoint > 0 {
                            let previous_epoch = msg.checkpoint - 1;
                            log::warn!(
                                "There was no valid block candidate to consolidate for epoch {}",
                                previous_epoch
                            );
                        }

                        // Send last beacon on block consolidation
                        let sessions_manager = SessionsManager::from_registry();
                        let beacon = self
                            .chain_state
                            .chain_info
                            .as_ref()
                            .unwrap()
                            .highest_block_checkpoint;
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
                        if self.mining_enabled {
                            // Block mining is now triggered by SessionsManager on peers beacon timeout
                            // Data request mining MUST finish BEFORE the block has been mined!!!!
                            // The transactions must be included into this block, both the transactions from
                            // our node and the transactions from other nodes
                            self.try_mine_data_request(ctx);
                        }

                        // Clear candidates
                        self.candidates.clear();
                        self.seen_candidates.clear();

                        log::debug!(
                            "Transactions pool size: {} value transfer, {} data request",
                            self.transactions_pool.vt_len(),
                            self.transactions_pool.dr_len()
                        );
                    }

                    _ => {
                        log::error!("No ChainInfo loaded in ChainManager");
                    }
                }
            }
        }

        self.peers_beacons_received = false;
    }
}

/// Handler for GetHighestBlockCheckpoint message
impl Handler<GetHighestCheckpointBeacon> for ChainManager {
    type Result = Result<CheckpointBeacon, failure::Error>;

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
    type Result = Result<HashSet<SuperBlockVote>, failure::Error>;

    fn handle(&mut self, _msg: GetSuperBlockVotes, _ctx: &mut Context<Self>) -> Self::Result {
        Ok(self
            .chain_state
            .superblock_state
            .get_current_superblock_votes())
    }
}

/// Handler for GetNodeStats message
impl Handler<GetNodeStats> for ChainManager {
    type Result = Result<NodeStats, failure::Error>;

    fn handle(&mut self, _msg: GetNodeStats, _ctx: &mut Context<Self>) -> Self::Result {
        Ok(self.chain_state.node_stats.clone())
    }
}

/// Handler for AddBlocks message
impl Handler<AddBlocks> for ChainManager {
    type Result = SessionUnitResult;

    #[allow(clippy::cognitive_complexity)]
    fn handle(&mut self, mut msg: AddBlocks, ctx: &mut Context<Self>) {
        log::debug!(
            "AddBlocks received while StateMachine is in state {:?}",
            self.sm_state
        );

        let consensus_constants = self.consensus_constants();

        match self.sm_state {
            StateMachine::WaitingConsensus => {
                // In WaitingConsensus state, only allow AddBlocks when the argument is
                // the genesis block
                if msg.blocks.len() == 1 && msg.blocks[0].hash() == consensus_constants.genesis_hash
                {
                    let block = msg.blocks.into_iter().next().unwrap();
                    match self.process_requested_block(ctx, block) {
                        Ok(()) => {
                            log::debug!("Successfully consolidated genesis block");

                            // Set last beacon because otherwise the network cannot bootstrap
                            let sessions_manager = SessionsManager::from_registry();
                            let last_beacon = LastBeacon {
                                highest_block_checkpoint: self.get_chain_beacon(),
                                highest_superblock_checkpoint: self.get_superblock_beacon(),
                            };
                            sessions_manager.do_send(SetLastBeacon {
                                beacon: last_beacon,
                            });
                        }
                        Err(e) => log::error!("Failed to consolidate genesis block: {}", e),
                    }
                }
            }
            StateMachine::Synchronizing => {
                if let Some(sync_target) = self.sync_target.clone() {
                    let mut batch_succeeded = true;
                    let mut i = 0;
                    let mut target_reached = false;
                    let chain_beacon = self.get_chain_beacon();
                    let superblock_period = u32::from(consensus_constants.superblock_period);
                    if msg.blocks.is_empty() {
                        batch_succeeded = false;
                        log::debug!("Received an empty AddBlocks message");
                    // FIXME(#684): this condition would be modified when genesis block exist
                    } else if chain_beacon.hash_prev_block != consensus_constants.bootstrap_hash
                        && msg.blocks[0].hash() != chain_beacon.hash_prev_block
                        && msg.blocks[0].block_header.beacon.checkpoint == chain_beacon.checkpoint
                    {
                        // Fork case
                        batch_succeeded = false;
                        log::error!("Mismatching blocks, fork detected");
                        self.initialize_from_storage(ctx);
                        log::info!("Restored chain state from storage");
                    } else {
                        // FIXME(#684): this condition would be deleted when genesis block exist
                        if chain_beacon.hash_prev_block == consensus_constants.bootstrap_hash
                            || msg.blocks[0].block_header.beacon.checkpoint
                                > chain_beacon.checkpoint
                        {
                        } else {
                            // Ignore genesis block
                            // TODO: this remove may be expensive, find a better way
                            msg.blocks.remove(0);
                        };

                        for block in msg.blocks.iter() {
                            i += 1;
                            let block_epoch = block.block_header.beacon.checkpoint;

                            // TODO: stop processing blocks according to the superblock target,
                            // then verify that the superblock is correct, persist the chain state
                            // and keep processing the remaining blocks. The remaining blocks
                            // should not be persisted.
                            // TODO: saturating_sub?
                            // No, the target superblock cannot be the superblock with index 0
                            let epoch_of_the_last_block_according_to_target_superblock =
                                sync_target.superblock.checkpoint * superblock_period - 1;
                            if block_epoch > epoch_of_the_last_block_according_to_target_superblock
                            {
                                log::debug!("Sync done up to block #{} because that is the last checkpoint according to superblock #{}", epoch_of_the_last_block_according_to_target_superblock, sync_target.superblock.checkpoint);
                                i -= 1;
                                target_reached = true;
                                break;
                            }

                            if let Err(e) = self.process_requested_block(ctx, block.clone()) {
                                log::error!("Error processing block: {}", e);
                                self.initialize_from_storage(ctx);
                                log::info!("Restored chain state from storage");
                                batch_succeeded = false;
                                break;
                            }

                            let beacon = self.get_chain_beacon();
                            show_sync_progress(beacon, &sync_target, self.epoch_constants.unwrap());

                            // TODO: this is duplicated above, this could be moved outside of the
                            // for loop because it is only used to handle the case when the last
                            // block is the target block
                            // TODO: saturating_sub?
                            // No, the target superblock cannot be the superblock with index 0
                            let epoch_of_the_last_block_according_to_target_superblock =
                                sync_target.superblock.checkpoint * superblock_period - 1;
                            if block_epoch == epoch_of_the_last_block_according_to_target_superblock
                            {
                                log::debug!("Sync done up to block #{} because that is the last checkpoint according to superblock #{}", epoch_of_the_last_block_according_to_target_superblock, sync_target.superblock.checkpoint);
                                target_reached = true;
                                break;
                            }
                        }
                    }

                    if batch_succeeded {
                        // Only persist consolidated blocks, truncate so the last block is the
                        // target block
                        // TODO: verify that truncate is correct
                        //let block_hashes = |msg: &AddBlocks| -> Vec<_> { msg.blocks.iter().map(|x| x.hash()).collect() };
                        use witnet_data_structures::chain::Block;
                        let block_hashes = |blocks: &[Block]| -> usize { blocks.len() };
                        log::debug!(
                            "Truncating list of blocks. Before: {:?}",
                            block_hashes(&msg.blocks)
                        );
                        // Only persist blocks up to the last block according to the superblock
                        // target. The remaining blocks will be processed, but not persisted to
                        // storage. So the next reorganization will delete these blocks.
                        let mut rest_blocks = msg.blocks.split_off(i);
                        log::debug!("After: {:?}", block_hashes(&msg.blocks));

                        if target_reached {
                            // TODO: we want to create the last superblock, if it was not created yet,
                            // but only if we have reached the sync target

                            // We need to construct the last superblock if the current superblock
                            // is different from the target superblock
                            let mut need_to_construct_superblock = true;
                            if sync_target.superblock.checkpoint == 0 {
                                need_to_construct_superblock = false;
                            }
                            let epoch_during_which_we_should_construct_the_target_superblock =
                                sync_target.superblock.checkpoint * superblock_period;
                            if need_to_construct_superblock {
                                // We need to persist blocks in order to be able to construct the
                                // superblock
                                // TODO: verify that superblock creation does not start until all
                                // the blocks have been persisted
                                self.persist_blocks_batch(ctx, msg.blocks);
                                let to_be_stored =
                                    self.chain_state.data_request_pool.finished_data_requests();
                                self.persist_data_requests(ctx, to_be_stored);
                                // Create superblocks while synchronizing but do not broadcast them
                                // This is needed to ensure that we can validate the received superblocks later on
                                // TODO: this is needed to check synchronization target
                                log::debug!("Will construct superblock during synchronization. Superblock index: {} Epoch {}", sync_target.superblock.checkpoint, epoch_during_which_we_should_construct_the_target_superblock);
                                self.construct_superblock(ctx, epoch_during_which_we_should_construct_the_target_superblock)
                                    .and_then({ let sync_target = sync_target.clone(); move |superblock, act, ctx| {
                                        if superblock.hash() == sync_target.superblock.hash_prev_block {
                                            act.persist_chain_state(ctx);

                                            actix::fut::ok(())
                                        } else {
                                            // The superblock hash is different from what
                                            // it should be.
                                            // This probably means a bug in the code, so
                                            // panic
                                            panic!("Mismatching superblock. Target: {:?} Created #{} {} {:?}", sync_target, superblock.index, superblock.hash(), superblock);
                                            actix::fut::err(())
                                        }
                                    }})
                                    .and_then(move |(), act, ctx| {
                                        let mut target_reached = false;
                                        let mut i = 0;
                                        for block in rest_blocks.iter() {
                                            i += 1;
                                            let block_epoch = block.block_header.beacon.checkpoint;

                                            if block_epoch
                                                > sync_target.block.checkpoint
                                            {
                                                log::debug!("Sync done up to block #{}", sync_target.block.checkpoint);
                                                i -= 1;
                                                target_reached = true;
                                                break;
                                            }

                                            // Do not update reputation when consolidating genesis block
                                            if block.hash() != consensus_constants.genesis_hash {
                                                if let Some(ref mut rep_engine) = act.chain_state.reputation_engine
                                                {
                                                    // TODO: do we also need to update_empty after the last block?
                                                    // Any epochs between the last block_epoch and current_epoch
                                                    // are never updated
                                                    if let Err(e) = rep_engine.ars_mut().update_empty(block_epoch) {
                                                        log::error!(
                                                            "Error updating reputation before processing block: {}",
                                                            e
                                                        );
                                                    }
                                                }
                                            }

                                            if let Err(e) = act.process_requested_block(ctx, block.clone()) {
                                                log::error!("Error processing block: {}", e);
                                                act.initialize_from_storage(ctx);
                                                log::info!("Restored chain state from storage");
                                                batch_succeeded = false;
                                                break;
                                            }

                                            let beacon = act.get_chain_beacon();
                                            show_sync_progress(beacon, &sync_target, act.epoch_constants.unwrap());

                                            // TODO: this is duplicated above, this could be moved outside of the
                                            // for loop because it is only used to handle the case when the last
                                            // block is the target block
                                            if block_epoch
                                                == sync_target.block.checkpoint
                                            {
                                                if sync_target.block.hash_prev_block != block.hash() {
                                                    log::error!("Target block hash mismatch: {} != {}", sync_target.block.hash_prev_block, block.hash());
                                                    act.initialize_from_storage(ctx);
                                                    log::info!("Restored chain state from storage");
                                                    batch_succeeded = false;
                                                    break;
                                                } else {
                                                    log::debug!("Sync done up to block #{}", sync_target.block.checkpoint);
                                                    target_reached = true;
                                                    break;
                                                }
                                            }
                                        }

                                        if batch_succeeded {
                                            // Only persist consolidated blocks, truncate so the last block is the
                                            // target block
                                            // TODO: verify that truncate is correct
                                            //let block_hashes = |msg: &AddBlocks| -> Vec<_> { msg.blocks.iter().map(|x| x.hash()).collect() };
                                            use witnet_data_structures::chain::Block;
                                            let block_hashes = |blocks: &[Block]| -> usize { blocks.len() };
                                            log::debug!(
                                                "Truncating list of blocks. Before: {:?}",
                                                block_hashes(&rest_blocks)
                                            );
                                            // Only persist blocks up to the last block according to the superblock
                                            // target. The remaining blocks will be processed, but not persisted to
                                            // storage. So the next reorganization will delete these blocks.
                                            rest_blocks.truncate(i);
                                            log::debug!("After: {:?}", block_hashes(&rest_blocks));

                                            // We need to persist blocks in order to be able to construct the
                                            // superblock
                                            // TODO: verify that superblock creation does not start until all
                                            // the blocks have been persisted
                                            act.persist_blocks_batch(ctx, rest_blocks);
                                            let to_be_stored =
                                                act.chain_state.data_request_pool.finished_data_requests();
                                            act.persist_data_requests(ctx, to_be_stored);

                                            if target_reached {
                                                log::info!("Block sync target achieved, go to WaitingConsensus state");
                                                // Target achived, go back to state 1
                                                act.sm_state = StateMachine::WaitingConsensus;

                                                // TODO: calculate if we need to construct the
                                                // superblock
                                                let need_to_construct_superblock = true;
                                                let epoch_during_which_we_should_construct_the_superblock = (sync_target.superblock.checkpoint + 1) * superblock_period;
                                                if need_to_construct_superblock {
                                                    log::debug!("Will construct the second superblock during synchronization. Superblock index: {} Epoch {}", sync_target.superblock.checkpoint + 1, epoch_during_which_we_should_construct_the_superblock);
                                                    act.construct_superblock(ctx, epoch_during_which_we_should_construct_the_superblock).and_then(|_superblock, _act, _ctx| {
                                                        // Ignore the created superblock: do not
                                                        // broadcast votes
                                                        actix::fut::ok(())
                                                    }).wait(ctx);
                                                }
                                            } else {
                                                // TODO: if the target superblock is in this batch but
                                                // the target block is in the next batch, does this
                                                // code work?
                                                act.request_blocks_batch(ctx);
                                            }
                                        }
                                        actix::fut::ok(())
                                    })
                                    .wait(ctx);
                            }
                        } else {
                            self.request_blocks_batch(ctx);
                        }
                    } else {
                        // This branch will happen if this node has forked, but the network has
                        // a valid consensus. In that case we would want to restore the node to
                        // the state just before the fork, and restart the synchronization.

                        // This branch could also happen when one peer has sent us an invalid block batch.
                        // Ideally we would mark it as a bad peer and restart the
                        // synchronization process, but that's not implemented yet.
                        // Note that in order to correctly restart the synchronization process,
                        // restoring the chain state from storage is not enough,
                        // as that storage was overwritten at the end of the last successful batch.

                        // In any case, the current behavior is to go back to WaitingConsensus
                        // state and restart the synchronization on the next PeersBeacons message.
                        self.sm_state = StateMachine::WaitingConsensus;
                    }
                } else {
                    log::warn!("Target Beacon is None");
                }
            }
            StateMachine::AlmostSynced | StateMachine::Synced => {}
        };

        // If we are not synchronizing, forget about when we started synchronizing
        if self.sm_state != StateMachine::Synchronizing {
            self.sync_waiting_for_add_blocks_since = None;
        }
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
    type Result = Result<(), failure::Error>;

    fn handle(
        &mut self,
        AddSuperBlockVote { superblock_vote }: AddSuperBlockVote,
        ctx: &mut Context<Self>,
    ) -> Self::Result {
        self.add_superblock_vote(superblock_vote, ctx)
    }
}

/// Handler for AddTransaction message
impl Handler<AddTransaction> for ChainManager {
    type Result = ResponseActFuture<Self, (), failure::Error>;

    fn handle(&mut self, msg: AddTransaction, _ctx: &mut Context<Self>) -> Self::Result {
        let timestamp_now = get_timestamp();
        self.add_transaction(msg, timestamp_now)
    }
}

/// Handler for GetBlocksEpochRange
impl Handler<GetBlocksEpochRange> for ChainManager {
    type Result = Result<Vec<(Epoch, Hash)>, ChainManagerError>;

    fn handle(
        &mut self,
        GetBlocksEpochRange {
            range,
            limit,
            limit_from_end,
        }: GetBlocksEpochRange,
        _ctx: &mut Context<Self>,
    ) -> Self::Result {
        log::debug!("GetBlocksEpochRange received {:?}", range);

        // Accept this message in any state
        // TODO: we should only accept this message in Synced state, but that breaks the
        // JSON-RPC getBlockChain method

        // Iterator over all the blocks in the given range
        let block_chain_range = self
            .chain_state
            .block_chain
            .range(range)
            .map(|(k, v)| (*k, *v));

        if limit == 0 {
            // Return all the blocks from this epoch range
            let hashes: Vec<(Epoch, Hash)> = block_chain_range.collect();

            Ok(hashes)
        } else if limit_from_end {
            let mut hashes: Vec<(Epoch, Hash)> = block_chain_range
                // Take the last "limit" blocks
                .rev()
                .take(limit)
                .collect();

            // Reverse again to return them in non-reversed order
            hashes.reverse();

            Ok(hashes)
        } else {
            let hashes: Vec<(Epoch, Hash)> = block_chain_range
                // Take the first "limit" blocks
                .take(limit)
                .collect();

            Ok(hashes)
        }
    }
}

impl PeersBeacons {
    /// Pretty-print a map {beacon: [peers]}
    pub fn pretty_format(&self) -> String {
        let mut beacon_peers_map: BTreeMap<String, Vec<String>> = BTreeMap::new();
        for (k, v) in self.pb.iter() {
            let v = v
                .as_ref()
                // TODO: this method ignores the superblock beacon
                .map(|x| x.highest_block_checkpoint)
                .map(|x| format!("#{} {}", x.checkpoint, x.hash_prev_block))
                .unwrap_or_else(|| "NO BEACON".to_string());
            beacon_peers_map.entry(v).or_default().push(k.to_string());
        }

        format!("{:?}", beacon_peers_map)
    }

    /// Run the consensus on the beacons, will return the most common beacon.
    /// We do not take into account our beacon to calculate the consensus.
    /// The beacons are `Option<CheckpointBeacon>`, so peers that have not
    /// sent us a beacon are counted as `None`. Keeping that in mind, we
    /// reach consensus as long as consensus_threshold % of peers agree.
    pub fn block_consensus(&self, consensus_threshold: usize) -> Option<CheckpointBeacon> {
        // We need to add `num_missing_peers` times NO BEACON, to take into account
        // missing outbound peers.
        let num_missing_peers = self.outbound_limit
            .map(|outbound_limit| {
                // TODO: is it possible to receive more than outbound_limit beacons?
                // (it shouldn't be possible)
                assert!(self.pb.len() <= outbound_limit as usize, "Received more beacons than the outbound_limit. Check the code for race conditions.");
                usize::try_from(outbound_limit).unwrap() - self.pb.len()
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
                        .map(|last_beacon| last_beacon.highest_block_checkpoint)
                })
                .chain(std::iter::repeat(None).take(num_missing_peers)),
            consensus_threshold,
        )
        // Flatten result:
        // None (consensus % below threshold) should be treated the same way as
        // Some(None) (most of the peers did not send a beacon)
        .and_then(|x| x)
    }

    // TODO: this is a copy-paste of block_consensus, refactor?
    /// Run the consensus on the beacons, will return the most common beacon.
    /// We do not take into account our beacon to calculate the consensus.
    /// The beacons are `Option<CheckpointBeacon>`, so peers that have not
    /// sent us a beacon are counted as `None`. Keeping that in mind, we
    /// reach consensus as long as consensus_threshold % of peers agree.
    pub fn superblock_consensus(&self, consensus_threshold: usize) -> Option<CheckpointBeacon> {
        // We need to add `num_missing_peers` times NO BEACON, to take into account
        // missing outbound peers.
        let num_missing_peers = self.outbound_limit
            .map(|outbound_limit| {
                // TODO: is it possible to receive more than outbound_limit beacons?
                // (it shouldn't be possible)
                assert!(self.pb.len() <= outbound_limit as usize, "Received more beacons than the outbound_limit. Check the code for race conditions.");
                usize::try_from(outbound_limit).unwrap() - self.pb.len()
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
                .chain(std::iter::repeat(None).take(num_missing_peers)),
            consensus_threshold,
        )
        // Flatten result:
        // None (consensus % below threshold) should be treated the same way as
        // Some(None) (most of the peers did not send a beacon)
        .and_then(|x| x)
    }

    /// Collects the peers to unregister based on the beacon they reported and the beacon to be compared it with
    pub fn decide_peers_to_unregister(&self, beacon: CheckpointBeacon) -> Vec<SocketAddr> {
        // Unregister peers which have a different beacon
        (&self.pb)
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
        (&self.pb)
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

    /// Collects the peers that have not sent us a beacon
    pub fn peers_with_no_beacon(&self) -> Vec<SocketAddr> {
        // Unregister peers which have not sent us a beacon
        (&self.pb)
            .iter()
            .filter_map(|(p, b)| if b.is_none() { Some(*p) } else { None })
            .collect()
    }
}

impl Handler<PeersBeacons> for ChainManager {
    type Result = <PeersBeacons as Message>::Result;

    // FIXME(#676): Remove clippy skip error
    #[allow(clippy::cognitive_complexity)]
    fn handle(&mut self, peers_beacons: PeersBeacons, ctx: &mut Context<Self>) -> Self::Result {
        log::debug!(
            "PeersBeacons received while StateMachine is in state {:?}",
            self.sm_state
        );

        log::debug!("Received beacons: {}", peers_beacons.pretty_format());

        // Activate peers beacons index to continue synced
        if !peers_beacons.pb.is_empty() {
            self.peers_beacons_received = true;
        }

        // Calculate the consensus, or None if there is no consensus
        let consensus_threshold = self.consensus_c as usize;
        let superblock_consensus = peers_beacons.superblock_consensus(consensus_threshold);
        let consensus = peers_beacons.block_consensus(consensus_threshold);
        let outbound_limit = peers_beacons.outbound_limit;
        let pb_len = peers_beacons.pb.len();
        let peers_needed_for_consensus = outbound_limit
            .map(|x| {
                // ceil(x * consensus_threshold / 100)
                (usize::from(x) * consensus_threshold + 99) / 100
            })
            .unwrap_or(1);
        let peers_with_no_beacon = peers_beacons.peers_with_no_beacon();
        let peers_to_unregister = if let Some(consensus_beacon) = consensus {
            peers_beacons.decide_peers_to_unregister(consensus_beacon)
        } else if let Some(superblock_consensus) = superblock_consensus {
            peers_beacons.decide_peers_to_unregister_s(superblock_consensus)
        } else if pb_len < peers_needed_for_consensus {
            // Not enough outbound peers, do not unregister any peers
            log::debug!(
                "Got {} peers but need at least {} to calculate the consensus",
                pb_len,
                peers_needed_for_consensus
            );
            vec![]
        } else {
            // No consensus: if state is AlmostSynced unregister those that are not coincident with ours.
            // Else, unregister all peers
            if self.sm_state == StateMachine::AlmostSynced || self.sm_state == StateMachine::Synced
            {
                log::warn!("Lack of peer consensus while state is `AlmostSynced`: peers that do not coincide with our last beacon will be unregistered");
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
                match (superblock_consensus, consensus) {
                    (Some(superblock_consensus), Some(consensus_beacon)) => {
                        self.sync_target = Some(SyncTarget {
                            block: consensus_beacon,
                            superblock: superblock_consensus,
                        });
                        log::debug!("Sync target {:?}", self.sync_target);

                        let our_beacon = self.get_chain_beacon();
                        log::debug!(
                            "Consensus beacon: {:?} Our beacon {:?}",
                            (consensus_beacon, superblock_consensus),
                            our_beacon
                        );

                        let consensus_constants = self.consensus_constants();

                        // Check if we are already synchronized
                        // TODO: use superblock beacon
                        self.sm_state = if consensus_beacon.hash_prev_block
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
                                "[CONSENSUS]: We are on {:?} but the network is on {:?}",
                                our_beacon,
                                consensus_beacon
                            );

                            self.initialize_from_storage(ctx);
                            log::info!("Restored chain state from storage");

                            StateMachine::WaitingConsensus
                        } else {
                            // Review candidates
                            let consensus_block_hash = consensus_beacon.hash_prev_block;
                            let candidate = self.candidates.remove(&consensus_block_hash);
                            // Clear candidates, as they are only valid for one epoch
                            self.candidates.clear();
                            self.seen_candidates.clear();
                            // TODO: Be functional my friend
                            if let Some(consensus_block) = candidate {
                                match self.process_requested_block(ctx, consensus_block) {
                                    Ok(()) => {
                                        log::info!(
                                            "Consolidate consensus candidate. AlmostSynced state"
                                        );
                                        StateMachine::AlmostSynced
                                    }
                                    Err(e) => {
                                        log::debug!(
                                            "Failed to consolidate consensus candidate: {}",
                                            e
                                        );

                                        self.request_blocks_batch(ctx);

                                        StateMachine::Synchronizing
                                    }
                                }
                            } else {
                                self.request_blocks_batch(ctx);

                                StateMachine::Synchronizing
                            }
                        };

                        Ok(peers_to_unregister)
                    }
                    // There is superblock consensus but no block consensus
                    // Sync up to the superblock
                    (Some(superblock_consensus), None) => {
                        // TODO: if there is no block consensus just pick one at random from the
                        // beacons that are in superblock_consensus. And refactor this match into a
                        // simpler if
                        let block = Default::default();
                        self.sync_target = Some(SyncTarget {
                            block,
                            superblock: superblock_consensus,
                        });

                        // There is no clear block consensus but there is superblock consensus
                        let local_superblock = self.get_superblock_beacon();

                        // If the superblock consensus is the same as the local consensus, we can
                        // be considered synced. Most likely, the network will have some extra
                        // blocks that are unknown to this node, but it should be synced after the
                        // next superblock voting round
                        if local_superblock == superblock_consensus {
                            // TODO: almost synced?
                            self.sm_state = StateMachine::Synced;

                            // Unregister peers with no superblock consensus
                            Ok(peers_to_unregister)
                        } else {
                            log::debug!("Sync target {:?}", self.sync_target);

                            self.request_blocks_batch(ctx);
                            self.sm_state = StateMachine::Synchronizing;

                            // Unregister peers with no superblock consensus
                            Ok(peers_to_unregister)
                        }
                    }
                    // There is no superblock consensus but there is block consensus
                    // This should never happen, but if it does, drop all peers
                    (None, Some(_consensus_beacon)) => Ok(peers_to_unregister),
                    // No consensus: unregister all peers
                    (None, None) => Ok(peers_to_unregister),
                }
            }
            StateMachine::Synchronizing => {
                match (superblock_consensus, consensus) {
                    (Some(superblock_consensus), Some(consensus_beacon)) => {
                        self.sync_target = Some(SyncTarget {
                            block: consensus_beacon,
                            superblock: superblock_consensus,
                        });

                        let our_beacon = self.get_chain_beacon();

                        // Check if we are already synchronized
                        self.sm_state = if our_beacon == consensus_beacon {
                            StateMachine::AlmostSynced
                        } else if our_beacon.checkpoint == consensus_beacon.checkpoint
                            && our_beacon.hash_prev_block != consensus_beacon.hash_prev_block
                        {
                            // Fork case
                            log::warn!(
                                "[CONSENSUS]: We are on {:?} but the network is on {:?}",
                                our_beacon,
                                consensus_beacon
                            );

                            self.initialize_from_storage(ctx);
                            log::info!("Restored chain state from storage");

                            StateMachine::WaitingConsensus
                        } else {
                            StateMachine::Synchronizing
                        };

                        Ok(peers_to_unregister)
                    }
                    (Some(superblock_consensus), None) => {
                        // TODO: if there is no block consensus just pick one at random from the
                        // beacons that are in superblock_consensus. And refactor this match into a
                        // simpler if
                        let block = Default::default();
                        self.sync_target = Some(SyncTarget {
                            block,
                            superblock: superblock_consensus,
                        });
                        // TODO: use superblock beacon to check if we are already synchronized
                        self.request_blocks_batch(ctx);

                        Ok(peers_to_unregister)
                    }
                    // There is no superblock consensus but there is block consensus
                    // This should never happen, but if it does, drop all peers
                    (None, Some(_consensus_beacon)) => {
                        self.sm_state = StateMachine::WaitingConsensus;

                        Ok(peers_to_unregister)
                    }
                    // No consensus: unregister all peers
                    (None, None) => {
                        self.sm_state = StateMachine::WaitingConsensus;

                        Ok(peers_to_unregister)
                    }
                }
            }
            StateMachine::AlmostSynced | StateMachine::Synced => {
                let our_beacon = self.get_chain_beacon();
                match consensus {
                    Some(consensus_beacon) if consensus_beacon == our_beacon => {
                        if self.sm_state == StateMachine::AlmostSynced {
                            // This is the only point in the whole base code for the state
                            // machine to move into `Synced` state.
                            log::debug!("Moving from AlmostSynced to Synced state");
                            log::info!("{}", SYNCED_BANNER);
                            self.sm_state = StateMachine::Synced;
                            self.add_temp_superblock_votes(ctx).unwrap();
                        }
                        Ok(peers_to_unregister)
                    }
                    Some(_) => {
                        // We are out of consensus!
                        log::warn!(
                            "[CONSENSUS]: We are on {:?} but the network is on {:?}",
                            our_beacon,
                            consensus
                        );

                        // If we are synced, it does not matter what blocks have been consolidated
                        // by our outbound peers, we will stay synced until the next superblock
                        // vote

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
                                "[CONSENSUS]: We are on {:?} but the network has no consensus",
                                our_beacon
                            );
                        }

                        // If we are synced, it does not matter what blocks have been consolidated
                        // by our outbound peers, we will stay synced until the next superblock
                        // vote

                        Ok(peers_with_no_beacon)
                    }
                }
            }
        };

        if self.sm_state == StateMachine::Synchronizing {
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
        }

        peers_to_unregister
    }
}

impl Handler<BuildVtt> for ChainManager {
    type Result = ResponseActFuture<Self, Hash, failure::Error>;

    fn handle(&mut self, msg: BuildVtt, _ctx: &mut Self::Context) -> Self::Result {
        if self.sm_state != StateMachine::Synced {
            return Box::new(actix::fut::err(
                ChainManagerError::NotSynced {
                    current_state: self.sm_state,
                }
                .into(),
            ));
        }
        let timestamp = u64::try_from(get_timestamp()).unwrap();
        match transaction_factory::build_vtt(
            msg.vto,
            msg.fee,
            &mut self.chain_state.own_utxos,
            self.own_pkh.unwrap(),
            &self.chain_state.unspent_outputs_pool,
            timestamp,
            self.tx_pending_timeout,
            msg.utxo_strategy,
        ) {
            Err(e) => {
                log::error!("Error when building value transfer transaction: {}", e);
                Box::new(actix::fut::err(e.into()))
            }
            Ok(vtt) => {
                let fut = transaction_factory::sign_transaction(&vtt, vtt.inputs.len())
                    .into_actor(self)
                    .then(|s, act, ctx| match s {
                        Ok(signatures) => {
                            let transaction =
                                Transaction::ValueTransfer(VTTransaction::new(vtt, signatures));
                            let tx_hash = transaction.hash();
                            Box::new(
                                act.handle(AddTransaction { transaction }, ctx)
                                    .map(move |_, _, _| tx_hash),
                            )
                        }
                        Err(e) => {
                            log::error!("Failed to sign value transfer transaction: {}", e);

                            let res: Box<
                                dyn ActorFuture<
                                    Item = Hash,
                                    Error = failure::Error,
                                    Actor = ChainManager,
                                >,
                            > = Box::new(actix::fut::err(e));
                            res
                        }
                    });

                Box::new(fut)
            }
        }
    }
}

impl Handler<BuildDrt> for ChainManager {
    type Result = ResponseActFuture<Self, Hash, failure::Error>;

    fn handle(&mut self, msg: BuildDrt, _ctx: &mut Self::Context) -> Self::Result {
        if self.sm_state != StateMachine::Synced {
            return Box::new(actix::fut::err(
                ChainManagerError::NotSynced {
                    current_state: self.sm_state,
                }
                .into(),
            ));
        }
        if let Err(e) = validate_rad_request(&msg.dro.data_request) {
            return Box::new(actix::fut::err(e));
        }
        let timestamp = u64::try_from(get_timestamp()).unwrap();
        match transaction_factory::build_drt(
            msg.dro,
            msg.fee,
            &mut self.chain_state.own_utxos,
            self.own_pkh.unwrap(),
            &self.chain_state.unspent_outputs_pool,
            timestamp,
            self.tx_pending_timeout,
        ) {
            Err(e) => {
                log::error!("Error when building data request transaction: {}", e);
                Box::new(actix::fut::err(e.into()))
            }
            Ok(drt) => {
                log::debug!("Created drt:\n{:?}", drt);
                let fut = transaction_factory::sign_transaction(&drt, drt.inputs.len())
                    .into_actor(self)
                    .then(|s, act, ctx| match s {
                        Ok(signatures) => {
                            let transaction =
                                Transaction::DataRequest(DRTransaction::new(drt, signatures));
                            let tx_hash = transaction.hash();
                            Box::new(
                                act.handle(AddTransaction { transaction }, ctx)
                                    .map(move |_, _, _| tx_hash),
                            )
                        }
                        Err(e) => {
                            log::error!("Failed to sign data request transaction: {}", e);

                            let res: Box<
                                dyn ActorFuture<
                                    Item = Hash,
                                    Error = failure::Error,
                                    Actor = ChainManager,
                                >,
                            > = Box::new(actix::fut::err(e));
                            res
                        }
                    });

                Box::new(fut)
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

impl Handler<GetDataRequestReport> for ChainManager {
    type Result = ResponseFuture<DataRequestInfo, failure::Error>;

    fn handle(&mut self, msg: GetDataRequestReport, _ctx: &mut Self::Context) -> Self::Result {
        let dr_pointer = msg.dr_pointer;

        // First, try to get it from memory
        if let Some(dr_info) = self
            .chain_state
            .data_request_pool
            .data_request_pool
            .get(&dr_pointer)
            .map(|dr_state| dr_state.info.clone())
        {
            Box::new(futures::finished(dr_info))
        } else {
            let dr_pointer_string = format!("DR-REPORT-{}", dr_pointer);
            // Otherwise, try to get it from storage
            let fut = storage_mngr::get::<_, DataRequestReport>(&dr_pointer_string).and_then(
                move |dr_report| match dr_report {
                    Some(x) => futures::finished(DataRequestInfo::from(x)),
                    None => futures::failed(DataRequestNotFound { hash: dr_pointer }.into()),
                },
            );

            Box::new(fut)
        }
    }
}

impl Handler<GetBalance> for ChainManager {
    type Result = Result<u64, failure::Error>;

    fn handle(&mut self, GetBalance { pkh }: GetBalance, _ctx: &mut Self::Context) -> Self::Result {
        if self.sm_state != StateMachine::Synced {
            return Err(ChainManagerError::NotSynced {
                current_state: self.sm_state,
            }
            .into());
        }

        Ok(transaction_factory::get_total_balance(
            &self.chain_state.unspent_outputs_pool,
            pkh,
        ))
    }
}

impl Handler<GetUtxoInfo> for ChainManager {
    type Result = Result<UtxoInfo, failure::Error>;

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
        let block_number_limit = self
            .chain_state
            .block_number()
            .saturating_sub(chain_info.consensus_constants.collateral_age);

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
    type Result = Result<(Reputation, bool), failure::Error>;

    fn handle(
        &mut self,
        GetReputation { pkh }: GetReputation,
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

        Ok((rep_eng.trs().get(&pkh), rep_eng.ars().contains(&pkh)))
    }
}

impl Handler<GetReputationAll> for ChainManager {
    type Result = Result<HashMap<PublicKeyHash, (Reputation, bool)>, failure::Error>;

    fn handle(&mut self, _msg: GetReputationAll, _ctx: &mut Self::Context) -> Self::Result {
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

        Ok(rep_eng
            .trs()
            .identities()
            .map(|(k, v)| (*k, (*v, rep_eng.ars().contains(k))))
            .collect())
    }
}

impl Handler<GetReputationStatus> for ChainManager {
    type Result = Result<GetReputationStatusResult, failure::Error>;

    fn handle(&mut self, _msg: GetReputationStatus, _ctx: &mut Self::Context) -> Self::Result {
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

        let num_active_identities = u32::try_from(rep_eng.ars().active_identities_number())?;
        let total_active_reputation = rep_eng.total_active_reputation();

        Ok(GetReputationStatusResult {
            num_active_identities,
            total_active_reputation,
        })
    }
}

impl Handler<TryMineBlock> for ChainManager {
    type Result = ();

    fn handle(&mut self, _msg: TryMineBlock, ctx: &mut Self::Context) -> Self::Result {
        self.try_mine_block(ctx);
    }
}

impl Handler<AddCommitReveal> for ChainManager {
    type Result = ResponseActFuture<Self, (), failure::Error>;

    fn handle(
        &mut self,
        AddCommitReveal {
            commit_transaction,
            reveal_transaction,
        }: AddCommitReveal,
        ctx: &mut Self::Context,
    ) -> Self::Result {
        let dr_pointer = commit_transaction.body.dr_pointer;
        // Hold reveal transaction under "waiting_for_reveal" field of data requests pool
        self.chain_state
            .data_request_pool
            .insert_reveal(dr_pointer, reveal_transaction);

        // Send AddTransaction message to self
        // And broadcast it to all of peers
        Box::new(
            self.handle(
                AddTransaction {
                    transaction: Transaction::Commit(commit_transaction),
                },
                ctx,
            )
            .map_err(|e, _, _| {
                log::warn!("Failed to add commit transaction: {}", e);
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
    type Result = Result<GetMempoolResult, failure::Error>;

    fn handle(&mut self, _msg: GetMempool, _ctx: &mut Self::Context) -> Self::Result {
        let res = GetMempoolResult {
            value_transfer: self.transactions_pool.vt_iter().map(|t| t.hash()).collect(),
            data_request: self.transactions_pool.dr_iter().map(|t| t.hash()).collect(),
        };

        Ok(res)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn peers_beacons_consensus_less_peers_than_outbound() {
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
        let peers_beacons = PeersBeacons {
            pb: vec![],
            outbound_limit: Some(4),
        };
        assert_eq!(peers_beacons.block_consensus(60), None);

        // 1 peer
        let peers_beacons = PeersBeacons {
            pb: vec![("127.0.0.1:10001".parse().unwrap(), Some(beacon1.clone()))],
            outbound_limit: Some(4),
        };
        assert_eq!(peers_beacons.block_consensus(60), None);

        // 2 peers
        let peers_beacons = PeersBeacons {
            pb: vec![
                ("127.0.0.1:10001".parse().unwrap(), Some(beacon1.clone())),
                ("127.0.0.1:10002".parse().unwrap(), Some(beacon1.clone())),
            ],
            outbound_limit: Some(4),
        };
        assert_eq!(peers_beacons.block_consensus(60), None);

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
        assert_eq!(peers_beacons.block_consensus(60), None);

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
            peers_beacons.block_consensus(60),
            Some(beacon1.highest_block_checkpoint)
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
        assert_eq!(peers_beacons.block_consensus(60), None);

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
            peers_beacons.block_consensus(60),
            Some(beacon1.highest_block_checkpoint)
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
            peers_beacons.block_consensus(60),
            Some(beacon1.highest_block_checkpoint)
        );
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
}
