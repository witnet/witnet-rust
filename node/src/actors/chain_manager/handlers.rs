use actix::{fut::WrapFuture, prelude::*};
use futures::Future;
use log;
use std::{cmp::Ordering, collections::HashMap};

use witnet_data_structures::{
    chain::{
        ChainState, CheckpointBeacon, DataRequestInfo, DataRequestReport, Epoch, Hash, Hashable,
        PublicKeyHash, Reputation,
    },
    error::{ChainInfoError, TransactionError::DataRequestNotFound},
    transaction::{DRTransaction, Transaction, VTTransaction},
};
use witnet_validations::validations::{compare_blocks, validate_block, validate_rad_request};

use super::{
    show_sync_progress, transaction_factory, ChainManager, ChainManagerError, StateMachine,
};
use crate::{
    actors::{
        messages::{
            AddBlocks, AddCandidates, AddCommitReveal, AddTransaction, Anycast, Broadcast,
            BuildDrt, BuildVtt, EpochNotification, GetBalance, GetBlocksEpochRange,
            GetDataRequestReport, GetHighestCheckpointBeacon, GetMemoryTransaction, GetReputation,
            GetReputationAll, GetReputationStatus, GetReputationStatusResult, GetState,
            PeersBeacons, SendLastBeacon, SessionUnitResult, TryMineBlock,
        },
        sessions_manager::SessionsManager,
    },
    storage_mngr,
    utils::mode_consensus,
};
use std::collections::BTreeMap;
use witnet_util::timestamp::get_timestamp;

pub const SYNCED_BANNER: &str = r"
███████╗██╗   ██╗███╗   ██╗ ██████╗███████╗██████╗ ██╗
██╔════╝╚██╗ ██╔╝████╗  ██║██╔════╝██╔════╝██╔══██╗██║
███████╗ ╚████╔╝ ██╔██╗ ██║██║     █████╗  ██║  ██║██║
╚════██║  ╚██╔╝  ██║╚██╗██║██║     ██╔══╝  ██║  ██║╚═╝
███████║   ██║   ██║ ╚████║╚██████╗███████╗██████╔╝██╗
╚══════╝   ╚═╝   ╚═╝  ╚═══╝ ╚═════╝╚══════╝╚═════╝ ╚═╝";

////////////////////////////////////////////////////////////////////////////////////////
// ACTOR MESSAGE HANDLERS
////////////////////////////////////////////////////////////////////////////////////////
/// Payload for the notification for a specific epoch
#[derive(Debug)]
pub struct EpochPayload;

/// Payload for the notification for all epochs
#[derive(Clone, Debug)]
pub struct EveryEpochPayload;

/// Handler for EpochNotification<EpochPayload>
impl Handler<EpochNotification<EpochPayload>> for ChainManager {
    type Result = ();

    fn handle(&mut self, msg: EpochNotification<EpochPayload>, ctx: &mut Context<Self>) {
        log::debug!("Epoch notification received {:?}", msg.checkpoint);

        match self.sm_state {
            StateMachine::WaitingConsensus => {}
            StateMachine::Synchronizing => {
                unimplemented!();
            }
            StateMachine::Synced => {
                unimplemented!();
            }
        };

        //TODO: Refactor next code in StateMachin branches

        // Genesis checkpoint notification. We need to start building the chain.
        if msg.checkpoint == 0 {
            log::warn!("Genesis checkpoint is here! Starting to bootstrap the chain...");
            self.started(ctx);
        }
    }
}

/// Handler for EpochNotification<EveryEpochPayload>
impl Handler<EpochNotification<EveryEpochPayload>> for ChainManager {
    type Result = ();

