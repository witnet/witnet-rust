//! # ChainManager actor
//!
//! This module contains the ChainManager actor which is in charge
//! of managing the blocks and transactions of the Witnet blockchain
//! received through the protocol, and also encapsulates the logic of the
//! _unspent transaction outputs_.
//!
//! Among its responsibilities are the following:
//!
//! * Initializing the chain info upon running the node for the first time and persisting it into storage [StorageManager](actors::storage_manager::StorageManager)
//! * Recovering the chain info from storage and keeping it in its state.
//! * Validating block candidates as they come from a session.
//! * Consolidating multiple block candidates for the same checkpoint into a single valid block.
//! * Putting valid blocks into storage by sending them to the inventory manager actor.
//! * Having a method for letting other components get blocks by *hash* or *checkpoint*.
//! * Having a method for letting other components get the epoch of the current tip of the
//! blockchain (e.g. the last epoch field required for the handshake in the Witnet network
//! protocol).
//! * Validating transactions as they come from any [Session](actors::session::Session). This includes:
//!     - Iterating over its inputs, adding the value of the inputs to calculate the value of the transaction.
//!     - Running the output scripts, expecting them all to return `TRUE` and leave an empty stack.
//!     - Verifying that the sum of all inputs is greater than or equal to the sum of all the outputs.
//! * Keeping valid transactions into memory. This in-memory transaction pool is what we call the _mempool_. Valid transactions are immediately appended to the mempool.
//! * Keeping every unspent transaction output (UTXO) in the block chain in memory. This is called the _UTXO set_.
//! * Updating the UTXO set with valid transactions that have already been anchored into a valid block. This includes:
//!     - Removing the UTXOs that the transaction spends as inputs.
//!     - Adding a new UTXO for every output in the transaction.
use std::{
    cmp::{max, min, Ordering},
    collections::{HashMap, HashSet},
    convert::TryFrom,
    time::Duration,
};

use actix::{
    prelude::*, ActorFuture, AsyncContext, Context, ContextFutureSpawner, Supervised,
    SystemService, WrapFuture,
};
use ansi_term::Color::{Purple, White, Yellow};
use failure::Fail;
use futures::future::{join_all, Future};
use itertools::Itertools;
use witnet_crypto::{hash::calculate_sha256, key::CryptoEngine};
use witnet_data_structures::{
    chain::{
        penalize_factor, reputation_issuance, Alpha, AltKeys, Block, BlockHeader, Bn256PublicKey,
        ChainInfo, ChainState, CheckpointBeacon, CheckpointVRF, ConsensusConstants,
        DataRequestReport, Epoch, EpochConstants, Hash, Hashable, InventoryItem, NodeStats,
        OwnUnspentOutputsPool, PublicKeyHash, Reputation, ReputationEngine, SignaturesToVerify,
        SuperBlock, SuperBlockVote, TransactionsPool, UnspentOutputsPool,
    },
    data_request::DataRequestPool,
    radon_report::{RadonReport, ReportContext},
    superblock::{ARSIdentities, AddSuperBlockVote, SuperBlockConsensus},
    transaction::{TallyTransaction, Transaction},
    types::LastBeacon,
    vrf::VrfCtx,
};
use witnet_rad::types::RadonTypes;
use witnet_util::timestamp::seconds_to_human_string;
use witnet_validations::validations::{
    compare_block_candidates, validate_block, validate_block_transactions,
    validate_new_transaction, verify_signatures, Diff, VrfSlots,
};

use crate::{
    actors::{
        chain_manager::handlers::SYNCED_BANNER,
        inventory_manager::InventoryManager,
        json_rpc::JsonRpcServer,
        messages::{
            AddItem, AddItems, AddTransaction, Anycast, BlockNotify, Broadcast,
            GetBlocksEpochRange, GetItemBlock, SendInventoryItem, SendLastBeacon,
            SendSuperBlockVote, StoreInventoryItem, SuperBlockNotify,
        },
        sessions_manager::SessionsManager,
        storage_keys,
    },
    signature_mngr, storage_mngr,
};

mod actor;
mod handlers;
mod mining;
/// High level transaction factory
pub mod transaction_factory;

/// Maximum blocks number to be sent during synchronization process
pub const MAX_BLOCKS_SYNC: usize = 500;

/// Possible errors when interacting with ChainManager
#[derive(Debug, PartialEq, Fail)]
pub enum ChainManagerError {
    /// A block being processed was already known to this node
    #[fail(display = "A block being processed was already known to this node")]
    BlockAlreadyExists,
    /// A block does not exist
    #[fail(display = "A block does not exist")]
    BlockDoesNotExist,
    /// Optional fields of ChainManager are not properly initialized yet
    #[fail(display = "ChainManager is not ready yet")]
    ChainNotReady,
    /// The node attempted to do an action that is only allowed while `ChainManager`
    /// is in `Synced` state.
    #[fail(
        display = "The node is not yet in `Synced` state (current state is {:?})",
        current_state
    )]
    NotSynced {
        /// Tells what the current state is, so users can better get an idea of why an action is
        /// not possible at this time.
        current_state: StateMachine,
    },
    /// The node is trying to mine a block so commits are not allowed
    #[fail(display = "Commit received while node is trying to mine a block")]
    TooLateToCommit,
    /// The node received a batch of blocks that is inconsistent with the current index
    #[fail(
        display = "Wrong number of blocks provided {:?} for superblock index {:?} and epoch {:?})",
        wrong_index, consolidated_superblock_index, current_superblock_index
    )]
    WrongBlocksForSuperblock {
        /// Tells what the wrong index was
        wrong_index: u32,
        /// Tells what the current superblock index was
        consolidated_superblock_index: u32,
        /// Tells what the current epoch was
        current_superblock_index: u32,
    },
}

/// State Machine
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum StateMachine {
    /// First state, ChainManager is waiting to consensus between its peers
    WaitingConsensus,
    /// Second state, ChainManager synchronization process
    Synchronizing,
    /// Third state, `ChainManager` has all the blocks in the chain and is ready to start
    /// consolidating block candidates in real time.
    AlmostSynced,
    /// Fourth state, `ChainManager` can consolidate block candidates, propose its own
    /// candidates (mining) and participate in resolving data requests (witnessing).
    Synced,
}

impl Default for StateMachine {
    fn default() -> Self {
        StateMachine::WaitingConsensus
    }
}

/// Synchronization target determined by the beacons received from outbound peers
#[derive(Clone, Copy, Debug)]
pub struct SyncTarget {
    // TODO: the target block must be set, but the node will not assume that it is valid
    block: CheckpointBeacon,
    // The target superblock must always be set. Here we only know the superblock index and hash,
    // we do not know the block hash. The block index can be derived from the superblock index.
    // This must be a superblock beacon consolidated with more than 2/3 of the votes, and it must be
    // irreversibly consolidated when reached.
    superblock: CheckpointBeacon,
}

////////////////////////////////////////////////////////////////////////////////////////
// ACTOR BASIC STRUCTURE
////////////////////////////////////////////////////////////////////////////////////////
/// ChainManager actor
#[derive(Debug, Default)]
pub struct ChainManager {
    /// Blockchain state data structure
    chain_state: ChainState,
    /// ChainState backup used to reset the state after a reorganization
    chain_state_snapshot: ChainStateSnapshot,
    /// Current Epoch
    current_epoch: Option<Epoch>,
    /// Transactions Pool (_mempool_)
    transactions_pool: TransactionsPool,
    /// Mining enabled
    mining_enabled: bool,
    /// state of the state machine
    sm_state: StateMachine,
    /// The best beacon known to this node—to which it will try to catch up
    sync_target: Option<SyncTarget>,
    /// The node asked for a batch of blocks on this epoch. This is used to implement a timeout
    /// that will move the node back to WaitingConsensus state if it does not receive any AddBlocks
    /// message after a certain number of epochs
    sync_waiting_for_add_blocks_since: Option<Epoch>,
    /// Map that stores candidate blocks for further validation and consolidation as tip of the blockchain
    /// (block_hash, block))
    candidates: HashMap<Hash, Block>,
    /// Best candidate
    best_candidate: Option<BlockCandidate>,
    /// Set that stores all the received candidates
    seen_candidates: HashSet<Hash>,
    /// Our public key hash, used to create the mint transaction
    own_pkh: Option<PublicKeyHash>,
    /// Our BLS public key, used to append in commit transactions
    bn256_public_key: Option<Bn256PublicKey>,
    /// VRF context
    vrf_ctx: Option<VrfCtx>,
    /// Sign and verify context
    secp: Option<CryptoEngine>,
    /// Peers beacons boolean
    peers_beacons_received: bool,
    /// Consensus parameter (in %)
    consensus_c: u32,
    /// Constants used to convert between epoch and timestamp
    epoch_constants: Option<EpochConstants>,
    /// Maximum number of sources to retrieve in a single epoch
    data_request_max_retrievals_per_epoch: u16,
    /// Timeout for data request retrieval and aggregation execution
    data_request_timeout: Option<Duration>,
    /// Pending transaction timeout
    tx_pending_timeout: u64,
    /// Magic number from ConsensusConstants
    magic: u16,
    /// External mint address
    external_address: Option<PublicKeyHash>,
    /// Mint Percentage to share with the external address
    external_percentage: u8,
    /// List of superblock votes received while we are synchronizing
    temp_superblock_votes: Vec<SuperBlockVote>,
}

/// Wrapper around a block candidate that contains additional metadata regarding
/// needed chain state mutations in case the candidate gets consolidated.
#[derive(Debug)]
pub struct BlockCandidate {
    /// Block
    pub block: Block,
    /// Utxo diff
    pub utxo_diff: Diff,
    /// Reputation
    pub reputation: Reputation,
    /// Vrf proof
    pub vrf_proof: Hash,
}

/// Required trait for being able to retrieve ChainManager address from registry
impl Supervised for ChainManager {}

/// Required trait for being able to retrieve ChainManager address from registry
impl SystemService for ChainManager {}

