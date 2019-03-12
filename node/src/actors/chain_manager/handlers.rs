use actix::{Actor, Context, Handler, Message, SystemService};
use log::{debug, error, warn};

use witnet_data_structures::{
    chain::{CheckpointBeacon, Epoch, Hashable, InventoryEntry, InventoryItem},
    error::{ChainInfoError, ChainInfoErrorKind, ChainInfoResult},
};
use witnet_util::error::WitnetError;

use super::{validations::validate_block, ChainManager, ChainManagerError, StateMachine};
use crate::actors::messages::{
    AddBlocks, AddCandidates, AddTransaction, Anycast, Broadcast, EpochNotification,
    GetBlocksEpochRange, GetHighestCheckpointBeacon, PeersBeacons, SendLastBeacon,
    SessionUnitResult,
};
use crate::actors::sessions_manager::SessionsManager;
use crate::utils::mode_consensus;

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
                    let candidates = self.candidates.clone();

                    // Decide the best candidate
                    // TODO: replace for loop with a try_fold
                    let mut chosen_candidate = None;
                    for (key, block_candidate) in candidates {
                        if let Some((chosen_key, _)) = chosen_candidate.clone() {
                            if chosen_key < key {
                                // Ignore candidates with bigger hashes
                                continue;
                            }
                        }
                        chosen_candidate = validate_block(
                            &block_candidate,
                            current_epoch,
                            chain_info.highest_block_checkpoint,
                            self.genesis_block_hash,
                            &self.chain_state.unspent_outputs_pool,
                            &self.transactions_pool,
                            &self.data_request_pool,
                        )
                        .ok()
                        .map(|block_in_chain| (key, block_in_chain));
                    }

                    // Consolidate the best candidate
                    if let Some((_, block_in_chain)) = chosen_candidate {
                        // Persist block and update ChainState
                        self.consolidate_block(
                            ctx,
                            block_in_chain.block,
                            block_in_chain.utxo_set,
                            block_in_chain.data_request_pool,
                            true,
                        );
                    } else {
                        warn!(
                            "There is no valid block candidate to consolidate for epoch {}",
                            msg.checkpoint
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
    type Result = ChainInfoResult<CheckpointBeacon>;

    fn handle(
        &mut self,
        _msg: GetHighestCheckpointBeacon,
        _ctx: &mut Context<Self>,
    ) -> Self::Result {
        if let Some(chain_info) = &self.chain_state.chain_info {
            Ok(chain_info.highest_block_checkpoint)
        } else {
            error!("No ChainInfo loaded in ChainManager");
            Err(WitnetError::from(ChainInfoError::new(
                ChainInfoErrorKind::ChainInfoNotFound,
                "No ChainInfo loaded in ChainManager".to_string(),
            )))
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
                let old_chain_state = self.chain_state.clone();
                if let Some(target_beacon) = self.target_beacon {
                    for block in msg.blocks {
                        if let Err(()) = self.process_requested_block(ctx, block) {
                            warn!("Unexpected fail in process_requested_block");
                            self.chain_state = old_chain_state;
                            break;
                        }
                        // This check is needed if we get more blocks than needed: stop at target
                        let our_beacon = self
                            .chain_state
                            .chain_info
                            .as_ref()
                            .unwrap()
                            .highest_block_checkpoint;
                        if our_beacon == target_beacon {
                            // Target achived, go back to state 1
                            self.sm_state = StateMachine::WaitingConsensus;
                            break;
                        }
                    }

                    let our_beacon = self
                        .chain_state
                        .chain_info
                        .as_ref()
                        .unwrap()
                        .highest_block_checkpoint;
                    if target_beacon != our_beacon {
                        // Try again, send Anycast<SendLastBeacon> to a "safu" peer, i.e. their last beacon matches our target beacon.
                        SessionsManager::from_registry().do_send(Anycast {
                            command: SendLastBeacon { beacon: our_beacon },
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
            self.process_candidate(block)
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
        }

        debug!("Adding transaction: {:?}", msg.transaction);
        // FIXME: transaction validation is broken
        //let outputs = &msg.transaction.outputs;
        let inputs = &msg.transaction.inputs;

        match self.chain_state.get_outputs_from_inputs(inputs) {
            Ok(_outputs_from_inputs) => {
                //let outputs_from_inputs = &outputs_from_inputs;

                // Check that all inputs point to unspent outputs
                if self
                    .chain_state
                    .find_unspent_outputs(&msg.transaction.inputs)
                {
                    /*
                    use crate::utils::{
                        count_tally_outputs, is_commit_input, is_commit_output, is_data_request_input,
                        is_reveal_output, is_tally_output, is_value_transfer_output, validate_tally_output_uniqueness,
                        validate_value_transfer_output_position,
                    };
                                        // Validate transaction
                                        // DRO RULE 1. Multiple data request outputs can be included into a single transaction.
                                        // As long as the inputs are greater than the outputs, the rule still hold true. The difference
                                        // with VTOs is that the total output value for data request outputs also includes the
                                        // commit fee, reveal fee and tally fee.
                                        let inputs_sum = outputs_from_inputs.iter().map(Output::value).sum();
                                        let outputs_sum = msg.transaction.outputs_sum();

                                        if outputs_sum < inputs_sum {
                                            let mut is_valid_input = true;
                                            let mut is_valid_output = true;
                                            let iter = outputs.iter().zip(inputs.iter());

                                            // VTO RULE 2. The number of VTOs in a single transaction is virtually unlimited as
                                            // long as the VTOs are all contiguous and located at the end of the outputs list.
                                            let is_valid_vto_position =
                                                validate_value_transfer_output_position(outputs);

                                            // TO RULE 1. Any transaction can contain at most one tally output.
                                            let consensus_output_overflow = count_tally_outputs(outputs) > 1;

                                            let is_valid_transaction = iter
                                                .take_while(|(output, input)| {
                                                    // Validate input
                                                    is_valid_input =
                                                        match &self.chain_state.get_output_from_input(input) {
                                                            // VTO RULE 4. The value brought into a transaction by an input pointing
                                                            // to a VTO can be freely assigned to any output of any type unless otherwise
                                                            // restricted by the specific validation rules for such output type.
                                                            Some(Output::ValueTransfer(_)) => true,

                                                            // DRO RULE 2. The value brought into a transaction by an input pointing
                                                            // to a data request output can only be spent by commit outputs.
                                                            Some(Output::DataRequest(_)) => is_commit_output(output),

                                                            // CO RULE 3. The value brought into a transaction by an input pointing
                                                            // to a commit output can only be spent by reveal or tally outputs.
                                                            Some(Output::Commit(_)) => {
                                                                is_reveal_output(output) || is_tally_output(output)
                                                            }

                                                            // RO 3. The value brought into a transaction by an input pointing to a
                                                            // reveal output can only be spent by value transfer outputs.
                                                            Some(Output::Reveal(_)) => match output {
                                                                Output::ValueTransfer(_) => true,
                                                                _ => false,
                                                            },
                                                            // TO RULE 4. The value brought into a transaction by an input pointing
                                                            // to a tally output can be freely assigned to any output of any type
                                                            // unless otherwise restricted by the specific validation rules for such
                                                            // output type.
                                                            Some(Output::Tally(_)) => true,

                                                            None => false,
                                                        };

                                                    is_valid_output = match output {
                                                        Output::ValueTransfer(_) => true,

                                                        Output::DataRequest(_) => true,
                                                        // CO RULE 1. Commit outputs can only take value from data request inputs
                                                        // whose index in the inputs list is the same as their own index in the outputs list.
                                                        // CO RULE 2. Multiple commit outputs can exist in a single transaction,
                                                        // but each of them needs to be coupled with a data request input occupying
                                                        // the same index in the inputs list as their own in the outputs list.
                                                        // Predictably, as a result of the previous rule, each of the multiple
                                                        // commit outputs only takes value from the data request input with the same index.
                                                        Output::Commit(_) => is_data_request_input(input),

                                                        Output::Reveal(_) => {
                                                            // RO RULE 3. The value brought into a transaction by an input pointing
                                                            // to a reveal output can only be spent by value transfer outputs.
                                                            is_value_transfer_output(output)
                                                            // RO RULE 1. Reveal outputs can only take value from commit inputs
                                                            // whose index in the inputs list is the same as their own index in the outputs list.
                                                            // RO RULE 2. Multiple reveal outputs can exist in a single transaction,
                                                            // but each of them needs to be coupled with a commit input occupying
                                                            // the same index in the inputs list as their own in the outputs list.
                                                            // Predictably, as a result of the previous rule, each of the multiple
                                                            // reveal outputs only takes value from the commit input with the same index.
                                                            && is_commit_input(input)
                                                            // TODO: validate only once
                                                            // RO RULE 4. Any transaction including an input pointing to a
                                                            // reveal output must also include exactly only one tally output.
                                                            && validate_tally_output_uniqueness(outputs)
                                                        }
                                                        Output::Tally(_) => true,
                                                    };

                                                    is_valid_input && is_valid_output
                                                })
                                                .last();

                                            if is_valid_transaction.is_some()
                                                && is_valid_vto_position
                                                && !consensus_output_overflow
                                            {*/
                    debug!("Transaction added successfully");
                    // Broadcast valid transaction
                    self.broadcast_item(InventoryItem::Transaction(msg.transaction.clone()));

                    // Add valid transaction to transactions_pool
                    self.transactions_pool
                        .insert(*transaction_hash, msg.transaction);
                } else {
                    warn!("Input OutputPointer not in pool");
                }
            }
            Err(_) => {
                //TODO Show input information
                warn!("Input with no output");
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
                        if let Some(consensus_block) = self.candidates.remove(&consensus_block_hash)
                        {
                            if self.process_requested_block(ctx, consensus_block).is_ok() {
                                StateMachine::Synced
                            } else {
                                // Send Anycast<SendLastBeacon> to a safu peer in order to begin the synchronization
                                SessionsManager::from_registry().do_send(Anycast {
                                    command: SendLastBeacon { beacon: our_beacon },
                                    safu: true,
                                });

                                StateMachine::Synchronizing
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

                    // Clear candidates
                    self.candidates.clear();

                    Ok(peers_out_of_consensus)
                } else {
                    // No consensus: unregister all peers
                    let all_peers = pb.into_iter().map(|(p, _b)| p).collect();
                    Ok(all_peers)
                }
            }
            StateMachine::Synchronizing => {
                // We are synchronizing, so ignore all the new beacons until we reach the target beacon

                // Return error meaning unexpected message, because if we return Ok(vec![]), the
                // SessionsManager will mark all the peers as safu and break our security assumptions
                Err(())
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