    fn handle(&mut self, msg: EpochNotification<EveryEpochPayload>, ctx: &mut Context<Self>) {
        log::debug!("Periodic epoch notification received {:?}", msg.checkpoint);
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

        // Handle case consensus not achieved
        if !self.peers_beacons_received {
            log::warn!("No beacon messages received from peers. Moving to WaitingConsensus status");
            self.sm_state = StateMachine::WaitingConsensus;
            // Clear candidates
            self.candidates.clear();
        }

        match self.sm_state {
            StateMachine::WaitingConsensus => {
                if let Some(chain_info) = &self.chain_state.chain_info {
                    // Send last beacon because otherwise the network cannot bootstrap
                    SessionsManager::from_registry().do_send(Broadcast {
                        command: SendLastBeacon {
                            beacon: chain_info.highest_block_checkpoint,
                        },
                        only_inbound: true,
                    });
                }
            }
            StateMachine::Synchronizing => {}
            StateMachine::Synced => match self.chain_state {
                ChainState {
                    chain_info: Some(ref mut chain_info),
                    reputation_engine: Some(ref mut rep_engine),
                    ..
                } => {
                    if self.epoch_constants.is_none()
                        || self.vrf_ctx.is_none()
                        || self.secp.is_none()
                    {
                        log::error!("{}", ChainManagerError::ChainNotReady);
                        return;
                    }
                    // Decide the best candidate
                    // TODO: replace for loop with a try_fold
                    let mut chosen_candidate = None;
                    for (key, block_candidate) in self.candidates.drain() {
                        let block_pkh = &block_candidate.block_sig.public_key.pkh();
                        let reputation = rep_engine.trs.get(block_pkh);

                        if let Some((chosen_key, chosen_reputation, _, _)) = chosen_candidate {
                            if compare_blocks(key, reputation, chosen_key, chosen_reputation)
                                != Ordering::Greater
                            {
                                // Ignore in case that reputation would be lower
                                // than previously chosen candidate or would be
                                // equal but with highest hash
                                continue;
                            }
                        }
                        match validate_block(
                            &block_candidate,
                            current_epoch,
                            chain_info.highest_block_checkpoint,
                            &self.chain_state.unspent_outputs_pool,
                            &self.chain_state.data_request_pool,
                            self.vrf_ctx.as_mut().unwrap(),
                            rep_engine,
                            self.epoch_constants.unwrap(),
                            self.secp.as_ref().unwrap(),
                        ) {
                            Ok(utxo_diff) => {
                                let block_pkh = &block_candidate.block_sig.public_key.pkh();
                                let reputation = rep_engine.trs.get(block_pkh);
                                chosen_candidate =
                                    Some((key, reputation, block_candidate, utxo_diff))
                            }
                            Err(e) => log::debug!("{}", e),
                        }
                    }

                    // Consolidate the best candidate
                    if let Some((_, _, block, utxo_diff)) = chosen_candidate {
                        // Persist block and update ChainState
                        self.consolidate_block(ctx, &block, utxo_diff);
                    } else {
                        let previous_epoch = msg.checkpoint - 1;
                        log::warn!(
                            "There was no valid block candidate to consolidate for epoch {}",
                            previous_epoch
                        );

                        // Update ActiveReputationSet in case of epochs without blocks
                        if let Err(e) = rep_engine.ars.update(vec![], previous_epoch) {
                            log::error!("Error updating empty reputation with no blocks: {}", e);
                        }
                    }

                    // Send last beacon in state 3 on block consolidation
                    SessionsManager::from_registry().do_send(Broadcast {
                        command: SendLastBeacon {
                            beacon: self
                                .chain_state
                                .chain_info
                                .as_ref()
                                .unwrap()
                                .highest_block_checkpoint,
                        },
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
                }

                _ => {
                    log::error!("No ChainInfo loaded in ChainManager");
                }
            },
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

/// Handler for AddBlocks message
impl Handler<AddBlocks> for ChainManager {
    type Result = SessionUnitResult;

    fn handle(&mut self, msg: AddBlocks, ctx: &mut Context<Self>) {
        log::debug!(
            "AddBlocks received while StateMachine is in state {:?}",
            self.sm_state
        );
        match self.sm_state {
            StateMachine::WaitingConsensus => {}
            StateMachine::Synchronizing => {
                if let Some(target_beacon) = self.target_beacon {
                    let mut batch_succeeded = true;
                    let chain_beacon = self.get_chain_beacon();
                    if msg.blocks.is_empty() {
                        batch_succeeded = false;
                        log::debug!("Received an empty AddBlocks message");
                    // FIXME(#684): this condition would be modified when genesis block exist
                    } else if chain_beacon.hash_prev_block != self.genesis_block_hash
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
                        let blocks = if chain_beacon.hash_prev_block == self.genesis_block_hash
                            || msg.blocks[0].block_header.beacon.checkpoint
                                > chain_beacon.checkpoint
                        {
                            &msg.blocks[..]
                        } else {
                            &msg.blocks[1..]
                        };

                        for block in blocks.iter() {
                            // Update reputation before checking Proof-of-Eligibility
                            let block_epoch = block.block_header.beacon.checkpoint;

                            if let Some(ref mut rep_engine) = self.chain_state.reputation_engine {
                                if let Err(e) = rep_engine.ars.update_empty(block_epoch) {
                                    log::error!(
                                        "Error updating reputation before processing block: {}",
                                        e
                                    );
                                }
                            }

                            if let Err(e) = self.process_requested_block(ctx, block) {
                                log::error!("Error processing block: {}", e);
                                self.initialize_from_storage(ctx);
                                log::info!("Restored chain state from storage");
                                batch_succeeded = false;
                                break;
                            }

                            let beacon = self.get_chain_beacon();
                            show_sync_progress(
                                beacon,
                                target_beacon,
                                self.epoch_constants.unwrap(),
                            );

                            if beacon == target_beacon {
                                break;
                            }
                        }
                    }

                    if batch_succeeded {
                        self.persist_blocks_batch(ctx, msg.blocks, target_beacon);
                        let to_be_stored =
                            self.chain_state.data_request_pool.finished_data_requests();
                        self.persist_data_requests(ctx, to_be_stored);
                        self.persist_chain_state(ctx);

                        let beacon = self.get_chain_beacon();

                        if beacon == target_beacon {
                            // Target achived, go back to state 1
                            self.sm_state = StateMachine::WaitingConsensus;
                        } else {
                            // Try again, send Anycast<SendLastBeacon> to a "safu" peer, i.e. their last beacon matches our target beacon.
                            SessionsManager::from_registry().do_send(Anycast {
                                command: SendLastBeacon { beacon },
                                safu: true,
                            });
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
            StateMachine::Synced => {}
        };
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

/// Handler for AddTransaction message
impl Handler<AddTransaction> for ChainManager {
    type Result = Result<(), failure::Error>;

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
        GetBlocksEpochRange { range, limit }: GetBlocksEpochRange,
        _ctx: &mut Context<Self>,
    ) -> Self::Result {
        log::debug!("GetBlocksEpochRange received {:?}", range);

        // Accept this message in any state
        // TODO: we should only accept this message in Synced state, but that breaks the
        // JSON-RPC getBlockChain method

        let mut hashes: Vec<(Epoch, Hash)> = self
            .chain_state
            .block_chain
            .range(range)
            .map(|(k, v)| (*k, *v))
            .collect();

        // Hashes Vec has not to be bigger than MAX_BLOCKS_SYNC
        if limit != 0 {
            hashes.truncate(limit);
        }

        Ok(hashes)
    }
}

impl Handler<PeersBeacons> for ChainManager {
    type Result = <PeersBeacons as Message>::Result;

    // FIXME(#676): Remove clippy skip error
    #[allow(clippy::cognitive_complexity)]
    fn handle(
        &mut self,
        PeersBeacons { pb }: PeersBeacons,
        ctx: &mut Context<Self>,
    ) -> Self::Result {
        log::debug!(
            "PeersBeacons received while StateMachine is in state {:?}",
            self.sm_state
        );

        // Pretty-print a map {beacon: [peers]}
        let mut beacon_peers_map: BTreeMap<String, Vec<String>> = BTreeMap::new();
        for (k, v) in pb.iter() {
            let v = v
                .map(|x| format!("#{} {}", x.checkpoint, x.hash_prev_block))
                .unwrap_or_else(|| "NO BEACON".to_string());
            beacon_peers_map.entry(v).or_default().push(k.to_string());
        }
        log::debug!("Received beacons: {:?}", beacon_peers_map);

        // Activate peers beacons index to continue synced
        self.peers_beacons_received = true;

        let consensus_threshold = self.consensus_c as usize;

        // Run the consensus on the beacons, will return the most common beacon.
        // We do not take into account our beacon to calculate the consensus.
        // The beacons are Option<CheckpointBeacon>, so peers that have not
        // sent us a beacon are counted as None. Keeping that in mind, we
        // reach consensus as long as consensus_threshold % of peers agree.
        // In case of tie returns None
        let consensus = mode_consensus(pb.iter().map(|(_p, b)| b), consensus_threshold)
            // Flatten result:
            // None (no consensus) should be treated the same way as
            // Some(None) (the consensus is that there is no consensus)
            .and_then(|x| *x);

        match self.sm_state {
            StateMachine::WaitingConsensus => {
                // As soon as there is consensus, we set the target beacon to the consensus
                // and set the state to Synchronizing
                if let Some(consensus_beacon) = consensus {
                    // Consensus: unregister peers which have a different beacon
                    let peers_out_of_consensus = pb
                        .into_iter()
                        .filter_map(|(p, b)| {
                            if b != Some(consensus_beacon) {
                                Some(p)
                            } else {
                                None
                            }
                        })
                        .collect();
                    self.target_beacon = Some(consensus_beacon);

                    let our_beacon = self.get_chain_beacon();

                    // Check if we are already synchronized
                    self.sm_state = if our_beacon == consensus_beacon {
                        log::info!("{}", SYNCED_BANNER);
                        StateMachine::Synced
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
                        // TODO: Be functional my friend
                        if let Some(consensus_block) = self.candidates.remove(&consensus_block_hash)
                        {
                            match self.process_requested_block(ctx, &consensus_block) {
                                Ok(()) => {
                                    log::info!("Consolidate consensus candidate. Synced state");
                                    log::info!("{}", SYNCED_BANNER);
                                    StateMachine::Synced
                                }
                                Err(e) => {
                                    log::debug!("Failed to consolidate consensus candidate: {}", e);

                                    // Send Anycast<SendLastBeacon> to a safu peer in order to begin the synchronization
                                    SessionsManager::from_registry().do_send(Anycast {
                                        command: SendLastBeacon { beacon: our_beacon },
                                        safu: true,
                                    });

                                    StateMachine::Synchronizing
                                }
                            }
                        } else {
                            // Send Anycast<SendLastBeacon> to a safu peer in order to begin the synchronization
                            SessionsManager::from_registry().do_send(Anycast {
                                command: SendLastBeacon { beacon: our_beacon },
                                safu: true,
                            });

                            StateMachine::Synchronizing
                        }
                    };

                    Ok(peers_out_of_consensus)
                } else {
                    // No consensus: unregister all peers
                    log::warn!("No consensus: unregister all peers");
                    let all_peers = pb.into_iter().map(|(p, _b)| p).collect();
                    Ok(all_peers)
                }
            }
            StateMachine::Synchronizing => {
                if let Some(consensus_beacon) = consensus {
                    // List peers that announced a beacon out of consensus
                    let peers_out_of_consensus = pb
                        .into_iter()
                        .filter_map(|(p, b)| {
                            if b != Some(consensus_beacon) {
                                Some(p)
                            } else {
                                None
                            }
                        })
                        .collect();
                    self.target_beacon = Some(consensus_beacon);

                    let our_beacon = self.get_chain_beacon();

                    // Check if we are already synchronized
                    self.sm_state = if our_beacon == consensus_beacon {
                        log::info!("{}", SYNCED_BANNER);
                        StateMachine::Synced
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

                    Ok(peers_out_of_consensus)
                } else {
                    // No consensus: unregister all peers
                    log::warn!("No consensus: unregister all peers");
                    let all_peers = pb.into_iter().map(|(p, _b)| p).collect();
                    // Move to waiting consensus stage
                    self.sm_state = StateMachine::WaitingConsensus;

                    Ok(all_peers)
                }
            }
            StateMachine::Synced => {
                // If we are synced and the consensus beacon is not the same as our beacon, then
                // we need to rewind one epoch
                if pb.is_empty() {
                    log::warn!("[CONSENSUS]: We have not received any beacons for this epoch");
                    self.sm_state = StateMachine::WaitingConsensus;
                }

                let our_beacon = self.get_chain_beacon();
                match consensus {
                    Some(a) if a == our_beacon => {
                        // Consensus: unregister peers which have a different beacon
                        let peers_out_of_consensus = pb
                            .into_iter()
                            .filter_map(|(p, b)| if b != Some(our_beacon) { Some(p) } else { None })
                            .collect();

                        Ok(peers_out_of_consensus)
                    }
                    Some(a) => {
                        // We are out of consensus!
                        // Unregister peers that announced a beacon out of consensus
                        let peers_out_of_consensus = pb
                            .into_iter()
                            .filter_map(|(p, b)| if b != Some(a) { Some(p) } else { None })
                            .collect();

                        log::warn!(
                            "[CONSENSUS]: We are on {:?} but the network is on {:?}",
                            our_beacon,
                            consensus
                        );

                        self.initialize_from_storage(ctx);
                        log::info!("Restored chain state from storage");

                        self.sm_state = StateMachine::WaitingConsensus;

                        Ok(peers_out_of_consensus)
                    }
                    None => {
                        // There is no consensus because of a tie, do not rewind?
                        // For example this could happen when each peer reports a different beacon...
                        log::warn!(
                            "[CONSENSUS]: We are on {:?} but the network has no consensus",
                            our_beacon
                        );

                        self.sm_state = StateMachine::WaitingConsensus;

                        // Unregister all peers to try to obtain a new set of trustworthy peers
                        let all_peers = pb.into_iter().map(|(p, _b)| p).collect();
                        Ok(all_peers)
                    }
                }
            }
        }
    }
}

impl Handler<BuildVtt> for ChainManager {
    type Result = ResponseActFuture<Self, Hash, failure::Error>;

    fn handle(&mut self, msg: BuildVtt, _ctx: &mut Self::Context) -> Self::Result {
        if self.sm_state != StateMachine::Synced {
            return Box::new(actix::fut::err(ChainManagerError::NotSynced.into()));
        }
        let timestamp = get_timestamp() as u64;
        match transaction_factory::build_vtt(
            msg.vto,
            msg.fee,
            &mut self.chain_state.own_utxos,
            self.own_pkh.unwrap(),
            &self.chain_state.unspent_outputs_pool,
            timestamp,
            self.tx_pending_timeout,
        ) {
            Err(e) => {
                log::error!("{}", e);
                Box::new(actix::fut::err(e))
            }
            Ok(vtt) => {
                let fut = transaction_factory::sign_transaction(&vtt, vtt.inputs.len())
                    .into_actor(self)
                    .then(|s, _act, ctx| match s {
                        Ok(signatures) => {
                            let transaction =
                                Transaction::ValueTransfer(VTTransaction::new(vtt, signatures));
                            let tx_hash = transaction.hash();
                            ctx.notify(AddTransaction { transaction });

                            actix::fut::ok(tx_hash)
                        }
                        Err(e) => {
                            log::error!("{}", e);

                            actix::fut::err(e)
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
            return Box::new(actix::fut::err(ChainManagerError::NotSynced.into()));
        }
        if let Err(e) = validate_rad_request(&msg.dro.data_request) {
            return Box::new(actix::fut::err(e));
        }
        let timestamp = get_timestamp() as u64;
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
                log::error!("{}", e);
                Box::new(actix::fut::err(e))
            }
            Ok(drt) => {
                log::debug!("Created drt:\n{:?}", drt);
                let fut = transaction_factory::sign_transaction(&drt, drt.inputs.len())
                    .into_actor(self)
                    .then(|s, _act, ctx| match s {
                        Ok(signatures) => {
                            let transaction =
                                Transaction::DataRequest(DRTransaction::new(drt, signatures));
                            let tx_hash = transaction.hash();
                            ctx.notify(AddTransaction { transaction });

                            actix::fut::ok(tx_hash)
                        }
                        Err(e) => {
                            log::error!("{}", e);

                            actix::fut::err(e)
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
            return Err(ChainManagerError::NotSynced.into());
        }

        Ok(transaction_factory::get_total_balance(
            &self.chain_state.unspent_outputs_pool,
            pkh,
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
            return Err(ChainManagerError::NotSynced.into());
        }

        let rep_eng = match self.chain_state.reputation_engine.as_ref() {
            Some(x) => x,
            None => return Err(ChainManagerError::ChainNotReady.into()),
        };

        Ok((rep_eng.trs.get(&pkh), rep_eng.ars.contains(&pkh)))
    }
}

impl Handler<GetReputationAll> for ChainManager {
    type Result = Result<HashMap<PublicKeyHash, (Reputation, bool)>, failure::Error>;

    fn handle(&mut self, _msg: GetReputationAll, _ctx: &mut Self::Context) -> Self::Result {
        if self.sm_state != StateMachine::Synced {
            return Err(ChainManagerError::NotSynced.into());
        }

        let rep_eng = match self.chain_state.reputation_engine.as_ref() {
            Some(x) => x,
            None => return Err(ChainManagerError::ChainNotReady.into()),
        };

        Ok(rep_eng
            .trs
            .identities()
            .map(|(k, v)| (*k, (*v, rep_eng.ars.contains(k))))
            .collect())
    }
}
impl Handler<GetReputationStatus> for ChainManager {
    type Result = Result<GetReputationStatusResult, failure::Error>;

    fn handle(&mut self, _msg: GetReputationStatus, _ctx: &mut Self::Context) -> Self::Result {
        if self.sm_state != StateMachine::Synced {
            return Err(ChainManagerError::NotSynced.into());
        }

        let rep_eng = match self.chain_state.reputation_engine.as_ref() {
            Some(x) => x,
            None => return Err(ChainManagerError::ChainNotReady.into()),
        };

        let num_active_identities = rep_eng.ars.active_identities_number() as u32;
        let total_active_reputation = rep_eng.trs.get_sum(rep_eng.ars.active_identities());

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
    type Result = ();

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
        if let Err(e) = self.handle(
            AddTransaction {
                transaction: Transaction::Commit(commit_transaction),
            },
            ctx,
        ) {
            log::warn!("Failed to add commit transaction: {}", e);
        }
    }
}

impl Handler<GetMemoryTransaction> for ChainManager {
    type Result = Result<Transaction, ()>;

    fn handle(&mut self, msg: GetMemoryTransaction, _ctx: &mut Self::Context) -> Self::Result {
        self.transactions_pool.get(&msg.hash).ok_or(())
    }
}
