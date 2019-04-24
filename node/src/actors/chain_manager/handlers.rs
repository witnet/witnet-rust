use actix::prelude::*;
use log::{debug, error, warn};

use witnet_data_structures::{
    chain::{CheckpointBeacon, Epoch, Hashable, InventoryEntry, InventoryItem},
    error::ChainInfoError,
};
use witnet_validations::validations::{validate_block, validate_transaction, UtxoDiff};

use super::{ChainManager, ChainManagerError, StateMachine};
use crate::actors::chain_manager::transaction_factory;
use crate::actors::messages::{BuildDrt, BuildVtt};
use crate::{
    actors::{
        messages::{
            AddBlocks, AddCandidates, AddTransaction, Anycast, Broadcast, EpochNotification,
            GetBlocksEpochRange, GetHighestCheckpointBeacon, PeersBeacons, SendLastBeacon,
            SessionUnitResult,
        },
        sessions_manager::SessionsManager,
    },
    utils::mode_consensus,
};
use actix::fut::WrapFuture;
use std::collections::HashMap;

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
        debug!("Epoch notification received {:?}", msg.checkpoint);

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
            warn!("Genesis checkpoint is here! Starting to bootstrap the chain...");
            self.started(ctx);
        }
    }
}

/// Handler for EpochNotification<EveryEpochPayload>
impl Handler<EpochNotification<EveryEpochPayload>> for ChainManager {
    type Result = ();

    fn handle(&mut self, msg: EpochNotification<EveryEpochPayload>, ctx: &mut Context<Self>) {
        debug!("Periodic epoch notification received {:?}", msg.checkpoint);
        self.current_epoch = Some(msg.checkpoint);

        debug!(
            "EpochNotification received while StateMachine is in state {:?}",
            self.sm_state
        );
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
            StateMachine::Synced => {
                if let (Some(current_epoch), Some(chain_info)) =
                    (self.current_epoch, self.chain_state.chain_info.as_ref())
                {
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
                        ) {
                            Ok(utxo_diff) => {
                                chosen_candidate = Some((key, block_candidate, utxo_diff))
                            }
                            Err(e) => debug!("{}", e),
                        }
                    }

                    // Consolidate the best candidate
                    if let Some((_, block, utxo_diff)) = chosen_candidate {
                        // Persist block and update ChainState
                        self.consolidate_block(ctx, &block, utxo_diff);
                    } else {
                        warn!(
                            "There was no valid block candidate to consolidate for epoch {}",
                            msg.checkpoint - 1
                        );
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
                } else {
                    warn!("ChainManager doesn't have current epoch");
                }
            }
        };
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
            error!("No ChainInfo loaded in ChainManager");
            Err(ChainInfoError::ChainInfoNotFound)?
        }
    }
}

/// Handler for AddBlocks message
impl Handler<AddBlocks> for ChainManager {
    type Result = SessionUnitResult;

