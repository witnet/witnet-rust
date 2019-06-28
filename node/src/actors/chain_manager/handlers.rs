use actix::{fut::WrapFuture, prelude::*};
use log;

use witnet_data_structures::{
    chain::{ChainState, CheckpointBeacon, Epoch, Hash, Hashable, InventoryItem, PublicKeyHash},
    error::{ChainInfoError, TransactionError},
    transaction::{DRTransaction, Transaction, VTTransaction},
};
use witnet_validations::validations::{
    validate_block, validate_commit_transaction, validate_dr_transaction,
    validate_reveal_transaction, validate_vt_transaction, UtxoDiff,
};

use super::{ChainManager, ChainManagerError, StateMachine};
use crate::{
    actors::{
        chain_manager::transaction_factory,
        messages::{
            AddBlocks, AddCandidates, AddTransaction, Anycast, Broadcast, BuildDrt, BuildVtt,
            EpochNotification, GetBlocksEpochRange, GetHighestCheckpointBeacon, PeersBeacons,
            SendLastBeacon, SessionUnitResult,
        },
        sessions_manager::SessionsManager,
    },
    utils::mode_consensus,
};

pub const SYNCED_BANNER: &str = r"
███████╗██╗   ██╗███╗   ██╗ ██████╗███████╗██████╗ ██╗
██╔════╝╚██╗ ██╔╝████╗  ██║██╔════╝██╔════╝██╔══██╗██║
███████╗ ╚████╔╝ ██╔██╗ ██║██║     █████╗  ██║  ██║██║
╚════██║  ╚██╔╝  ██║╚██╗██║██║     ██╔══╝  ██║  ██║╚═╝
███████║   ██║   ██║ ╚████║╚██████╗███████╗██████╔╝██╗
╚══════╝ ╚═╝ ╚═╝ ╚═══╝ ╚═════╝╚══════╝╚═════╝ ╚═╝";

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
                    // Decide the best candidate
                    // TODO: replace for loop with a try_fold
                    let mut chosen_candidate = None;
                    for (key, block_candidate) in self.candidates.drain() {
                        if let Some((chosen_key, _, _)) = chosen_candidate {
                            if chosen_key < key {
                                // Ignore candidates with bigger hashes
                                continue;
                            }
                        }
                        match validate_block(
                            &block_candidate,
                            current_epoch,
                            chain_info.highest_block_checkpoint,
                            self.genesis_block_hash,
                            &self.chain_state.unspent_outputs_pool,
                            &self.chain_state.data_request_pool,
                            self.vrf_ctx.as_mut().unwrap(),
                            rep_engine,
                        ) {
                            Ok(utxo_diff) => {
                                chosen_candidate = Some((key, block_candidate, utxo_diff))
                            }
                            Err(e) => log::debug!("{}", e),
                        }
                    }

                    // Consolidate the best candidate
                    if let Some((_, block, utxo_diff)) = chosen_candidate {
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
                        // Data race: the data requests should be sent after mining the block, otherwise
                        // it takes 2 epochs to move from one stage to the next one
                        self.try_mine_block(ctx);

                        // Data request mining MUST finish BEFORE the block has been mined!!!!
                        // (The transactions must be included into this block, both the transactions from
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
            Err(ChainInfoError::ChainInfoNotFound)?
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

                            if self.get_chain_beacon() == target_beacon {
                                break;
                            }
                        }
                    }

                    if batch_succeeded {
                        self.persist_blocks_batch(ctx, msg.blocks, target_beacon);
                        let to_be_stored =
                            self.chain_state.data_request_pool.finished_data_requests();
                        to_be_stored.into_iter().for_each(|dr| {
                            self.persist_data_request(ctx, &dr);
                        });
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
    type Result = SessionUnitResult;

    fn handle(&mut self, msg: AddTransaction, _ctx: &mut Context<Self>) {
        log::debug!(
            "AddTransaction received while StateMachine is in state {:?}",
            self.sm_state
        );
        // Ignore AddTransaction when not in Synced state
        match self.sm_state {
            StateMachine::Synced => {}
            _ => {
                return;
            }
        };

        let transaction = &msg.transaction;
        let tx_hash = transaction.hash();
        let utxo_diff = UtxoDiff::new(&self.chain_state.unspent_outputs_pool);

        let validation_result: Result<(), failure::Error> = match transaction {
            Transaction::ValueTransfer(tx) => {
                if self.transactions_pool.vt_contains(&tx_hash) {
                    log::debug!("Transaction is already in the pool: {}", tx_hash);
                    return;
                }

                validate_vt_transaction(tx, &utxo_diff).map(|_| ())
            }

            Transaction::DataRequest(tx) => {
                if self.transactions_pool.dr_contains(&tx_hash) {
                    log::debug!("Transaction is already in the pool: {}", tx_hash);
                    return;
                }

                validate_dr_transaction(tx, &utxo_diff).map(|_| ())
            }
            Transaction::Commit(tx) => {
                let dr_pointer = tx.body.dr_pointer;
                let pkh = PublicKeyHash::from_public_key(&tx.signatures[0].public_key);

                if self
                    .transactions_pool
                    .commit_contains(&dr_pointer, &pkh, &tx_hash)
                {
                    log::debug!("Transaction is already in the pool: {}", tx_hash);
                    return;
                }

                let mut dr_beacon = self.get_chain_beacon();
                // We need a checkpoint beacon with the current epoch, but `get_chain_beacon`
                // returns the epoch of the last block.
                if let Some(epoch) = self.current_epoch {
                    dr_beacon.checkpoint = epoch;
                }

                let rep_eng = self.chain_state.reputation_engine.as_ref().unwrap();
                validate_commit_transaction(
                    tx,
                    &self.chain_state.data_request_pool,
                    dr_beacon,
                    // The unwrap is safe because if there is no VRF context,
                    // the actor should have stopped execution
                    self.vrf_ctx.as_mut().unwrap(),
                    rep_eng,
                )
                .map(|_| ())
            }
            Transaction::Reveal(tx) => {
                let dr_pointer = tx.body.dr_pointer;
                let pkh = PublicKeyHash::from_public_key(&tx.signatures[0].public_key);

                if self
                    .transactions_pool
                    .reveal_contains(&dr_pointer, &pkh, &tx_hash)
                {
                    log::debug!("Transaction is already in the pool: {}", tx_hash);
                    return;
                }

                validate_reveal_transaction(tx, &self.chain_state.data_request_pool).map(|_| ())
            }
            _ => Err(TransactionError::NotValidTransaction.into()),
        };

        match validation_result {
            Ok(_) => {
                log::debug!("Transaction added successfully");
                // Broadcast valid transaction
                self.broadcast_item(InventoryItem::Transaction(msg.transaction.clone()));

                // Add valid transaction to transactions_pool
                self.transactions_pool.insert(msg.transaction);
            }

            Err(e) => log::warn!("{}", e),
        }
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
        // Activate peers beacons index to continue synced
        self.peers_beacons_received = true;

        match self.sm_state {
            StateMachine::WaitingConsensus => {
                // As soon as there is consensus, we set the target beacon to the consensus
                // and set the state to Synchronizing

                // Run the consensus on the beacons, will return the most common beacon
                // In case of tie returns None
                if let Some(consensus_beacon) = mode_consensus(pb.iter().map(|(_p, b)| b)).cloned()
                {
                    // Consensus: unregister peers which have a different beacon
                    let peers_out_of_consensus = pb
                        .into_iter()
                        .filter_map(|(p, b)| if b != consensus_beacon { Some(p) } else { None })
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
                                    self.persist_item(
                                        ctx,
                                        InventoryItem::Block(consensus_block.clone()),
                                    );
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
                    let all_peers = pb.into_iter().map(|(p, _b)| p).collect();
                    Ok(all_peers)
                }
            }
            StateMachine::Synchronizing => {
                // Run the consensus on the beacons, will return the most common beacon
                // In case of tie returns None
                if let Some(consensus_beacon) = mode_consensus(pb.iter().map(|(_p, b)| b)).cloned()
                {
                    // List peers that announced a beacon out of consensus
                    let peers_out_of_consensus = pb
                        .into_iter()
                        .filter_map(|(p, b)| if b != consensus_beacon { Some(p) } else { None })
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
                    let all_peers = pb.into_iter().map(|(p, _b)| p).collect();
                    Ok(all_peers)
                }
            }
            StateMachine::Synced => {
                // If we are synced and the consensus beacon is not the same as our beacon, then
                // we need to rewind one epoch

                if pb.is_empty() {
                    log::warn!("[CONSENSUS]: We have zero outbound peers");
                    self.sm_state = StateMachine::WaitingConsensus;
                }

                let our_beacon = self.get_chain_beacon();

                // We also take into account our beacon to calculate the consensus
                let consensus_beacon = mode_consensus(pb.iter().map(|(_p, b)| b)).cloned();

                match consensus_beacon {
                    Some(a) if a == our_beacon => {
                        // Consensus: unregister peers which have a different beacon
                        let peers_out_of_consensus = pb
                            .into_iter()
                            .filter_map(|(p, b)| if b != our_beacon { Some(p) } else { None })
                            .collect();

                        Ok(peers_out_of_consensus)
                    }
                    Some(a) => {
                        // We are out of consensus!
                        // Unregister peers that announced a beacon out of consensus
                        let peers_out_of_consensus = pb
                            .into_iter()
                            .filter_map(|(p, b)| if b != a { Some(p) } else { None })
                            .collect();

                        log::warn!(
                            "[CONSENSUS]: We are on {:?} but the network is on {:?}",
                            our_beacon,
                            consensus_beacon
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
    type Result = <BuildVtt as Message>::Result;

    fn handle(&mut self, msg: BuildVtt, ctx: &mut Self::Context) -> Self::Result {
        match transaction_factory::build_vtt(
            msg.vto,
            msg.fee,
            &self.chain_state.own_utxos,
            self.own_pkh.unwrap(),
            &self.chain_state.unspent_outputs_pool,
        ) {
            Err(e) => log::error!("{}", e),
            Ok(vtt) => {
                transaction_factory::sign_transaction(&vtt, vtt.inputs.len())
                    .into_actor(self)
                    .then(|s, _act, ctx| {
                        match s {
                            Ok(signatures) => {
                                let transaction =
                                    Transaction::ValueTransfer(VTTransaction::new(vtt, signatures));
                                ctx.notify(AddTransaction { transaction });
                            }
                            Err(e) => log::error!("{}", e),
                        }

                        actix::fut::ok(())
                    })
                    .wait(ctx);
            }
        }
    }
}
impl Handler<BuildDrt> for ChainManager {
    type Result = <BuildDrt as Message>::Result;

    fn handle(&mut self, msg: BuildDrt, ctx: &mut Self::Context) -> Self::Result {
        match transaction_factory::build_drt(
            msg.dro,
            msg.fee,
            &self.chain_state.own_utxos,
            self.own_pkh.unwrap(),
            &self.chain_state.unspent_outputs_pool,
        ) {
            Err(e) => log::error!("{}", e),
            Ok(drt) => {
                log::debug!("Created vtt:\n{:?}", drt);
                transaction_factory::sign_transaction(&drt, drt.inputs.len())
                    .into_actor(self)
                    .then(|s, _act, ctx| {
                        match s {
                            Ok(signatures) => {
                                let transaction =
                                    Transaction::DataRequest(DRTransaction::new(drt, signatures));
                                ctx.notify(AddTransaction { transaction });
                            }
                            Err(e) => log::error!("{}", e),
                        }

                        actix::fut::ok(())
                    })
                    .wait(ctx);
            }
        }
    }
}