/// Auxiliary methods for ChainManager actor
impl ChainManager {
    /// Persist previous chain state into storage
    fn persist_chain_state(&mut self, superblock_index: u32, ctx: &mut Context<Self>) {
        let previous_chain_state = self.chain_state_snapshot.restore(superblock_index);
        if previous_chain_state.is_none() {
            return;
        }
        let previous_chain_state = previous_chain_state.unwrap();

        // When updating the chain state, we need to update the highest superblock checkpoint.
        // This is the highest superblock that obtained a majority of votes and we do not want to
        // lose it when restoring the state.
        let state = ChainState {
            chain_info: Some(ChainInfo {
                highest_superblock_checkpoint: self.get_superblock_beacon(),
                ..previous_chain_state.chain_info.as_ref().unwrap().clone()
            }),
            superblock_state: self.chain_state.superblock_state.clone(),
            ..previous_chain_state
        };

        let chain_beacon = state.get_chain_beacon();
        let superblock_beacon = state.get_superblock_beacon();

        log::debug!(
            "Persisting chain state for superblock #{} with chain beacon {:?} and super beacon {:?}",
            superblock_index,
            chain_beacon,
            superblock_beacon
        );

        assert_eq!(superblock_beacon.checkpoint, superblock_index);

        storage_mngr::put(&storage_keys::chain_state_key(self.get_magic()), &state)
            .into_actor(self)
            .and_then(|_, _, _| {
                log::debug!("Successfully persisted previous_chain_info into storage");
                fut::ok(())
            })
            .map_err(|err, _, _| {
                log::error!(
                    "Failed to persist previous_chain_info into storage: {}",
                    err
                )
            })
            .wait(ctx);
    }

    /// Replace `previous_chain_state` with current `chain_state`
    fn move_chain_state_forward(&mut self, superblock_index: u32) {
        self.chain_state_snapshot
            .take(superblock_index, &self.chain_state);
    }

    /// Method to Send items to Inventory Manager
    fn persist_items(&self, ctx: &mut Context<Self>, items: Vec<StoreInventoryItem>) {
        // Get InventoryManager address
        let inventory_manager_addr = InventoryManager::from_registry();

        // Persist block into storage through InventoryManager. `AsyncContext::wait` registers
        // future within context, but context waits until this future resolves
        // before processing any other events.
        inventory_manager_addr
            .send(AddItems { items })
            .into_actor(self)
            .then(|res, _act, _ctx| match res {
                // Process the response from InventoryManager
                Err(e) => {
                    // Error when sending message
                    log::error!("Unsuccessful communication with InventoryManager: {}", e);
                    actix::fut::err(())
                }
                Ok(_) => actix::fut::ok(()),
            })
            .wait(ctx)
    }

    /// Method to persist a Data Request into the Storage
    fn persist_data_requests(&self, ctx: &mut Context<Self>, dr_reports: Vec<DataRequestReport>) {
        let kvs: Vec<_> = dr_reports
            .into_iter()
            .map(|dr_report| {
                let dr_pointer = &dr_report.tally.dr_pointer;
                let dr_pointer_string = format!("DR-REPORT-{}", dr_pointer);

                (dr_pointer_string, dr_report)
            })
            .collect();
        let kvs_len = kvs.len();
        storage_mngr::put_batch(&kvs)
            .into_actor(self)
            .map_err(|e, _, _| {
                log::error!("Failed to persist data request report into storage: {}", e)
            })
            .and_then(move |_, _, _| {
                log::trace!(
                    "Successfully persisted reports for {} data requests into storage",
                    kvs_len
                );
                fut::ok(())
            })
            .wait(ctx);
    }

    fn broadcast_item(&self, item: InventoryItem) {
        // Get SessionsManager address
        let sessions_manager_addr = SessionsManager::from_registry();

        sessions_manager_addr.do_send(Broadcast {
            command: SendInventoryItem { item },
            only_inbound: false,
        });
    }

    fn process_requested_block(
        &mut self,
        ctx: &mut Context<Self>,
        block: Block,
    ) -> Result<(), failure::Error> {
        if let (
            Some(epoch_constants),
            Some(chain_info),
            Some(rep_engine),
            Some(vrf_ctx),
            Some(secp_ctx),
        ) = (
            self.epoch_constants,
            self.chain_state.chain_info.as_ref(),
            self.chain_state.reputation_engine.as_ref(),
            self.vrf_ctx.as_mut(),
            self.secp.as_ref(),
        ) {
            if self.current_epoch.is_none() {
                log::trace!("Called process_requested_block when current_epoch is None");
            }
            let block_number = self.chain_state.block_number();
            let mut vrf_input = chain_info.highest_vrf_output;
            vrf_input.checkpoint = block.block_header.beacon.checkpoint;

            let utxo_diff = process_validations(
                &block,
                self.current_epoch.unwrap_or_default(),
                vrf_input,
                chain_info.highest_block_checkpoint,
                rep_engine,
                epoch_constants,
                &self.chain_state.unspent_outputs_pool,
                &self.chain_state.data_request_pool,
                vrf_ctx,
                secp_ctx,
                block_number,
                &chain_info.consensus_constants,
            )?;

            // Persist block and update ChainState
            self.consolidate_block(ctx, block, utxo_diff);

            Ok(())
        } else {
            Err(ChainManagerError::ChainNotReady.into())
        }
    }
    #[allow(clippy::map_entry)]
    fn process_candidate(&mut self, block: Block) {
        if let (Some(current_epoch), Some(chain_info), Some(rep_engine), Some(vrf_ctx)) = (
            self.current_epoch,
            self.chain_state.chain_info.as_ref(),
            self.chain_state.reputation_engine.as_ref(),
            self.vrf_ctx.as_mut(),
        ) {
            let hash_block = block.hash();
            // If this candidate has not been seen before, validate it
            if self.seen_candidates.insert(hash_block) {
                if self.sm_state == StateMachine::WaitingConsensus
                    || self.sm_state == StateMachine::Synchronizing
                {
                    self.candidates.insert(hash_block, block);

                    return;
                }

                let mut vrf_input = chain_info.highest_vrf_output;
                vrf_input.checkpoint = current_epoch;
                let target_vrf_slots = VrfSlots::from_rf(
                    u32::try_from(rep_engine.ars().active_identities_number()).unwrap(),
                    chain_info.consensus_constants.mining_replication_factor,
                    chain_info.consensus_constants.mining_backup_factor,
                    block.block_header.beacon.checkpoint,
                    chain_info.consensus_constants.initial_difficulty,
                    chain_info
                        .consensus_constants
                        .epochs_with_initial_difficulty,
                );
                let block_pkh = &block.block_sig.public_key.pkh();
                let reputation = rep_engine.trs().get(block_pkh);
                let vrf_proof = match block.block_header.proof.proof.proof_to_hash(vrf_ctx) {
                    Ok(vrf) => vrf,
                    Err(e) => {
                        log::warn!(
                            "Block candidate has an invalid mining eligibility proof: {}",
                            e
                        );
                        return;
                    }
                };

                if let Some(best_candidate) = &self.best_candidate {
                    let best_hash = best_candidate.block.hash();
                    if compare_block_candidates(
                        hash_block,
                        reputation,
                        vrf_proof,
                        best_hash,
                        best_candidate.reputation,
                        best_candidate.vrf_proof,
                        &target_vrf_slots,
                    ) != Ordering::Greater
                    {
                        log::debug!("Ignoring new block candidate ({}) because a better one ({}) has been already validated", hash_block, best_hash);
                        return;
                    }
                }
                match process_validations(
                    &block,
                    current_epoch,
                    vrf_input,
                    chain_info.highest_block_checkpoint,
                    rep_engine,
                    self.epoch_constants.unwrap(),
                    &self.chain_state.unspent_outputs_pool,
                    &self.chain_state.data_request_pool,
                    // The unwrap is safe because if there is no VRF context,
                    // the actor should have stopped execution
                    self.vrf_ctx.as_mut().expect("No initialized VRF context"),
                    self.secp
                        .as_ref()
                        .expect("No initialized SECP256K1 context"),
                    self.chain_state.block_number(),
                    &chain_info.consensus_constants,
                ) {
                    Ok(utxo_diff) => {
                        self.best_candidate = Some(BlockCandidate {
                            block: block.clone(),
                            utxo_diff,
                            reputation,
                            vrf_proof,
                        });

                        self.broadcast_item(InventoryItem::Block(block));
                    }
                    Err(e) => log::warn!(
                        "Error when processing a block candidate {}: {}",
                        hash_block,
                        e
                    ),
                }
            } else {
                log::trace!("Block candidate already seen: {}", hash_block);
            }
        } else {
            log::warn!("ChainManager doesn't have current epoch");
        }
    }

    fn persist_blocks_batch(&self, ctx: &mut Context<Self>, blocks: Vec<Block>) {
        let mut to_persist = Vec::with_capacity(blocks.len());
        for block in blocks {
            to_persist.push(StoreInventoryItem::Block(Box::new(block)));
        }

        self.persist_items(ctx, to_persist);
    }