    fn handle(&mut self, msg: AddBlocks, ctx: &mut Context<Self>) {
        debug!(
            "AddBlocks received while StateMachine is in state {:?}",
            self.sm_state
        );
        match self.sm_state {
            StateMachine::WaitingConsensus => {}
            StateMachine::Synchronizing => {
                if let Some(target_beacon) = self.target_beacon {
                    let mut batch_succeeded = true;
                    for block in msg.blocks.iter() {
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

                    if batch_succeeded {
                        self.persist_blocks_batch(ctx, msg.blocks, target_beacon);
                        let to_be_stored =
                            self.chain_state.data_request_pool.finished_data_requests();
                        to_be_stored.into_iter().for_each(|dr| {
                            self.persist_data_request(ctx, &dr);
                        });
                        self.persist_chain_state(ctx);
                    }

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
                    warn!("Target Beacon is None");
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
        debug!(
            "AddTransaction received while StateMachine is in state {:?}",
            self.sm_state
        );
        // Ignore AddTransaction when not in Synced state
        match self.sm_state {
            StateMachine::WaitingConsensus => {
                return;
            }
            StateMachine::Synchronizing => {
                return;
            }
            StateMachine::Synced => {}
        };

        let transaction_hash = &msg.transaction.hash();
        if self.transactions_pool.contains(transaction_hash) {
            debug!("Transaction is already in the pool: {}", transaction_hash);
            return;
        } else {
            let utxo_diff = UtxoDiff::new(&self.chain_state.unspent_outputs_pool);
            match validate_transaction(
                &msg.transaction,
                &utxo_diff,
                &self.chain_state.data_request_pool,
                &mut HashMap::new(),
            ) {
                Ok(_) => {
                    debug!("Transaction added successfully");
                    // Broadcast valid transaction
                    self.broadcast_item(InventoryItem::Transaction(msg.transaction.clone()));

                    // Add valid transaction to transactions_pool
                    self.transactions_pool
                        .insert(*transaction_hash, msg.transaction);
                }

                Err(e) => warn!("{}", e),
            }
        }
    }
}

/// Handler for GetBlocksEpochRange
impl Handler<GetBlocksEpochRange> for ChainManager {
    type Result = Result<Vec<(Epoch, InventoryEntry)>, ChainManagerError>;

    fn handle(
        &mut self,
        GetBlocksEpochRange { range, limit }: GetBlocksEpochRange,
        _ctx: &mut Context<Self>,
    ) -> Self::Result {
        debug!("GetBlocksEpochRange received {:?}", range);

        // Accept this message in any state
        // TODO: we should only accept this message in Synced state, but that breaks the
        // JSON-RPC getBlockChain method

        let mut hashes: Vec<(Epoch, InventoryEntry)> = self
            .chain_state
            .block_chain
            .range(range)
            .map(|(k, v)| (*k, InventoryEntry::Block(*v)))
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

    fn handle(
        &mut self,
        PeersBeacons { pb }: PeersBeacons,
        ctx: &mut Context<Self>,
    ) -> Self::Result {
        debug!(
            "PeersBeacons received while StateMachine is in state {:?}",
            self.sm_state
        );
        match self.sm_state {
            StateMachine::WaitingConsensus => {
                // As soon as there is consensus, we set the target beacon to the consensus
                // and set the state to Synchronizing

                // Run the consensus on the beacons, will return the most common beacon
                // In case of tie returns None
                if let Some(beacon) = mode_consensus(pb.iter().map(|(_p, b)| b)).cloned() {
                    // Consensus: unregister peers which have a different beacon
                    let peers_out_of_consensus = pb
                        .into_iter()
                        .filter_map(|(p, b)| if b != beacon { Some(p) } else { None })
                        .collect();
                    self.target_beacon = Some(beacon);

                    let our_beacon = self
                        .chain_state
                        .chain_info
                        .as_ref()
                        .unwrap()
                        .highest_block_checkpoint;

                    // Check if we are already synchronized
                    self.sm_state = if our_beacon == beacon {
                        StateMachine::Synced
                    } else {
                        // Review candidates
                        let consensus_block_hash = beacon.hash_prev_block;
                        // TODO: Be functional my friend
                        if let Some(consensus_block) = self.candidates.remove(&consensus_block_hash)
                        {
                            match self.process_requested_block(ctx, &consensus_block) {
                                Ok(()) => {
                                    debug!("Consolidate consensus candidate. Synced state");
                                    StateMachine::Synced
                                }
                                Err(e) => {
                                    debug!("Failed to consolidate consensus candidate: {}", e);

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
                // We are synchronizing, so ignore all the new beacons until we reach the target beacon

                Ok(vec![])
            }
            StateMachine::Synced => {
                // If we are synced and the consensus beacon is not the same as our beacon, then
                // we need to rewind one epoch

                if pb.is_empty() {
                    // TODO: all other peers disconnected, return to WaitingConsensus state?
                    warn!("[CONSENSUS]: We have zero outbound peers");
                }

                let our_beacon = self
                    .chain_state
                    .chain_info
                    .as_ref()
                    .unwrap()
                    .highest_block_checkpoint;

                // We also take into account our beacon to calculate the consensus
                // TODO: should we count our own beacon when deciding consensus?
                let consensus_beacon =
                    mode_consensus(pb.iter().map(|(_p, b)| b).chain(&[our_beacon])).cloned();

                match consensus_beacon {
                    Some(a) if a == our_beacon => {
                        // Consensus: unregister peers which have a different beacon
                        let peers_out_of_consensus = pb
                            .into_iter()
                            .filter_map(|(p, b)| if b != our_beacon { Some(p) } else { None })
                            .collect();
                        // TODO: target_beacon is not used in this state, right?
                        // TODO: target_beacon can be a field in the state machine enum:
                        // Synchronizing { target_beacon }
                        // We could do the same with chain_state.chain_info, to avoid all the unwraps
                        //self.target_beacon = Some(beacon);
                        Ok(peers_out_of_consensus)
                    }
                    Some(_a) => {
                        // We are out of consensus!
                        // TODO: We should probably rewind(1) to avoid a fork, but for simplicity
                        // (rewind is not implemented yet) we just print a message and carry on
                        warn!(
                            "[CONSENSUS]: We are on {:?} but the network is on {:?}",
                            our_beacon, consensus_beacon
                        );

                        // Return an empty vector indicating that we do not want to unregister any peer
                        Ok(vec![])
                    }
                    None => {
                        // There is no consensus because of a tie, do not rewind?
                        // For example this could happen when each peer reports a different beacon...
                        warn!(
                            "[CONSENSUS]: We are on {:?} but the network has no consensus",
                            our_beacon
                        );
                        // Return an empty vector indicating that we do not want to unregister any peer
                        Ok(vec![])
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
            Err(e) => error!("{}", e),
            Ok(vtt) => {
                transaction_factory::sign_transaction(vtt)
                    .into_actor(self)
                    .then(|t, _act, ctx| {
                        match t {
                            Ok(transaction) => {
                                ctx.notify(AddTransaction { transaction });
                            }
                            Err(e) => error!("{}", e),
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
            Err(e) => error!("{}", e),
            Ok(vtt) => {
                debug!("Created vtt:\n{:?}", vtt);
                transaction_factory::sign_transaction(vtt)
                    .into_actor(self)
                    .then(|t, _act, ctx| {
                        match t {
                            Ok(transaction) => {
                                ctx.notify(AddTransaction { transaction });
                            }
                            Err(e) => error!("{}", e),
                        }

                        actix::fut::ok(())
                    })
                    .wait(ctx);
            }
        }
    }
}