    fn consolidate_block(&mut self, ctx: &mut Context<Self>, block: Block, utxo_diff: Diff) {
        // Update chain_info and reputation_engine
        let epoch_constants = match self.epoch_constants {
            Some(x) => x,
            None => {
                log::error!("No EpochConstants loaded in ChainManager");
                return;
            }
        };
        match self.chain_state {
            ChainState {
                chain_info: Some(ref mut chain_info),
                reputation_engine: Some(ref mut reputation_engine),
                ..
            } => {
                let block_hash = block.hash();
                let block_epoch = block.block_header.beacon.checkpoint;

                // Update `highest_block_checkpoint`
                let beacon = CheckpointBeacon {
                    checkpoint: block_epoch,
                    hash_prev_block: block_hash,
                };

                // Get VRF context
                let vrf_ctx = match self.vrf_ctx.as_mut() {
                    Some(x) => x,
                    None => {
                        log::error!("No VRF context available");
                        return;
                    }
                };

                // Decide the input message for the VRF of this block candidate:
                // If the candidate builds right on top of the genesis block, use candidate's own checkpoint and the genesis block hash.
                // Else, use use candidate's own checkpoint and the hash of the VRF proof from the block it builds on.
                let vrf_input = match block_epoch {
                    0 => CheckpointVRF {
                        checkpoint: block_epoch,
                        hash_prev_vrf: block_hash,
                    },
                    _ => {
                        let proof_hash = block.block_header.proof.proof_to_hash(vrf_ctx).unwrap();
                        CheckpointVRF {
                            checkpoint: block_epoch,
                            hash_prev_vrf: proof_hash,
                        }
                    }
                };

                // Print reputation logs on debug level on Synced state,
                // but on trace level while synchronizing
                let log_level = if let StateMachine::Synced = self.sm_state {
                    log::Level::Debug
                } else {
                    log::Level::Trace
                };

                // Update beacon and vrf output
                chain_info.highest_block_checkpoint = beacon;
                chain_info.highest_vrf_output = vrf_input;

                let rep_info = update_pools(
                    &block,
                    &mut self.chain_state.unspent_outputs_pool,
                    &mut self.chain_state.data_request_pool,
                    &mut self.transactions_pool,
                    utxo_diff,
                    self.own_pkh,
                    &mut self.chain_state.own_utxos,
                    epoch_constants,
                    &mut self.chain_state.node_stats,
                );

                let miner_pkh = block.block_header.proof.proof.pkh();

                // Do not update reputation when consolidating genesis block
                if block_hash != chain_info.consensus_constants.genesis_hash {
                    update_reputation(
                        reputation_engine,
                        &mut self.chain_state.alt_keys,
                        &chain_info.consensus_constants,
                        miner_pkh,
                        rep_info,
                        log_level,
                        block_epoch,
                        self.own_pkh.unwrap_or_default(),
                    );
                }

                // Update bn256 public keys with block information
                self.chain_state.alt_keys.insert_keys_from_block(&block);

                // Insert candidate block into `block_chain` state
                self.chain_state.block_chain.insert(block_epoch, block_hash);

                match self.sm_state {
                    StateMachine::WaitingConsensus => {
                        // Persist finished data requests into storage
                        let to_be_stored =
                            self.chain_state.data_request_pool.finished_data_requests();
                        self.persist_data_requests(ctx, to_be_stored);

                        let _reveals = self
                            .chain_state
                            .data_request_pool
                            .update_data_request_stages();

                        self.persist_items(ctx, vec![StoreInventoryItem::Block(Box::new(block))]);
                    }
                    StateMachine::Synchronizing => {
                        // In Synchronizing stage, blocks and data requests are persisted
                        // trough batches in AddBlocks handler
                        let _reveals = self
                            .chain_state
                            .data_request_pool
                            .update_data_request_stages();
                    }
                    StateMachine::AlmostSynced | StateMachine::Synced => {
                        // Persist finished data requests into storage
                        let to_be_stored =
                            self.chain_state.data_request_pool.finished_data_requests();
                        for dr_report in &to_be_stored {
                            show_tally_info(&dr_report.tally, block_epoch);
                        }
                        self.persist_data_requests(ctx, to_be_stored);

                        let reveals = self
                            .chain_state
                            .data_request_pool
                            .update_data_request_stages();

                        if block_hash == self.chain_state.node_stats.last_block_proposed {
                            self.chain_state.node_stats.block_mined_count += 1;
                            log::info!("Congratulations! Your block was consolidated into the block chain by an apparent majority of peers");
                        }

                        show_info_dr(&self.chain_state.data_request_pool, &block);

                        for reveal in reveals {
                            // Send AddTransaction message to self
                            // And broadcast it to all of peers
                            ctx.address().do_send(AddTransaction {
                                transaction: Transaction::Reveal(reveal),
                            })
                        }
                        // Persist blocks and transactions but do not persist chain_state, it will
                        // be persisted on superblock consolidation
                        // FIXME #1437: discard persisted and non-consolidated blocks
                        // This means that after a reorganization a call to getBlock or
                        // getTransaction will show the content without any warning that the block
                        // is not on the main chain. To fix this we could remove forked blocks when
                        // a reorganization is detected.
                        self.persist_items(
                            ctx,
                            vec![StoreInventoryItem::Block(Box::new(block.clone()))],
                        );

                        // Send notification to JsonRpcServer
                        JsonRpcServer::from_registry().do_send(BlockNotify { block })
                    }
                }
            }
            _ => {
                log::error!("No ChainInfo loaded in ChainManager");
            }
        }
    }

    /// Create a superblock, sign a superblock vote and broadcast it
    fn create_and_broadcast_superblock(&mut self, ctx: &mut Context<Self>, current_epoch: u32) {
        self.construct_superblock(ctx, current_epoch)
            .and_then(move |superblock, act, _ctx| {
                let superblock_hash = superblock.hash();
                log::debug!(
                    "Local SUPERBLOCK #{} {}: {:?}",
                    superblock.index,
                    superblock_hash,
                    superblock
                );

                // TODO: Check if it is needed to create a superblock vote before doing it
                let mut superblock_vote =
                    SuperBlockVote::new_unsigned(superblock_hash, superblock.index);
                let bn256_message = superblock_vote.bn256_signature_message();

                signature_mngr::bn256_sign(bn256_message)
                    .map_err(|e| {
                        log::error!("Failed to sign superblock with bn256 key: {}", e);
                    })
                    .and_then(move |bn256_keyed_signature| {
                        // Actually, we don't need to include the BN256 public key because
                        // it is stored in the `alt_keys` mapping, indexed by the
                        // secp256k1 public key hash
                        let bn256_signature = bn256_keyed_signature.signature;
                        superblock_vote.set_bn256_signature(bn256_signature);
                        let secp256k1_message = superblock_vote.secp256k1_signature_message();
                        let sign_bytes = calculate_sha256(&secp256k1_message).0;
                        signature_mngr::sign_data(sign_bytes)
                            .map(move |secp256k1_signature| {
                                superblock_vote.set_secp256k1_signature(secp256k1_signature);

                                superblock_vote
                            })
                            .map_err(|e| {
                                log::error!("Failed to sign superblock with secp256k1 key: {}", e);
                            })
                    })
                    .into_actor(act)
                    .and_then(|res, act, ctx| match act.add_superblock_vote(res, ctx) {
                        Ok(()) => actix::fut::ok(()),
                        Err(e) => {
                            log::error!(
                                "Error when broadcasting recently created superblock: {}",
                                e
                            );

                            actix::fut::err(())
                        }
                    })
            })
            .wait(ctx)
    }

    fn get_chain_beacon(&self) -> CheckpointBeacon {
        self.chain_state.get_chain_beacon()
    }

    /// Retrieve the last consolidated superblock hash and index.
    fn get_superblock_beacon(&self) -> CheckpointBeacon {
        self.chain_state.get_superblock_beacon()
    }

    fn consensus_constants(&self) -> ConsensusConstants {
        self.chain_state.get_consensus_constants()
    }

    fn add_temp_superblock_votes(&mut self, ctx: &mut Context<Self>) -> Result<(), failure::Error> {
        for superblock_vote in std::mem::take(&mut self.temp_superblock_votes) {
            log::debug!("add_temp_superblock_votes {:?}", superblock_vote);
            // Check if we already received this vote
            if self.chain_state.superblock_state.contains(&superblock_vote) {
                return Ok(());
            }

            // Validate secp256k1 signature
            signature_mngr::verify_signatures(vec![SignaturesToVerify::SuperBlockVote {
                superblock_vote: superblock_vote.clone(),
            }])
            .map_err(|e| {
                log::error!("Verify superblock vote signature: {}", e);
            })
            .into_actor(self)
            .and_then(move |(), act, _ctx| {
                // Check if we already received this vote (again, because this future can be executed
                // by multiple tasks concurrently)
                if act.chain_state.superblock_state.contains(&superblock_vote) {
                    return actix::fut::ok(());
                }
                act.chain_state.superblock_state.add_vote(&superblock_vote);

                actix::fut::ok(())
            })
            .spawn(ctx);
        }

        Ok(())
    }

    fn add_superblock_vote(
        &mut self,
        superblock_vote: SuperBlockVote,
        ctx: &mut Context<Self>,
    ) -> Result<(), failure::Error> {
        log::trace!(
            "AddSuperBlockVote received while StateMachine is in state {:?}",
            self.sm_state
        );

        if self.sm_state != StateMachine::Synced {
            self.temp_superblock_votes.push(superblock_vote.clone());
        }

        // Check if we already received this vote
        if self.chain_state.superblock_state.contains(&superblock_vote) {
            return Ok(());
        }

        // Validate secp256k1 signature
        signature_mngr::verify_signatures(vec![SignaturesToVerify::SuperBlockVote {
            superblock_vote: superblock_vote.clone(),
        }])
        .map_err(|e| {
            log::error!("Verify superblock vote signature: {}", e);
        })
        .into_actor(self)
        .and_then(move |(), act, _ctx| {
            // Check if we already received this vote (again, because this future can be executed
            // by multiple tasks concurrently)
            // Note: the `fut:err` is used to signal that vote shouldn't be broadcasted (again)
            if act.chain_state.superblock_state.contains(&superblock_vote) {
                return actix::fut::err(());
            }
            // Validate vote: the identity should be able to vote
            // We broadcast all superblock votes with valid secp256k1 signature, signed by members
            // of the ARS, even if the superblock hash is different from our local superblock hash.
            // If the superblock index is different from the current one we cannot check ARS membership,
            // so we broadcast it if the index is within an acceptable range (not too old).
            let should_broadcast = match act.chain_state.superblock_state.add_vote(&superblock_vote)
            {
                AddSuperBlockVote::AlreadySeen => false,
                AddSuperBlockVote::DoubleVote => {
                    // We must forward double votes to make sure all the nodes are aware of them
                    log::debug!(
                        "Identity voted more than once: {}",
                        superblock_vote.secp256k1_signature.public_key.pkh()
                    );

                    true
                }
                AddSuperBlockVote::InvalidIndex => {
                    log::debug!(
                        "Not forwarding superblock vote: invalid superblock index: {}",
                        superblock_vote.superblock_index
                    );

                    false
                }
                AddSuperBlockVote::NotInSigningCommittee => {
                    log::debug!(
                        "Not forwarding superblock vote: identity not in Signing Committee: {}",
                        superblock_vote.secp256k1_signature.public_key.pkh()
                    );

                    false
                }
                AddSuperBlockVote::MaybeValid
                | AddSuperBlockVote::ValidButDifferentHash
                | AddSuperBlockVote::ValidWithSameHash => true,
            };

            actix::fut::result(if should_broadcast {
                Ok(superblock_vote)
            } else {
                Err(())
            })
        })
        .and_then(|superblock_vote, act, _ctx| {
            // Broadcast vote
            SessionsManager::from_registry()
                .send(Broadcast {
                    command: SendSuperBlockVote { superblock_vote },
                    only_inbound: false,
                })
                .map_err(|e| {
                    log::error!("Forward superblock vote: {}", e);
                })
                .into_actor(act)
        })
        .spawn(ctx);

        Ok(())
    }

    #[must_use]
    fn add_transaction(
        &mut self,
        msg: AddTransaction,
        timestamp_now: i64,
    ) -> ResponseActFuture<Self, (), failure::Error> {
        log::trace!(
            "AddTransaction received while StateMachine is in state {:?}",
            self.sm_state
        );
        // Ignore AddTransaction when not in Synced state
        match self.sm_state {
            StateMachine::Synced => {}
            _ => {
                return Box::new(actix::fut::err(
                    ChainManagerError::NotSynced {
                        current_state: self.sm_state,
                    }
                    .into(),
                ));
            }
        };

        match self.transactions_pool.contains(&msg.transaction) {
            Ok(false) => {
                self.transactions_pool
                    .insert_pending_transaction(&msg.transaction);
            }
            Ok(true) => {
                log::trace!(
                    "Transaction is already in the pool: {}",
                    msg.transaction.hash()
                );
                return Box::new(actix::fut::ok(()));
            }
            Err(e) => {
                log::warn!("Cannot add transaction: {}", e);
                return Box::new(actix::fut::err(e.into()));
            }
        }

        if let (
            Some(chain_info),
            Some(reputation_engine),
            Some(current_epoch),
            Some(epoch_constants),
        ) = (
            self.chain_state.chain_info.as_ref(),
            self.chain_state.reputation_engine.as_ref(),
            self.current_epoch,
            self.epoch_constants,
        ) {
            if let Transaction::Commit(_commit) = &msg.transaction {
                let timestamp_mining = epoch_constants
                    .block_mining_timestamp(current_epoch)
                    .unwrap();

                if timestamp_now > timestamp_mining {
                    let e = ChainManagerError::TooLateToCommit;
                    log::debug!("{}", e);
                    return Box::new(actix::fut::err(e.into()));
                }
            }

            let mut signatures_to_verify = vec![];
            let mut vrf_input = chain_info.highest_vrf_output;
            vrf_input.checkpoint = current_epoch;
            let fut = futures::future::result(validate_new_transaction(
                &msg.transaction,
                (
                    reputation_engine,
                    &self.chain_state.unspent_outputs_pool,
                    &self.chain_state.data_request_pool,
                ),
                vrf_input,
                current_epoch,
                epoch_constants,
                self.chain_state.block_number(),
                &mut signatures_to_verify,
                chain_info.consensus_constants.collateral_minimum,
                chain_info.consensus_constants.collateral_age,
                chain_info.consensus_constants.max_vt_weight,
                chain_info.consensus_constants.max_dr_weight,
            ))
            .into_actor(self)
            .and_then(|fee, act, _ctx| {
                signature_mngr::verify_signatures(signatures_to_verify)
                    .map(move |_| fee)
                    .into_actor(act)
            })
            .then(|res, act, _ctx| match res {
                Ok(fee) => {
                    // Broadcast valid transaction
                    act.broadcast_item(InventoryItem::Transaction(msg.transaction.clone()));

                    // Add valid transaction to transactions_pool
                    let tx_hash = msg.transaction.hash();
                    let removed_transactions = act.transactions_pool.insert(msg.transaction, fee);
                    log_removed_transactions(&removed_transactions, tx_hash);

                    actix::fut::ok(())
                }
                Err(e) => {
                    log::warn!(
                        "Error when validating transaction {}: {}",
                        msg.transaction.hash(),
                        e
                    );

                    actix::fut::err(e)
                }
            });

            Box::new(fut)
        } else {
            Box::new(actix::fut::err(ChainManagerError::ChainNotReady.into()))
        }
    }

    /// Set Magic number
    pub fn set_magic(&mut self, new_magic: u16) {
        self.magic = new_magic;
    }

    /// Get Magic number
    pub fn get_magic(&self) -> u16 {
        self.magic
    }

    /// Build and vote candidate superblock process which uses futures
    #[must_use]
    pub fn build_and_vote_candidate_superblock(
        &mut self,
        ctx: &mut Context<Self>,
        superblock_epoch: u32,
    ) -> ResponseActFuture<Self, (), ()> {
        let fut = self.construct_superblock(ctx, superblock_epoch).and_then(
            move |superblock, act, _ctx| {
                let superblock_hash = superblock.hash();
                log::debug!(
                    "Local SUPERBLOCK #{} {}: {:?}",
                    superblock.index,
                    superblock_hash,
                    superblock
                );

                let mut superblock_vote =
                    SuperBlockVote::new_unsigned(superblock_hash, superblock.index);
                let bn256_message = superblock_vote.bn256_signature_message();

                signature_mngr::bn256_sign(bn256_message)
                    .map_err(|e| {
                        log::error!("Failed to sign superblock with bn256 key: {}", e);
                    })
                    .and_then(move |bn256_keyed_signature| {
                        // There is no need to include the BN256 public key because it is stored in
                        // the `alt_keys` mapping, indexed by the secp256k1 public key hash
                        superblock_vote.set_bn256_signature(bn256_keyed_signature.signature);
                        let secp256k1_message = superblock_vote.secp256k1_signature_message();
                        let sign_bytes = calculate_sha256(&secp256k1_message).0;
                        signature_mngr::sign_data(sign_bytes)
                            .map(move |secp256k1_signature| {
                                superblock_vote.set_secp256k1_signature(secp256k1_signature);

                                superblock_vote
                            })
                            .map_err(|e| {
                                log::error!("Failed to sign superblock with secp256k1 key: {}", e);
                            })
                    })
                    .into_actor(act)
                    .and_then(|res, act, ctx| match act.add_superblock_vote(res, ctx) {
                        Ok(()) => actix::fut::ok(()),
                        Err(e) => {
                            log::error!(
                                "Error when broadcasting recently created superblock: {}",
                                e
                            );

                            actix::fut::err(())
                        }
                    })
            },
        );

        Box::new(fut)
    }

    /// Try to consolidate superblock process which uses futures
    #[must_use]
    pub fn try_consolidate_superblock(
        &mut self,
        ctx: &mut Context<Self>,
        block_epoch: u32,
        sync_target: SyncTarget,
    ) -> ResponseActFuture<Self, (), ()> {
        let fut =
            self.construct_superblock(ctx, block_epoch)
                .and_then(move |superblock, act, ctx| {
                    if superblock.hash() == sync_target.superblock.hash_prev_block {
                        // In synchronizing state, the consensus beacon is the one we just created
                        act.chain_state
                            .chain_info
                            .as_mut()
                            .unwrap()
                            .highest_superblock_checkpoint =
                            act.chain_state.superblock_state.get_beacon();
                        log::info!(
                            "Consensus while sync! Superblock {:?}",
                            act.get_superblock_beacon()
                        );

                        // Copy current chain state into previous chain state, and persist it
                        act.move_chain_state_forward(sync_target.superblock.checkpoint);
                        act.persist_chain_state(sync_target.superblock.checkpoint, ctx);

                        actix::fut::ok(())
                    } else {
                        // The superblock hash is different from what it should be.
                        log::error!(
                            "Mismatching superblock. Target: {:?} Created #{} {} {:?}",
                            sync_target,
                            superblock.index,
                            superblock.hash(),
                            superblock
                        );
                        act.update_state_machine(StateMachine::WaitingConsensus);
                        act.initialize_from_storage(ctx);
                        log::info!("Restored chain state from storage");

                        // If we are not synchronizing, forget about when we started synchronizing
                        act.sync_waiting_for_add_blocks_since = None;
                        actix::fut::err(())
                    }
                });

        Box::new(fut)
    }

    /// Construct superblock process which uses futures
    #[must_use]
    pub fn construct_superblock(
        &mut self,
        ctx: &mut Context<Self>,
        block_epoch: u32,
    ) -> ResponseActFuture<Self, SuperBlock, ()> {
        let consensus_constants = self.consensus_constants();

        let superblock_period = u32::from(consensus_constants.superblock_period);

        let superblock_index = block_epoch / superblock_period;
        if superblock_index == 0 {
            panic!(
                "Superblock 0 should not be created! Block epoch: {}",
                block_epoch
            );
        }
        // This is the superblock for which we will be counting votes, and if there is consensus,
        // it will be the new consolidated superblock
        let voted_superblock_beacon = self.chain_state.superblock_state.get_beacon();

        let inventory_manager = InventoryManager::from_registry();

        let init_epoch = block_epoch - superblock_period;
        let final_epoch = block_epoch.saturating_sub(1);
        let genesis_hash = consensus_constants.genesis_hash;

        let fut = futures::future::ok(self.handle(
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
                            log::error!("Error in GetItemBlock {}: {}", hash, e);
                            futures::future::err(())
                        }
                        Err(e) => {
                            log::error!("Error in GetItemBlock {}: {}", hash, e);
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
            .map_err(|e, _, _| log::error!("Superblock building failed: {:?}", e))
            .and_then(move |(block_headers, last_hash), act, ctx| {
                let consensus = if act.sm_state == StateMachine::Synced || act.sm_state == StateMachine::AlmostSynced {
                    if voted_superblock_beacon.checkpoint + 1 != superblock_index {
                        // Warn when there is are missing superblocks between the one that will be
                        // consolidated and the one that will be created
                        log::warn!("Counting votes for Superblock {:?} when the current superblock index is {}", voted_superblock_beacon, superblock_index);
                    }

                    act.chain_state.superblock_state.has_consensus()
                } else {
                    log::debug!("The node is not synced yet, so assume that superblock {:?} is valid", voted_superblock_beacon);

                    // If the node is not synced yet, assume that the superblock is valid.
                    // This is because the node is consolidating blocks received during the synchronization
                    // process, which are assumed to be valid.
                    SuperBlockConsensus::SameAsLocal
                };

                match consensus {
                    SuperBlockConsensus::SameAsLocal => {
                        // At this point we need to persist previous_chain_state,
                        // Take last beacon from superblock state and use it in current chain_info
                        act.chain_state.chain_info.as_mut().unwrap().highest_superblock_checkpoint =
                            act.chain_state.superblock_state.get_beacon();

                        if act.sm_state == StateMachine::Synced || act.sm_state == StateMachine::AlmostSynced {
                            // Persist previous_chain_state with current superblock_state
                            act.persist_chain_state(voted_superblock_beacon.checkpoint, ctx);
                            act.move_chain_state_forward(superblock_index);
                        }

                        if let Some(consolidated_superblock) = act.chain_state.superblock_state.get_current_superblock() {
                            // Let JSON-RPC clients know that the blocks in the previous superblock can now
                            // be considered consolidated
                            act.notify_superblock_consolidation(consolidated_superblock, ctx);

                            log::info!("Consensus reached for Superblock #{}", voted_superblock_beacon.checkpoint);
                            log::debug!("Current tip of the chain: {:?}", act.get_chain_beacon());
                            log::debug!(
                                "The last block of the consolidated superblock is {}",
                                last_hash
                            );
                        }

                        let chain_info = act.chain_state.chain_info.as_ref().unwrap();
                        let reputation_engine = act.chain_state.reputation_engine.as_ref().unwrap();

                        let reputed_ars_members =
                            // Before reaching the epoch activity_period + collateral_age the bootstrap committee signs the superblock
                            // collateral_age is measured in blocks instead of epochs, but this only means that the period in which
                            // the bootstrap committee signs is at least epoch activity_period + collateral_age
                            if block_epoch
                                > chain_info.consensus_constants.collateral_age
                                    + chain_info.consensus_constants.activity_period
                            {
                                let ars_members = reputation_engine.get_rep_ordered_ars_list();
                                let reputed = reputed_ars(&ars_members, &reputation_engine);

                                // In case of no reputed nodes, return all active nodes
                                if reputed.is_empty() {
                                    ars_members
                                } else {
                                    reputed
                                }
                            } else {
                                chain_info
                                    .consensus_constants
                                    .bootstrapping_committee
                                    .iter()
                                    .map(|add| add.parse().expect("Malformed bootstrapping committee"))
                                    .collect()
                            };

                    // Get the list of members of the ARS with reputation greater than 0
                    // the list itself is ordered by decreasing reputation
                    let reputed_ars = ARSIdentities::new(reputed_ars_members);

                    // Committee size should decrease if sufficient epochs have elapsed since last confirmed superblock
                    let committee_size = current_committee_size_requirement(
                        consensus_constants.superblock_signing_committee_size,
                        act.chain_state.superblock_state.get_committee_length(),
                        consensus_constants.superblock_committee_decreasing_period,
                        consensus_constants.superblock_committee_decreasing_step,
                        chain_info.highest_superblock_checkpoint.checkpoint,
                        superblock_index,
                    );
                    log::debug!("The current signing committee size is {}", committee_size);

                    let superblock = act.chain_state.superblock_state.build_superblock(
                        &block_headers,
                        reputed_ars,
                        committee_size,
                        superblock_index,
                        last_hash,
                        &act.chain_state.alt_keys,
                    );

                    // Put the local superblock into chain state
                    act.chain_state
                        .superblock_state
                        .set_current_superblock(superblock.clone());

                    actix::fut::ok(superblock)
                }
                SuperBlockConsensus::Different(target_superblock_hash) => {
                    // No consensus: move to waiting consensus and restore chain_state from storage
                    // TODO: it could be possible to synchronize with a target superblock hash
                    log::warn!(
                        "Superblock consensus {} different from current superblock",
                        target_superblock_hash
                    );
                    act.initialize_from_storage(ctx);
                    act.update_state_machine(StateMachine::WaitingConsensus);

                    actix::fut::err(())
                }
                SuperBlockConsensus::NoConsensus => {
                    // No consensus: move to AlmostSynced and restore chain_state from storage
                    log::warn!("No superblock consensus");
                    act.initialize_from_storage(ctx);
                    act.update_state_machine(StateMachine::AlmostSynced);

                    actix::fut::err(())
                }
                SuperBlockConsensus::Unknown => {
                    // Consensus unknown: move to waiting consensus and restore chain_state from storage
                    log::warn!("Superblock consensus unknown");
                    act.initialize_from_storage(ctx);
                    act.update_state_machine(StateMachine::WaitingConsensus);

                    actix::fut::err(())
                }
            }
        });

        Box::new(fut)
    }

    /// Block validation process which uses futures
    #[must_use]
    pub fn future_process_validations(
        &mut self,
        block: Block,
        current_epoch: Epoch,
        vrf_input: CheckpointVRF,
        chain_beacon: CheckpointBeacon,
        epoch_constants: EpochConstants,
    ) -> ResponseActFuture<Self, Diff, failure::Error> {
        let block_number = self.chain_state.block_number();
        let mut signatures_to_verify = vec![];
        let consensus_constants = self.consensus_constants();

        let fut = futures::future::result(validate_block(
            &block,
            current_epoch,
            vrf_input,
            chain_beacon,
            &mut signatures_to_verify,
            self.chain_state.reputation_engine.as_ref().unwrap(),
            &consensus_constants,
        ))
        .and_then(|()| signature_mngr::verify_signatures(signatures_to_verify))
        .into_actor(self)
        .and_then(move |(), act, _ctx| {
            let mut signatures_to_verify = vec![];
            futures::future::result(validate_block_transactions(
                &act.chain_state.unspent_outputs_pool,
                &act.chain_state.data_request_pool,
                &block,
                vrf_input,
                &mut signatures_to_verify,
                act.chain_state.reputation_engine.as_ref().unwrap(),
                epoch_constants,
                block_number,
                &consensus_constants,
            ))
            .and_then(|diff| signature_mngr::verify_signatures(signatures_to_verify).map(|_| diff))
            .into_actor(act)
        });

        Box::new(fut)
    }

    /// Transition the ChainManager state machine into a new state.
    ///
    /// This is expected to be the only means for updating the state machine, so debugging is easier
    /// and to ensure that every transition gets logged in a predictable format.
    fn update_state_machine(&mut self, next_state: StateMachine) {
        match (&self.sm_state, &next_state) {
            (old, new) if old == new => log::debug!("State machine staying in state {:?}", old),
            (_, StateMachine::Synced) => log::debug!(
                "State machine is transitioning from {:?} into {:?}\n{}",
                self.sm_state,
                next_state,
                SYNCED_BANNER
            ),
            _ => log::debug!(
                "State machine is transitioning from {:?} into {:?}",
                self.sm_state,
                next_state
            ),
        }

        self.sm_state = next_state
    }

    fn request_blocks_batch(&mut self, ctx: &mut Context<Self>) {
        // Send Anycast<SendLastBeacon> to a safu peer in order to begin the synchronization
        SessionsManager::from_registry()
            .send(Anycast {
                command: SendLastBeacon {
                    last_beacon: LastBeacon {
                        highest_block_checkpoint: self.get_chain_beacon(),
                        highest_superblock_checkpoint: self.get_superblock_beacon(),
                    },
                },
                safu: true,
            })
            .into_actor(self)
            .then(|res, act, _ctx| match res {
                Ok(Ok(())) => actix::fut::ok(()),
                _ => {
                    // On error case go back to WaitingConsensus state
                    log::warn!("Failed to send LastBeacon to random peer");
                    if act.sm_state == StateMachine::Synchronizing {
                        act.update_state_machine(StateMachine::WaitingConsensus);
                        act.sync_waiting_for_add_blocks_since = None;
                    }

                    actix::fut::err(())
                }
            })
            .spawn(ctx);
        let epoch = self.current_epoch.unwrap();
        self.sync_waiting_for_add_blocks_since = Some(epoch);
    }

    fn process_blocks_batch(
        &mut self,
        ctx: &mut Context<Self>,
        sync_target: &SyncTarget,
        blocks: &[Block],
    ) -> (bool, usize) {
        let mut batch_succeeded = true;
        let mut num_processed_blocks = 0;

        for block in blocks.iter() {
            if let Err(e) = self.process_requested_block(ctx, block.clone()) {
                log::error!("Error processing block: {}", e);
                if num_processed_blocks > 0 {
                    // Restore only in case there were several blocks consolidated before
                    // This is not needed if the error is in the first block because
                    // the state has not been mutated yet
                    self.initialize_from_storage(ctx);
                    log::info!("Restored chain state from storage");
                }
                batch_succeeded = false;
                break;
            }

            num_processed_blocks += 1;

            let beacon = self.get_chain_beacon();
            show_sync_progress(beacon, &sync_target, self.epoch_constants.unwrap());
        }

        (batch_succeeded, num_processed_blocks)
    }

    fn process_first_batch(
        &mut self,
        ctx: &mut Context<ChainManager>,
        sync_target: &SyncTarget,
        blocks: &[Block],
    ) -> (bool, usize) {
        let (batch_succeeded, num_processed_blocks) =
            self.process_blocks_batch(ctx, &sync_target, &blocks);

        if !batch_succeeded {
            log::error!("Received invalid blocks batch");
            self.update_state_machine(StateMachine::WaitingConsensus);
            self.sync_waiting_for_add_blocks_since = None;
        }

        (batch_succeeded, num_processed_blocks)
    }

    fn superblock_consolidation_is_needed(
        &self,
        sync_target: &SyncTarget,
        superblock_period: u32,
    ) -> Option<u32> {
        if sync_target.superblock.checkpoint
            == self.chain_state.superblock_state.get_beacon().checkpoint
        {
            None
        } else {
            Some(sync_target.superblock.checkpoint * superblock_period)
        }
    }

    /// Let JSON-RPC clients know that the blocks in the previous superblock can now
    /// be considered consolidated
    fn notify_superblock_consolidation(
        &mut self,
        superblock: SuperBlock,
        ctx: &mut Context<ChainManager>,
    ) {
        let superblock_period = u32::from(self.consensus_constants().superblock_period);
        let final_epoch = superblock
            .index
            .checked_mul(superblock_period)
            .expect("Multiplying a superblock index by `superblock_period` should never overflow");
        let initial_epoch = final_epoch.saturating_sub(superblock_period);
        let beacons = self.handle(
            GetBlocksEpochRange::new_with_limit(initial_epoch..final_epoch, 0),
            ctx,
        );

        // If there is a superblock to consolidate, and we got the confirmed block beacons, send
        // notification
        if let Ok(beacons) = beacons {
            let consolidated_block_hashes: Vec<Hash> =
                beacons.iter().cloned().map(|(_epoch, hash)| hash).collect();
            let superblock_notify = SuperBlockNotify {
                superblock,
                consolidated_block_hashes,
            };

            // Store the list of block hashes that pertain to this superblock
            InventoryManager::from_registry().do_send(AddItem {
                item: StoreInventoryItem::Superblock(superblock_notify.clone()),
            });

            JsonRpcServer::from_registry().do_send(superblock_notify);
        }
    }
}

/// Helper struct used to persist an old copy of the `ChainState` to the storage
#[derive(Debug, Default)]
struct ChainStateSnapshot {
    // Previous chain_state and superblock index that corresponds to the last consolidated block.
    // Note that when creating the snapshot, the superblock is not consolidated yet.
    // When the superblock with index n is consolidated by the ChainManager,
    // the state snapshot with superblock index n should be irreversibly persisted into the storage
    previous_chain_state: Option<(ChainState, u32)>,
    // The ChainState at this superblock index is already persisted in the storage
    // Used to detect code that tries to persist old state
    highest_persisted_superblock: u32,
}

impl ChainStateSnapshot {
    // Returns false if the snapshot did already exist
    // Returns true if the snapshot did not already exist
    // Panics if a different chain state was already saved for this super epoch
    fn take(&mut self, superblock_index: u32, state: &ChainState) -> bool {
        let chain_beacon = state.get_chain_beacon();
        let superblock_beacon = state.get_superblock_beacon();

        log::debug!(
            "Taking snapshot at superblock #{}. Chain beacon {:?}, superblock beacon {:?}",
            superblock_index,
            chain_beacon,
            superblock_beacon
        );

        if let Some((prev_chain_state, prev_super_epoch)) = self.previous_chain_state.as_mut() {
            if *prev_super_epoch == superblock_index {
                log::warn!("ChainState snapshot {} already exists", superblock_index);
                if prev_chain_state == state {
                    false
                } else {
                    // Only allow overwriting a different chain state if the superblock index is 0
                    if superblock_index == 0 {
                        log::warn!(
                            "ChainState mismatch in superblock #{}. Overwritting old with new",
                            superblock_index
                        );
                        *prev_chain_state = state.clone();

                        true
                    } else {
                        // Two snapshots of the same superblock should be identical, this is a bug
                        panic!(
                            "ChainState mismatch for superblock #{}: `{:?} != {:?}`",
                            superblock_index, prev_chain_state, state
                        );
                    }
                }
            } else {
                log::warn!(
                    "Overwriting old chain state snapshot, it was superblock #{}",
                    prev_super_epoch
                );
                self.previous_chain_state = Some((state.clone(), superblock_index));

                true
            }
        } else {
            self.previous_chain_state = Some((state.clone(), superblock_index));

            true
        }
    }

    // Returns None if this super_epoch was already consolidated before
    // Returns Some(chain_state) if this super_epoch can be consolidated
    // It is assumed that the caller will persist the chain_state
    fn restore(&mut self, super_epoch: u32) -> Option<ChainState> {
        if super_epoch == 0 {
            // The superblock with index 0 is always consolidated, no need to persist it to storage
            // This is because the superblock 0 does not include any blocks, it is the bootstrap
            // superblock, so there is no state to persist
            log::debug!("Skip persisting superblock #0 because it is already persisted");

            None
        } else if self.highest_persisted_superblock == super_epoch {
            // This can happen during reorganizations
            log::debug!(
                "Tried to persist chain state for superblock #{} but it is already persisted",
                super_epoch
            );

            None
        } else if self.highest_persisted_superblock > super_epoch {
            panic!("Tried to persist chain state for superblock #{} but it is already persisted. The highest persisted superblock is #{}", super_epoch, self.highest_persisted_superblock);
        } else {
            let skipped_superblocks = super_epoch - self.highest_persisted_superblock - 1;
            if skipped_superblocks > 0 {
                // This can happen when a new node is synchronizing: it will consolidate the top of
                // chain without consolidating all the previous superblocks
                log::debug!(
                    "Skipped {} superblocks in consolidation",
                    skipped_superblocks
                );
            }

            // Replace self.previous_chain_state with None to prevent consolidating the same chain
            // state more than once
            if let Some((chain_state, prev_super_epoch)) = self.previous_chain_state.take() {
                if prev_super_epoch != super_epoch {
                    panic!("Cannot persist chain state. There is no snapshot for superblock #{}. The current snapshot is for superblock #{}", super_epoch, prev_super_epoch);
                }

                self.highest_persisted_superblock = super_epoch;

                Some(chain_state)
            } else {
                panic!("Cannot persist chain state. There is no snapshot for superblock #{}. The highest persisted superblock is #{}", super_epoch, self.highest_persisted_superblock);
            }
        }
    }

    // Remove the taken snapshot
    fn clear(&mut self) {
        self.previous_chain_state = None;
    }
}

/// Block validation process which doesn't use futures
#[allow(clippy::too_many_arguments)]
pub fn process_validations(
    block: &Block,
    current_epoch: Epoch,
    vrf_input: CheckpointVRF,
    chain_beacon: CheckpointBeacon,
    rep_eng: &ReputationEngine,
    epoch_constants: EpochConstants,
    utxo_set: &UnspentOutputsPool,
    dr_pool: &DataRequestPool,
    vrf_ctx: &mut VrfCtx,
    secp_ctx: &CryptoEngine,
    block_number: u32,
    consensus_constants: &ConsensusConstants,
) -> Result<Diff, failure::Error> {
    let mut signatures_to_verify = vec![];
    validate_block(
        block,
        current_epoch,
        vrf_input,
        chain_beacon,
        &mut signatures_to_verify,
        rep_eng,
        &consensus_constants,
    )?;
    verify_signatures(signatures_to_verify, vrf_ctx, secp_ctx)?;

    let mut signatures_to_verify = vec![];
    let utxo_dif = validate_block_transactions(
        utxo_set,
        dr_pool,
        block,
        vrf_input,
        &mut signatures_to_verify,
        rep_eng,
        epoch_constants,
        block_number,
        consensus_constants,
    )?;
    verify_signatures(signatures_to_verify, vrf_ctx, secp_ctx)?;

    Ok(utxo_dif)
}

// This struct count the number of truths, lies and errors committed by an identity
#[derive(Debug, Default, Clone, Eq, PartialEq)]
struct RequestResult {
    pub truths: u32,
    pub lies: u32,
    pub errors: u32,
}

#[derive(Debug, Default)]
struct ReputationInfo {
    // Counter of "witnessing acts".
    // For every data request with a tally in this block, increment alpha_diff
    // by the number of reveals present in the tally.
    alpha_diff: Alpha,

    // Map used to count the witnesses results in one epoch
    result_count: HashMap<PublicKeyHash, RequestResult>,
}

impl ReputationInfo {
    fn new() -> Self {
        Self::default()
    }

    fn update(
        &mut self,
        tally_transaction: &TallyTransaction,
        data_request_pool: &DataRequestPool,
        own_pkh: Option<PublicKeyHash>,
        node_stats: &mut NodeStats,
    ) {
        let dr_pointer = tally_transaction.dr_pointer;
        let dr_state = &data_request_pool.data_request_pool[&dr_pointer];
        let commits = &dr_state.info.commits;
        // 1 reveal = 1 witnessing act
        let reveals_count = u32::try_from(dr_state.info.reveals.len()).unwrap();
        self.alpha_diff += Alpha(reveals_count);

        // Set of pkhs which were slashed in the tally transaction
        let out_of_consensus = &tally_transaction.out_of_consensus;
        let error_committers = &tally_transaction.error_committers;
        for pkh in commits.keys() {
            let result = self.result_count.entry(*pkh).or_default();
            if error_committers.contains(pkh) {
                result.errors += 1;
            } else if out_of_consensus.contains(pkh) {
                result.lies += 1;
            } else {
                result.truths += 1;
            }
        }

        // Update node stats
        if own_pkh.is_some()
            && out_of_consensus.contains(&own_pkh.unwrap())
            && !error_committers.contains(&own_pkh.unwrap())
        {
            node_stats.slashed_count += 1;
        }
    }
}

// Helper methods
#[allow(clippy::too_many_arguments)]
fn update_pools(
    block: &Block,
    unspent_outputs_pool: &mut UnspentOutputsPool,
    data_request_pool: &mut DataRequestPool,
    transactions_pool: &mut TransactionsPool,
    utxo_diff: Diff,
    own_pkh: Option<PublicKeyHash>,
    own_utxos: &mut OwnUnspentOutputsPool,
    epoch_constants: EpochConstants,
    node_stats: &mut NodeStats,
) -> ReputationInfo {
    let mut rep_info = ReputationInfo::new();

    for ta_tx in &block.txns.tally_txns {
        // Process tally transactions: used to update reputation engine
        rep_info.update(&ta_tx, data_request_pool, own_pkh, node_stats);

        // IMPORTANT: Update the data request pool after updating reputation info
        if let Err(e) = data_request_pool.process_tally(&ta_tx, &block.hash()) {
            log::error!("Error processing tally transaction:\n{}", e);
        }
    }

    for vt_tx in &block.txns.value_transfer_txns {
        transactions_pool.vt_remove(&vt_tx.hash());
    }

    for dr_tx in &block.txns.data_request_txns {
        if let Err(e) = data_request_pool.process_data_request(
            &dr_tx,
            block.block_header.beacon.checkpoint,
            epoch_constants,
            &block.hash(),
        ) {
            log::error!("Error processing data request transaction:\n{}", e);
        } else {
            transactions_pool.dr_remove(&dr_tx.hash());
        }
    }

    for co_tx in &block.txns.commit_txns {
        if let Err(e) = data_request_pool.process_commit(&co_tx, &block.hash()) {
            log::error!("Error processing commit transaction:\n{}", e);
        } else {
            if Some(co_tx.body.proof.proof.pkh()) == own_pkh {
                node_stats.commits_count += 1;
            }
            transactions_pool.remove_inputs(&co_tx.body.collateral);
        }
    }

    for re_tx in &block.txns.reveal_txns {
        if let Err(e) = data_request_pool.process_reveal(&re_tx, &block.hash()) {
            log::error!("Error processing reveal transaction:\n{}", e);
        }
    }

    // Remove reveals because they expire every consolidated block
    transactions_pool.clear_reveals();

    // Update own_utxos:
    if let Some(own_pkh) = own_pkh {
        utxo_diff.visit(
            own_utxos,
            |own_utxos, output_pointer, output| {
                // Insert new outputs
                if output.pkh == own_pkh {
                    own_utxos.insert(output_pointer.clone(), 0);
                }
            },
            |own_utxos, output_pointer| {
                // Remove spent inputs
                own_utxos.remove(&output_pointer);
            },
        );
    }

    utxo_diff.apply(unspent_outputs_pool);

    rep_info
}

fn separate_honest_errors_and_liars<K, I>(rep_info: I) -> (Vec<K>, Vec<K>, Vec<(K, u32)>)
where
    I: IntoIterator<Item = (K, RequestResult)>,
{
    let mut honests = vec![];
    let mut liars = vec![];
    let mut errors = vec![];
    for (pkh, result) in rep_info {
        if result.lies > 0 {
            liars.push((pkh, result.lies));
        // TODO: Decide which percentage would be fair enough
        } else if result.truths >= result.errors {
            honests.push(pkh);
        } else {
            errors.push(pkh);
        }
    }

    (honests, errors, liars)
}

// FIXME(#676): Remove clippy skip error
#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cognitive_complexity,
    clippy::too_many_arguments
)]
fn update_reputation(
    rep_eng: &mut ReputationEngine,
    secp_bls_mapping: &mut AltKeys,
    consensus_constants: &ConsensusConstants,
    miner_pkh: PublicKeyHash,
    ReputationInfo {
        alpha_diff,
        result_count,
    }: ReputationInfo,
    log_level: log::Level,
    block_epoch: Epoch,
    own_pkh: PublicKeyHash,
) {
    let old_alpha = rep_eng.current_alpha;
    let new_alpha = Alpha(old_alpha.0 + alpha_diff.0);
    log::log!(log_level, "Reputation Engine Update:\n");
    log::log!(
        log_level,
        "Witnessing acts: Total {} + new {}",
        old_alpha.0,
        alpha_diff.0
    );
    log::log!(log_level, "Lie count: {{");
    for (pkh, result) in result_count
        .iter()
        .sorted_by(|a, b| a.0.to_string().cmp(&b.0.to_string()))
    {
        log::log!(
            log_level,
            "    {}: {} truths, {} errors, {} lies",
            pkh,
            result.truths,
            result.errors,
            result.lies
        );
    }
    log::log!(log_level, "}}");
    let (honests, _errors, liars) = separate_honest_errors_and_liars(result_count.clone());
    let revealers = result_count.into_iter().map(|(pkh, _)| pkh);
    // Leftover reputation from the previous epoch
    let extra_rep_previous_epoch = rep_eng.extra_reputation;
    // Expire in old_alpha to maximize reputation lost in penalizations.
    // Example: we are in old_alpha 10000, new_alpha 5 and some reputation expires in
    // alpha 10002. This reputation will expire in the next epoch.
    let expired_rep = rep_eng.trs_mut().expire(&old_alpha);
    // There is some reputation issued for every witnessing act
    let issued_rep = reputation_issuance(
        Reputation(consensus_constants.reputation_issuance),
        Alpha(consensus_constants.reputation_issuance_stop),
        old_alpha,
        new_alpha,
    );
    let own_rep = rep_eng.trs().get(&own_pkh);
    // Penalize liars and accumulate the reputation
    // The penalization depends on the number of lies from the last epoch
    let liars_and_penalize_function = liars.iter().map(|(pkh, num_lies)| {
        if own_pkh == *pkh {
            let after_slashed_rep = f64::from(own_rep.0)
                * consensus_constants
                    .reputation_penalization_factor
                    .powf(f64::from(*num_lies));
            let slashed_rep = own_rep.0 - (after_slashed_rep as u32);
            log::info!(
                "Your reputation score has been slashed by {} points",
                slashed_rep
            );
        }

        (
            pkh,
            penalize_factor(
                consensus_constants.reputation_penalization_factor,
                *num_lies,
            ),
        )
    });
    let penalized_rep = rep_eng
        .trs_mut()
        .penalize_many(liars_and_penalize_function)
        .unwrap();

    let mut reputation_bounty = extra_rep_previous_epoch;
    reputation_bounty += expired_rep;
    reputation_bounty += issued_rep;
    reputation_bounty += penalized_rep;

    let num_honest = u32::try_from(honests.len()).unwrap();

    log::log!(
        log_level,
        "+ {:9} rep from previous epoch",
        extra_rep_previous_epoch.0
    );
    log::log!(log_level, "+ {:9} expired rep", expired_rep.0);
    log::log!(log_level, "+ {:9} issued rep", issued_rep.0);
    log::log!(log_level, "+ {:9} penalized rep", penalized_rep.0);
    log::log!(log_level, "= {:9} reputation bounty", reputation_bounty.0);

    // Gain reputation
    if num_honest > 0 {
        let rep_reward = reputation_bounty.0 / num_honest;
        // Expiration starts counting from new_alpha.
        // All the reputation earned in this block will expire at the same time.
        let expire_alpha = Alpha(new_alpha.0 + consensus_constants.reputation_expire_alpha_diff);
        let honest_gain = honests.into_iter().map(|pkh| {
            if own_pkh == pkh {
                log::info!(
                    "Your reputation score has increased by {} points",
                    rep_reward
                );
            }
            (pkh, Reputation(rep_reward))
        });
        rep_eng.trs_mut().gain(expire_alpha, honest_gain).unwrap();

        let gained_rep = Reputation(rep_reward * num_honest);
        reputation_bounty -= gained_rep;

        log::log!(
            log_level,
            "({} rep x {} revealers = {})",
            rep_reward,
            num_honest,
            gained_rep.0
        );
        log::log!(log_level, "- {:9} gained rep", gained_rep.0);
    } else {
        log::log!(log_level, "(no revealers for this epoch)");
        log::log!(log_level, "- {:9} gained rep", 0);
    }

    let extra_reputation = reputation_bounty;
    rep_eng.extra_reputation = extra_reputation;
    log::log!(
        log_level,
        "= {:9} extra rep for next epoch",
        extra_reputation.0
    );

    // Update active reputation set
    // Add block miner pkh to active identities
    if let Err(e) = rep_eng
        .ars_mut()
        .update(revealers.chain(vec![miner_pkh]), block_epoch)
    {
        log::error!("Error updating reputation in consolidation: {}", e);
    }

    // Retain identities that exist in the ARS
    secp_bls_mapping.retain(|k| rep_eng.is_ars_member(k));

    rep_eng.current_alpha = new_alpha;
}

fn show_tally_info(tally_tx: &TallyTransaction, block_epoch: Epoch) {
    let result = RadonTypes::try_from(tally_tx.tally.as_slice());
    let result_str = RadonReport::from_result(result, &ReportContext::default())
        .into_inner()
        .to_string();
    log::info!(
        "{} {} completed at epoch #{} with result: {}",
        Yellow.bold().paint("[Data Request]"),
        Yellow.bold().paint(tally_tx.dr_pointer.to_string()),
        Yellow.bold().paint(block_epoch.to_string()),
        Yellow.bold().paint(result_str),
    );
}

fn show_info_dr(data_request_pool: &DataRequestPool, block: &Block) {
    let block_hash = block.hash();
    let block_epoch = block.block_header.beacon.checkpoint;

    let info = data_request_pool
        .data_request_pool
        .iter()
        .fold(String::new(), |acc, (k, v)| {
            format!(
                "{}\n\t* {} Stage: {}, Commits: {}, Reveals: {}",
                acc,
                White.bold().paint(k.to_string()),
                White.bold().paint(format!("{:?}", v.stage)),
                v.info.commits.len(),
                v.info.reveals.len()
            )
        });

    if info.is_empty() {
        log::info!(
            "{} Block {} consolidated for epoch #{} {}",
            Purple.bold().paint("[Chain]"),
            Purple.bold().paint(block_hash.to_string()),
            Purple.bold().paint(block_epoch.to_string()),
            White.paint("with no data requests".to_string()),
        );
    } else {
        log::info!(
            "{} Block {} consolidated for epoch #{}\n{}{}",
            Purple.bold().paint("[Chain]"),
            Purple.bold().paint(block_hash.to_string()),
            Purple.bold().paint(block_epoch.to_string()),
            White.bold().paint("Data Requests: "),
            White.bold().paint(info),
        );
    }
}

fn show_sync_progress(
    beacon: CheckpointBeacon,
    sync_target: &SyncTarget,
    epoch_constants: EpochConstants,
) {
    let target_checkpoint = sync_target.block.checkpoint;
    // Show progress log
    let mut percent_done_float =
        f64::from(beacon.checkpoint) / f64::from(target_checkpoint) * 100.0;

    // Never show 100% unless it's actually done
    if beacon.checkpoint != target_checkpoint && percent_done_float > 99.99 {
        percent_done_float = 99.99;
    }
    let percent_done_string = format!("{:.2}%", percent_done_float);

    // Block age is actually the difference in age: it assumes that the last
    // block is 0 seconds old
    let block_age =
        (target_checkpoint - beacon.checkpoint) * u32::from(epoch_constants.checkpoints_period);

    let human_age = seconds_to_human_string(u64::from(block_age));
    log::info!(
        "Synchronization progress: {} ({:>6}/{:>6}). Latest synced block is {} old.",
        percent_done_string,
        beacon.checkpoint,
        target_checkpoint,
        human_age
    );
}

// TODO: handle recovery cases after reduction
// Returns the committee size to be applied given the default committee size, decreasing period
// and  step, last consolidated epoch and the current checkpoint
fn current_committee_size_requirement(
    default_committee_size: u32,
    last_committee_size: u32,
    decreasing_period: u32,
    decreasing_step: u32,
    last_consolidated_checkpoint: u32,
    current_checkpoint: u32,
) -> u32 {
    // If the last consolidated superblock is 0, return the default committee size
    if last_consolidated_checkpoint == 0 {
        default_committee_size
    } else if current_checkpoint - last_consolidated_checkpoint >= decreasing_period {
        // Calculate the difference between the last consolidated superblock checkpoint and the current one
        // If this difference exceeds the decreasing_period, reduce the committee size by decreasing_step * difference
        // The minimum committee size is 1
        max(
            default_committee_size.saturating_sub(
                (current_checkpoint.saturating_sub(last_consolidated_checkpoint)
                    / decreasing_period)
                    * decreasing_step,
            ),
            1,
        )
    } else {
        // In this case, if the last_committee_size was equal to default, return default
        // Else, increase the committee size step by step
        min(
            last_committee_size.saturating_add(decreasing_step),
            default_committee_size,
        )
    }
}

/// Get the identities of all ARS members with non-neutral reputation
pub fn reputed_ars(
    v: &[PublicKeyHash],
    reputation_engine: &ReputationEngine,
) -> Vec<PublicKeyHash> {
    v.iter()
        .filter_map(|pkh| {
            if reputation_engine.trs().get(pkh).0 > 0 {
                Some(*pkh)
            } else {
                None
            }
        })
        .collect()
}

/// When the TransactionsPool is full, inserting a transaction can result in removing other
/// transactions. This will log the removed transactions.
pub fn log_removed_transactions(removed_transactions: &[Transaction], inserted_tx_hash: Hash) {
    if removed_transactions.is_empty() {
        log::trace!("Transaction {} added successfully", inserted_tx_hash);
    } else {
        let mut removed_tx_hashes: Vec<String> = vec![];
        // The transaction we tried to insert may be among the removed transactions
        // In that case, do not log "Transaction {} added successfully"
        let mut removed_the_one_we_just_inserted = false;
        for tx in removed_transactions {
            let removed_tx_hash = tx.hash();
            removed_tx_hashes.push(removed_tx_hash.to_string());

            if removed_tx_hash == inserted_tx_hash {
                removed_the_one_we_just_inserted = true;
            }
        }

        if removed_the_one_we_just_inserted {
            log::trace!(
                "Transaction {} was not added because the TransactionsPool is full",
                inserted_tx_hash
            );
        } else {
            log::trace!("Transaction {} added successfully", inserted_tx_hash);
        }

        log::debug!(
            "TransactionsPool is full! Removed the following transactions: {:?}",
            removed_tx_hashes
        );
    }
}

#[cfg(test)]
mod tests {
    use witnet_data_structures::{
        chain::{
            ChainInfo, Environment, KeyedSignature, PartialConsensusConstants, PublicKey,
            ValueTransferOutput,
        },
        transaction::{CommitTransaction, DRTransaction, RevealTransaction},
    };

    pub use super::*;
    use witnet_config::{config::consensus_constants_from_partial, defaults::Testnet};

    #[test]
    fn test_rep_info_update() {
        let mut rep_info = ReputationInfo::default();
        let mut dr_pool = DataRequestPool::default();

        let pk1 = PublicKey {
            compressed: 0,
            bytes: [1; 32],
        };
        let pk2 = PublicKey {
            compressed: 0,
            bytes: [2; 32],
        };
        let pk3 = PublicKey {
            compressed: 0,
            bytes: [3; 32],
        };

        let mut dr_tx = DRTransaction::default();
        dr_tx.signatures.push(KeyedSignature {
            public_key: pk1.clone(),
            ..KeyedSignature::default()
        });
        let dr_pointer = dr_tx.hash();

        let mut co_tx = CommitTransaction::default();
        co_tx.body.dr_pointer = dr_pointer;
        co_tx.signatures.push(KeyedSignature {
            public_key: pk1.clone(),
            ..KeyedSignature::default()
        });
        let mut co_tx2 = CommitTransaction::default();
        co_tx2.body.dr_pointer = dr_pointer;
        co_tx2.signatures.push(KeyedSignature {
            public_key: pk2.clone(),
            ..KeyedSignature::default()
        });
        let mut co_tx3 = CommitTransaction::default();
        co_tx3.body.dr_pointer = dr_pointer;
        co_tx3.signatures.push(KeyedSignature {
            public_key: pk3.clone(),
            ..KeyedSignature::default()
        });
        let mut re_tx1 = RevealTransaction::default();
        re_tx1.body.dr_pointer = dr_pointer;
        re_tx1.signatures.push(KeyedSignature {
            public_key: pk1.clone(),
            ..KeyedSignature::default()
        });
        let mut re_tx2 = RevealTransaction::default();
        re_tx2.body.dr_pointer = dr_pointer;
        re_tx2.signatures.push(KeyedSignature {
            public_key: pk2.clone(),
            ..KeyedSignature::default()
        });

        let mut ta_tx = TallyTransaction::default();
        ta_tx.dr_pointer = dr_pointer;
        ta_tx.outputs = vec![ValueTransferOutput {
            pkh: pk1.pkh(),
            ..Default::default()
        }];
        ta_tx.out_of_consensus = vec![pk3.pkh()];
        ta_tx.error_committers = vec![pk2.pkh()];

        dr_pool
            .add_data_request(1, dr_tx, &Hash::default())
            .unwrap();
        dr_pool.process_commit(&co_tx, &Hash::default()).unwrap();
        dr_pool.process_commit(&co_tx2, &Hash::default()).unwrap();
        dr_pool.process_commit(&co_tx3, &Hash::default()).unwrap();
        dr_pool.update_data_request_stages();
        dr_pool.process_reveal(&re_tx1, &Hash::default()).unwrap();
        dr_pool.process_reveal(&re_tx2, &Hash::default()).unwrap();

        rep_info.update(&ta_tx, &dr_pool, None, &mut NodeStats::default());

        assert_eq!(
            rep_info.result_count[&pk1.pkh()],
            RequestResult {
                truths: 1,
                lies: 0,
                errors: 0,
            }
        );
        assert_eq!(
            rep_info.result_count[&pk2.pkh()],
            RequestResult {
                truths: 0,
                lies: 0,
                errors: 1,
            }
        );
        assert_eq!(
            rep_info.result_count[&pk3.pkh()],
            RequestResult {
                truths: 0,
                lies: 1,
                errors: 0,
            }
        );
    }

    #[test]
    fn get_superblock_beacon() {
        let mut chain_manager = ChainManager::default();
        chain_manager.chain_state.chain_info = Some(ChainInfo {
            environment: Environment::default(),
            consensus_constants: consensus_constants_from_partial(
                &PartialConsensusConstants::default(),
                &Testnet,
            ),
            highest_block_checkpoint: CheckpointBeacon::default(),
            highest_superblock_checkpoint: CheckpointBeacon {
                checkpoint: 0,
                hash_prev_block: Hash::SHA256([1; 32]),
            },
            highest_vrf_output: CheckpointVRF::default(),
        });

        assert_eq!(
            chain_manager.get_superblock_beacon(),
            CheckpointBeacon {
                checkpoint: 0,
                hash_prev_block: Hash::SHA256([1; 32]),
            }
        );
    }

    #[test]
    fn test_current_committee_size_requirement() {
        let mut size = current_committee_size_requirement(5, 5, 4, 1, 0, 1);

        assert_eq!(size, 5);

        size = current_committee_size_requirement(5, 5, 4, 1, 0, 300);

        assert_eq!(size, 5);

        size = current_committee_size_requirement(5, 5, 4, 1, 3, 4);

        assert_eq!(size, 5);

        size = current_committee_size_requirement(5, 5, 4, 1, 3, 7);

        assert_eq!(size, 4);

        size = current_committee_size_requirement(5, 5, 4, 1, 3, 12);

        assert_eq!(size, 3);

        size = current_committee_size_requirement(5, 5, 4, 1, 3, 200);

        assert_eq!(size, 1);

        size = current_committee_size_requirement(100, 100, 5, 5, 5, 50);

        assert_eq!(size, 55);

        size = current_committee_size_requirement(100, 55, 5, 5, 5, 6);

        assert_eq!(size, 60);

        size = current_committee_size_requirement(100, 98, 5, 5, 5, 6);

        assert_eq!(size, 100);

        size = current_committee_size_requirement(100, 100, 5, 5, 5, 6);

        assert_eq!(size, 100);
    }

    #[test]
    fn test_reputed_ars() {
        // Set a reputation engine with 6 members of the ARS
        let mut rep_engine = ReputationEngine::new(1000);
        let mut ids = vec![];
        for i in 0..6 {
            ids.push(PublicKeyHash::from_bytes(&[i; 20]).unwrap());
        }
        rep_engine.ars_mut().push_activity(ids.clone());

        rep_engine
            .trs_mut()
            .gain(Alpha(10), vec![(ids[0], Reputation(79))])
            .unwrap();
        rep_engine
            .trs_mut()
            .gain(Alpha(10), vec![(ids[1], Reputation(9))])
            .unwrap();

        // The reputed ars should be the vector of the first two members
        // (with reputation greater than 0)
        let rep_ars = reputed_ars(&rep_engine.get_rep_ordered_ars_list(), &rep_engine);
        assert_eq!(rep_ars.len(), 2);
        assert_eq!(rep_ars, [ids[0], ids[1]]);
    }

    #[test]
    fn test_reputed_ars_2() {
        let mut rep_engine = ReputationEngine::new(1000);
        let mut ids = vec![];
        for i in 0..6 {
            ids.push(PublicKeyHash::from_bytes(&[i; 20]).unwrap());
        }
        rep_engine.ars_mut().push_activity(ids.clone());

        rep_engine
            .trs_mut()
            .gain(Alpha(10), vec![(ids[0], Reputation(7))])
            .unwrap();
        rep_engine
            .trs_mut()
            .gain(Alpha(10), vec![(ids[1], Reputation(6))])
            .unwrap();
        rep_engine
            .trs_mut()
            .gain(Alpha(10), vec![(ids[2], Reputation(5))])
            .unwrap();
        rep_engine
            .trs_mut()
            .gain(Alpha(10), vec![(ids[3], Reputation(4))])
            .unwrap();
        rep_engine
            .trs_mut()
            .gain(Alpha(10), vec![(ids[4], Reputation(3))])
            .unwrap();
        rep_engine
            .trs_mut()
            .gain(Alpha(10), vec![(ids[5], Reputation(2))])
            .unwrap();

        // The size of the reputed_ars list should equal that of the ARS as all
        // the nodes in the example ARS have reputation greater than 0
        let rep_ars = reputed_ars(&rep_engine.get_rep_ordered_ars_list(), &rep_engine);
        assert_eq!(rep_ars.len(), 6);
        assert_eq!(rep_ars, [ids[0], ids[1], ids[2], ids[3], ids[4], ids[5]]);
    }
}
