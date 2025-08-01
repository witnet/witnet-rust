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
use std::path::PathBuf;
use std::{
    cmp::{Ordering, max, min},
    collections::{HashMap, HashSet, VecDeque},
    convert::TryFrom,
    future,
    net::SocketAddr,
    pin::Pin,
    time::Duration,
};

use actix::{
    ActorFutureExt, ActorTryFutureExt, AsyncContext, Context, ContextFutureSpawner, Supervised,
    SystemService, WrapFuture, prelude::*,
};
use ansi_term::Color::{Purple, White, Yellow};
use derive_more::Display;
use futures::future::{FutureExt, try_join_all};
use glob::glob;
use itertools::Itertools;
use rand::Rng;
use thiserror::Error;

use crate::{
    actors::{
        chain_manager::handlers::SYNCED_BANNER,
        inventory_manager::InventoryManager,
        json_rpc::JsonRpcServer,
        messages::{
            AddItem, AddItems, AddTransaction, Anycast, BlockNotify, Broadcast, DropOutboundPeers,
            GetBlocksEpochRange, GetItemBlock, NodeStatusNotify, RemoveAddressesFromTried,
            SendInventoryItem, SendInventoryRequest, SendLastBeacon, SendSuperBlockVote,
            SetLastBeacon, SetSuperBlockTargetBeacon, StoreInventoryItem, SuperBlockNotify,
        },
        node::{NodeOps, PutNodeOps},
        peers_manager::PeersManager,
        sessions_manager::SessionsManager,
        storage_keys,
    },
    signature_mngr, storage_mngr,
    utils::{Force, deserialize_from_file, file_name_compose, stop_system_if_panicking},
};
use witnet_config::{
    config::Tapi, defaults::PSEUDO_CONSENSUS_CONSTANTS_WIP0022_REWARD_COLLATERAL_RATIO,
};
use witnet_crypto::hash::calculate_sha256;
use witnet_data_structures::{
    chain::{
        Alpha, AltKeys, Block, BlockHeader, Bn256PublicKey, ChainImport, ChainInfo, ChainState,
        CheckpointBeacon, CheckpointVRF, ConsensusConstants, ConsensusConstantsWit2,
        DataRequestInfo, DataRequestOutput, DataRequestStage, Epoch, EpochConstants, Hash,
        Hashable, InventoryEntry, InventoryItem, NodeStats, PublicKeyHash, Reputation,
        ReputationEngine, SignaturesToVerify, StateMachine, SuperBlock, SuperBlockVote,
        TransactionsPool, penalize_factor,
        priority::{Priorities, PriorityEngine, PriorityVisitor},
        reputation_issuance,
        tapi::{ActiveWips, after_second_hard_fork, current_active_wips, in_emergency_period},
    },
    data_request::DataRequestPool,
    get_environment, get_protocol_version, get_protocol_version_activation_epoch,
    proto::versioning::{ProtocolVersion, VersionedHashable},
    radon_error::RadonError,
    radon_report::{RadonReport, ReportContext},
    register_protocol_version,
    staking::prelude::*,
    superblock::{ARSIdentities, AddSuperBlockVote, SuperBlockConsensus},
    transaction::{RevealTransaction, TallyTransaction, Transaction},
    types::{
        LastBeacon,
        visitor::{StatefulVisitor, Visitor},
    },
    utxo_pool::{Diff, OwnUnspentOutputsPool, UnspentOutputsPool, UtxoDiff, UtxoWriteBatch},
    vrf::VrfCtx,
    wit::Wit,
};
use witnet_rad::{error::RadError::TooManyWitnesses, types::RadonTypes};
use witnet_util::timestamp::seconds_to_human_string;
use witnet_validations::{
    eligibility::legacy::VrfSlots,
    validations::{
        compare_block_candidates, dr_transaction_fee, st_transaction_fee, validate_block,
        validate_block_transactions, validate_new_transaction, validate_rad_request,
        verify_signatures, vt_transaction_fee,
    },
};

mod actor;
mod handlers;
/// Block and data request mining
pub mod mining;

/// Maximum blocks number to be sent during synchronization process
pub const MAX_BLOCKS_SYNC: usize = 500;

/// Possible errors when interacting with ChainManager
#[derive(Debug, PartialEq, Eq, Error)]
pub enum ChainManagerError {
    /// A block being processed was already known to this node
    #[error("A block being processed was already known to this node")]
    BlockAlreadyExists,
    /// A block does not exist
    #[error("A block does not exist")]
    BlockDoesNotExist,
    /// Optional fields of ChainManager are not properly initialized yet
    #[error("ChainManager is not ready yet. This may self-fix in a little while")]
    ChainNotReady,
    /// The node attempted to do an action that is only allowed while `ChainManager`
    /// is in `Synced` state.
    #[error("The node is not yet in `Synced` state (current state is {current_state:?})")]
    NotSynced {
        /// Tells what the current state is, so users can better get an idea of why an action is
        /// not possible at this time.
        current_state: StateMachine,
    },
    /// The node is trying to mine a block so commits are not allowed
    #[error("Commit received while node is trying to mine a block")]
    TooLateToCommit,
    /// The node received a batch of blocks that is inconsistent with the current index
    #[error(
        "Wrong number of blocks provided {wrong_index:?} for superblock index {consolidated_superblock_index:?} and epoch {current_superblock_index:?})"
    )]
    WrongBlocksForSuperblock {
        /// Tells what the wrong index was
        wrong_index: u32,
        /// Tells what the current superblock index was
        consolidated_superblock_index: u32,
        /// Tells what the current epoch was
        current_superblock_index: u32,
    },
    /// Tried to mine block candidates but mining is disabled through configuration.
    #[error("Mining is disabled through configuration")]
    MiningIsDisabled,
    /// A staking-related error happened.
    #[error("A staking-related error happened: {0:?}")]
    Staking(StakesError<PublicKeyHash, Wit, Epoch>),
    /// The node is not eligible to perform a certain action.
    #[error("The node is not eligible to perform this action")]
    NotEligible,
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
#[derive(Default)]
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
    /// The superblock hash and superblock according to a majority of peers
    sync_superblock: Option<(Hash, SuperBlock)>,
    /// The node asked for a batch of blocks on this epoch. This is used to implement a timeout
    /// that will move the node back to WaitingConsensus state if it does not receive any AddBlocks
    /// message after a certain number of epochs
    sync_waiting_for_add_blocks_since: Option<Epoch>,
    /// Map that stores candidate blocks for further validation and consolidation as tip of the blockchain
    /// (block_hash, block))
    candidates: HashMap<Hash, Vec<Block>>,
    /// Best candidate
    best_candidate: Option<BlockCandidate>,
    /// Set that stores all the recently received candidates
    seen_candidates: HashSet<Block>,
    /// Set that stores all the recently received transactions
    seen_transactions: HashSet<Transaction>,
    /// Our public key hash, used to create the mint transaction
    own_pkh: Option<PublicKeyHash>,
    /// Our BLS public key, used to append in commit transactions
    bn256_public_key: Option<Bn256PublicKey>,
    /// VRF context
    vrf_ctx: Option<VrfCtx>,
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
    temp_superblock_votes: HashSet<SuperBlockVote>,
    /// Commits and reveals to process later
    temp_reveals: Vec<RevealTransaction>,
    /// Value transfers and data requests to process later
    temp_vts_and_drs: VecDeque<Transaction>,
    /// Maximum number of recovered transactions to include by epoch
    max_reinserted_transactions: usize,
    /// Last received Beacons
    last_received_beacons: Vec<(SocketAddr, Option<LastBeacon>)>,
    /// Last SuperBlock consensus
    last_superblock_consensus: Option<CheckpointBeacon>,
    /// Settings for Threshold Activation of Protocol Improvements
    tapi: Tapi,
    /// Transaction priority engine
    priority_engine: PriorityEngine,
    /// Chain snapshot to be imported
    import: Force<ChainImport<ImportError>>,
    /// Signals that a chain snapshot export is due.
    export: Force<PathBuf>,
    /// Consensus constants for wit/2
    consensus_constants_wit2: ConsensusConstantsWit2,
    /// Initial WIT supply
    initial_supply: u64,
}

impl ChainManager {
    /// Drop the value of the `import` field.
    fn drop_import(&mut self) {
        self.import = Force::None;
    }

    /// Order a chain snapshot export upon starting.
    fn put_export(&mut self, export: Force<PathBuf>) {
        self.export = export;
    }

    /// Put a chain export into the `import` field.
    fn put_import(&mut self, import: Force<ChainImport<ImportError>>) {
        self.import = import;
    }

    /// Try to read and load a chain snapshot from the filesystem into a ChainManager.
    ///
    /// This method is intentionally best-effort.
    pub fn put_import_from_path(&mut self, path: Force<PathBuf>) {
        // Deconstruct path and force degree, and abort if there's no path
        let (path, force) = match path {
            Force::All(path) => (path, true),
            Force::Some(path) => (path, false),
            Force::None => {
                return;
            }
        };

        // A future for reading and deserializing the chain state from a single file
        let chain_state_path = path.clone();
        let chain_state = Box::pin(async move {
            let path_display = chain_state_path.display().to_string();
            log::debug!("Trying to read chain state from file {path_display}");

            // Open file, create reader, and decode using bincode
            let chain_state =
                deserialize_from_file(&chain_state_path).map_err(|e: ImportError| match e {
                    ImportError::Bincode(_) => ImportError::Deserialize {
                        path: path_display.clone(),
                    },
                    ImportError::Io(_) => ImportError::FileRead {
                        path: path_display.clone(),
                    },
                    e => e,
                })?;

            Ok(chain_state)
        });

        // A future for reading and deserializing superblocks from a single file
        let superblocks_path = file_name_compose(path.clone(), Some("superblocks".into()));
        let superblocks = Box::pin(async move {
            // Derive superblocks file path from the base path
            let path_display = superblocks_path.display().to_string();
            log::debug!("Trying to read superblocks file at {path_display}");

            // Open file, create reader, and decode using bincode
            let superblocks: Vec<_> =
                deserialize_from_file(&superblocks_path).map_err(|e: ImportError| match e {
                    ImportError::Bincode(_) => ImportError::Deserialize {
                        path: path_display.clone(),
                    },
                    ImportError::Io(_) => ImportError::FileRead {
                        path: path_display.clone(),
                    },
                    e => e,
                })?;

            log::info!(
                "Read {} superblocks from file {}",
                superblocks.len(),
                path_display
            );

            Ok(superblocks)
        });

        // A vector of futures for reading and deserializing blocks from multiple files
        let blocks_path = file_name_compose(path, Some("blocks_batch_*".into()));
        let path_display = blocks_path.display().to_string();
        let blocks = match glob(&path_display) {
            Ok(entries) => {
                let mut blocks = Vec::new();
                for batch_path in entries.flatten() {
                    let fut = futurize_batch_read(batch_path);
                    blocks.push(fut);
                }
                Ok(blocks)
            }
            Err(_) => Err(ImportError::FileRead { path: path_display }),
        };

        let import = Force::new(
            ChainImport {
                blocks,
                chain_state,
                superblocks,
            },
            force,
        );

        self.put_import(import);
    }
}

fn futurize_batch_read(
    batch_path: PathBuf,
) -> Pin<
    Box<
        (
            dyn futures_util::Future<
                    Output = Result<Vec<witnet_data_structures::chain::Block>, ImportError>,
                > + std::marker::Send
                + 'static
        ),
    >,
> {
    Box::pin(async move {
        let path_display = batch_path.display().to_string();
        let batch: Vec<_> =
            deserialize_from_file(&batch_path).map_err(|e: ImportError| match e {
                ImportError::Bincode(_) => ImportError::Deserialize {
                    path: path_display.clone(),
                },
                ImportError::Io(_) => ImportError::FileRead {
                    path: path_display.clone(),
                },
                e => e,
            })?;

        log::info!("Read {} blocks from file {}", batch.len(), path_display);

        Ok(batch)
    })
}

impl Drop for ChainManager {
    fn drop(&mut self) {
        log::trace!("Dropping ChainManager");
        stop_system_if_panicking("ChainManager");
    }
}

impl PutNodeOps for ChainManager {
    fn put_node_ops(&mut self, ops: NodeOps) {
        self.put_import_from_path(ops.snapshot_import());
        self.put_export(ops.snapshot_export());
    }
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
    /// Prority info
    pub priorities: Priorities,
}

/// Required trait for being able to retrieve ChainManager address from registry
impl Supervised for ChainManager {}

/// Required trait for being able to retrieve ChainManager address from registry
impl SystemService for ChainManager {}

/// Auxiliary methods for ChainManager actor
impl ChainManager {
    /// Persist previous chain state into storage
    /// None case: persist current chain state into storage (during synchronization)
    fn persist_chain_state(
        &mut self,
        superblock_index: Option<u32>,
    ) -> ResponseActFuture<Self, Result<(), ()>> {
        let previous_chain_state = if let Some(superblock_index) = superblock_index {
            let chain_state_snapshot = self.chain_state_snapshot.restore(superblock_index);

            if chain_state_snapshot.is_none() {
                return Box::pin(actix::fut::ok(()));
            }

            chain_state_snapshot.unwrap()
        } else {
            // None case is used to persist chain_state during synchronization
            self.chain_state.clone()
        };

        // When updating the chain state, we need to update the highest superblock checkpoint.
        // This is the highest superblock that obtained a majority of votes and we do not want to
        // lose it when restoring the state.
        let mut state = ChainState {
            chain_info: Some(ChainInfo {
                highest_superblock_checkpoint: self.get_superblock_beacon(),
                ..previous_chain_state.chain_info.as_ref().unwrap().clone()
            }),
            superblock_state: self.chain_state.superblock_state.clone(),
            ..previous_chain_state
        };

        let chain_beacon = state.get_chain_beacon();
        let superblock_beacon = state.get_superblock_beacon();

        if let Some(superblock_index) = superblock_index {
            log::debug!(
                "Persisting chain state for superblock #{superblock_index} with chain beacon {chain_beacon:?} and super beacon {superblock_beacon:?}"
            );

            assert_eq!(superblock_beacon.checkpoint, superblock_index);
        } else {
            log::debug!(
                "Persisting chain state during synchronization, chain beacon: {chain_beacon:?}"
            );
        }

        // Update UTXO set:
        // * Remove from memory the UTXOs that will be persisted
        // * Persist the consolidated UTXOs to the database
        self.chain_state
            .unspent_outputs_pool
            .remove_persisted_from_memory(&state.unspent_outputs_pool.diff);
        let mut batch = UtxoWriteBatch::default();
        state.unspent_outputs_pool.persist_add_to_batch(&mut batch);

        let fut = storage_mngr::put_chain_state_in_batch(
            &storage_keys::chain_state_key(self.get_magic()),
            &state,
            batch,
        )
        .into_actor(self)
        .and_then(|_, _, _| {
            log::debug!("Successfully persisted previous_chain_state into storage");
            fut::ok(())
        })
        .map_err(|err, _, _| {
            log::error!("Failed to persist previous_chain_state into storage: {err}")
        });

        Box::pin(fut)
    }

    /// Persist an empty `ChainState` to the storage and set the node to `WaitingConsensus`.
    /// This can be used to recover from a forked chain without manually deleting the storage.
    fn delete_chain_state_and_reinitialize(&mut self) -> ResponseActFuture<Self, Result<(), ()>> {
        // Delete all the UTXOs from the database
        let mut batch = UtxoWriteBatch::default();
        self.chain_state
            .unspent_outputs_pool
            .delete_all_from_db_batch(&mut batch);

        let empty_state = ChainState::default();
        let fut = storage_mngr::put_chain_state_in_batch(
            &storage_keys::chain_state_key(self.get_magic()),
            &empty_state,
            batch,
        )
        .into_actor(self)
        .map_err(|err, _, _| {
            log::error!("Failed to persist empty chain state into storage: {err}");
        })
        .and_then(|(), act, ctx| {
            log::info!("Successfully persisted empty chain state into storage");
            act.update_state_machine(StateMachine::WaitingConsensus, ctx);

            act.initialize_from_storage_fut(true)
        });

        Box::pin(fut)
    }

    /// Resynchronize block chain using a list of blocks that are already in the storage.
    ///
    /// The blocks are assumed to be valid, so validations are skipped, and block metadata is not
    /// persisted to the storage because it is assumed to already be there.
    fn resync_from_storage<F>(
        &mut self,
        mut block_list: VecDeque<(Epoch, Hash)>,
        ctx: &mut Context<Self>,
        done: F,
    ) where
        F: FnOnce(&mut Self, &mut Context<Self>) + 'static,
    {
        if block_list.is_empty() {
            // Done, all the blocks have been processed
            done(self, ctx);
            // Early return
            return;
        }

        let last_epoch = block_list.back().unwrap().0;
        let (epoch, hash) = block_list.pop_front().unwrap();
        let inventory_manager_addr = InventoryManager::from_registry();
        inventory_manager_addr
            .send(GetItemBlock { hash })
            .into_actor(self)
            .map(move |res, act, ctx| {
                match res {
                    Ok(Ok(block)) => {
                        log::info!("REWIND [{epoch}/{last_epoch}] Got block {hash} from storage");
                        act.process_requested_block(ctx, block, true)
                            .expect("resync from storage fail");
                        // We need to persist the chain state periodically, otherwise the entire
                        // UTXO set will be in memory, consuming a huge amount of memory.
                        if block_list.len() % 1000 == 0 {
                            act.persist_chain_state(None)
                                .map(|_res: Result<(), ()>, _act, _ctx| ())
                                .wait(ctx);
                        }
                        // Recursion
                        act.resync_from_storage(block_list, ctx, done);
                    }
                    Ok(Err(e)) => {
                        panic!("{e:?}");
                    }
                    Err(e) => {
                        panic!("{e:?}");
                    }
                }
            })
            .spawn(ctx);
    }

    /// Replace `previous_chain_state` with current `chain_state`
    fn move_chain_state_forward(&mut self, superblock_index: u32) {
        self.chain_state_snapshot
            .take(superblock_index, &self.chain_state);
    }

    /// Method to Send items to Inventory Manager
    fn persist_items(
        &self,
        items: Vec<StoreInventoryItem>,
    ) -> ResponseActFuture<Self, Result<(), anyhow::Error>> {
        // Get InventoryManager address
        let inventory_manager_addr = InventoryManager::from_registry();

        // Persist block into storage through InventoryManager.
        Box::pin(
            inventory_manager_addr
                .send(AddItems { items })
                .into_actor(self)
                .map_ok(|_, _, _| {
                    // Upon success, ignore any response and simply let the future resolve
                })
                .map_err(|err, _, _| {
                    // Error when sending message
                    log::error!("Unsuccessful communication with InventoryManager: {err}");

                    err.into()
                }),
        )
    }

    /// Method to persist a Data Request into the Storage
    fn persist_data_requests(&self, ctx: &mut Context<Self>, dr_infos: Vec<DataRequestInfo>) {
        let kvs: Vec<_> = dr_infos
            .into_iter()
            .map(|dr_info| {
                let dr_pointer = &dr_info.tally.as_ref().unwrap().dr_pointer;
                let dr_pointer_string = format!("DR-REPORT-{dr_pointer}");

                (dr_pointer_string, dr_info)
            })
            .collect();
        let kvs_len = kvs.len();
        storage_mngr::put_batch(&kvs)
            .into_actor(self)
            .map_err(|e, _, _| {
                log::error!("Failed to persist data request report into storage: {e}")
            })
            .and_then(move |_, _, _| {
                log::trace!(
                    "Successfully persisted reports for {kvs_len} data requests into storage"
                );
                fut::ok(())
            })
            .map(|_res: Result<(), ()>, _act, _ctx| ())
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
        resynchronizing: bool,
    ) -> Result<(), anyhow::Error> {
        if let (Some(epoch_constants), Some(chain_info), Some(rep_engine), Some(vrf_ctx)) = (
            self.epoch_constants,
            self.chain_state.chain_info.as_ref(),
            self.chain_state.reputation_engine.as_ref(),
            self.vrf_ctx.as_mut(),
        ) {
            if self.current_epoch.is_none() {
                log::trace!("Called process_requested_block when current_epoch is None");
            }
            if self.chain_state.unspent_outputs_pool.db.is_none() {
                panic!("NO UTXO DB");
            }
            let block_number = self.chain_state.block_number();
            let mut vrf_input = chain_info.highest_vrf_output;
            vrf_input.checkpoint = block.block_header.beacon.checkpoint;
            let active_wips = ActiveWips {
                active_wips: self.chain_state.tapi_engine.wip_activation.clone(),
                block_epoch: block.block_header.beacon.checkpoint,
            };

            let mut transaction_visitor = PriorityVisitor::default();

            let protocol_version = get_protocol_version(Some(block.block_header.beacon.checkpoint));
            let utxo_diff = process_validations(
                &block,
                self.current_epoch.unwrap_or_default(),
                vrf_input,
                chain_info.highest_block_checkpoint,
                rep_engine,
                epoch_constants,
                &self.chain_state.unspent_outputs_pool,
                &mut self.chain_state.data_request_pool,
                vrf_ctx,
                block_number,
                &chain_info.consensus_constants,
                &self.consensus_constants_wit2,
                resynchronizing,
                &active_wips,
                Some(&mut transaction_visitor),
                &self.chain_state.stakes,
                protocol_version,
            )?;

            // Extract the collected priorities from the internal state of the visitor
            let priorities = transaction_visitor.take_state();

            // Persist block and update ChainState
            self.consolidate_block(ctx, block, utxo_diff, priorities, resynchronizing);

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
            // To continue processing, received block epoch should equal to `current_epoch` or `current_epoch + 1`
            if !(block.block_header.beacon.checkpoint == current_epoch
                || block.block_header.beacon.checkpoint == current_epoch + 1)
            {
                log::debug!(
                    "Ignoring received block candidate because its beacon shows an old epoch ({}). The current epoch is {}.",
                    block.block_header.beacon.checkpoint,
                    current_epoch,
                );

                return;
            }

            let hash_block = block.hash();
            // If this candidate has not been seen before, validate it
            if !self.seen_candidates.contains(&block) {
                self.seen_candidates.insert(block.clone());
                if self.sm_state == StateMachine::WaitingConsensus
                    || self.sm_state == StateMachine::Synchronizing
                {
                    self.candidates
                        .entry(hash_block)
                        .or_default()
                        .push(block.clone());
                    // If the node is not synced, broadcast recent candidates without validating them
                    self.broadcast_item(InventoryItem::Block(block));

                    return;
                }

                let mut vrf_input = chain_info.highest_vrf_output;
                vrf_input.checkpoint = current_epoch;
                let active_wips = ActiveWips {
                    active_wips: self.chain_state.tapi_engine.wip_activation.clone(),
                    block_epoch: block.block_header.beacon.checkpoint,
                };
                let target_vrf_slots = VrfSlots::from_rf(
                    u32::try_from(rep_engine.ars().active_identities_number()).unwrap(),
                    chain_info.consensus_constants.mining_replication_factor,
                    chain_info.consensus_constants.mining_backup_factor,
                    block.block_header.beacon.checkpoint,
                    chain_info.consensus_constants.minimum_difficulty,
                    chain_info
                        .consensus_constants
                        .epochs_with_minimum_difficulty,
                    &active_wips,
                );
                let block_pkh = &block.block_sig.public_key.pkh();
                let reputation = rep_engine.trs().get(block_pkh);
                let is_active = rep_engine.ars().contains(block_pkh);
                let vrf_proof = match block.block_header.proof.proof.proof_to_hash(vrf_ctx) {
                    Ok(vrf) => vrf,
                    Err(e) => {
                        log::warn!("Block candidate has an invalid mining eligibility proof: {e}");

                        // In order to do not block possible validate candidates in AlmostSynced
                        // state, we would broadcast the errors too
                        if self.sm_state == StateMachine::AlmostSynced {
                            self.broadcast_item(InventoryItem::Block(block));
                        }

                        return;
                    }
                };
                let protocol_version =
                    ProtocolVersion::from_epoch(block.block_header.beacon.checkpoint);

                if let Some(best_candidate) = &self.best_candidate {
                    let best_hash = best_candidate.block.hash();
                    let best_pkh = best_candidate.block.block_sig.public_key.pkh();
                    let best_candidate_is_active =
                        if after_second_hard_fork(current_epoch, get_environment()) {
                            rep_engine.ars().contains(&best_pkh)
                        } else {
                            // In case of being before to second hard fork we would use the same bool
                            // than the other to avoid the "activeness" comparison
                            is_active
                        };
                    let power = self
                        .chain_state
                        .stakes
                        .query_power(
                            *block_pkh,
                            Capability::Mining,
                            block.block_header.beacon.checkpoint,
                        )
                        .unwrap_or(0);
                    let best_candidate_power = self
                        .chain_state
                        .stakes
                        .query_power(
                            best_pkh,
                            Capability::Mining,
                            best_candidate.block.block_header.beacon.checkpoint,
                        )
                        .unwrap_or(0);

                    if compare_block_candidates(
                        hash_block,
                        reputation,
                        vrf_proof,
                        is_active,
                        power,
                        best_hash,
                        best_candidate.reputation,
                        best_candidate.vrf_proof,
                        best_candidate_is_active,
                        best_candidate_power,
                        &target_vrf_slots,
                        protocol_version,
                    ) != Ordering::Greater
                    {
                        log::debug!(
                            "Ignoring new block candidate ({hash_block}) because a better one ({best_hash}) has been already validated"
                        );

                        return;
                    }
                }

                // This visitor will be used to derive a `Priorities` value from the transactions
                // in this block candidate.
                let mut transaction_visitor = PriorityVisitor::default();

                let block_number = self.chain_state.block_number();
                match process_validations(
                    &block,
                    current_epoch,
                    vrf_input,
                    chain_info.highest_block_checkpoint,
                    rep_engine,
                    self.epoch_constants.unwrap(),
                    &self.chain_state.unspent_outputs_pool,
                    &mut self.chain_state.data_request_pool,
                    // The unwrap is safe because if there is no VRF context,
                    // the actor should have stopped execution
                    self.vrf_ctx.as_mut().expect("No initialized VRF context"),
                    block_number,
                    &chain_info.consensus_constants,
                    &self.consensus_constants_wit2,
                    false,
                    &active_wips,
                    Some(&mut transaction_visitor),
                    &self.chain_state.stakes,
                    protocol_version,
                ) {
                    Ok(utxo_diff) => {
                        let priorities = transaction_visitor.take_state();

                        self.best_candidate = Some(BlockCandidate {
                            block: block.clone(),
                            utxo_diff,
                            reputation,
                            vrf_proof,
                            priorities,
                        });

                        self.broadcast_item(InventoryItem::Block(block));
                    }
                    Err(e) => {
                        log::warn!("Error when processing a block candidate {hash_block}: {e}");

                        // In order to do not block possible validate candidates in AlmostSynced
                        // state, we would broadcast the errors too
                        if self.sm_state == StateMachine::AlmostSynced {
                            self.broadcast_item(InventoryItem::Block(block));
                        }
                    }
                }
            } else {
                log::trace!("Block candidate already seen: {hash_block}");
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

        ctx.wait(self.persist_items(to_persist).map(|_, _, _| ()));
    }

    fn consolidate_block(
        &mut self,
        ctx: &mut Context<Self>,
        block: Block,
        utxo_diff: Diff,
        priorities: Priorities,
        resynchronizing: bool,
    ) {
        // Update chain_info and reputation_engine
        let own_pkh = match self.own_pkh {
            Some(x) => x,
            None => {
                log::error!("No OwnPkh loaded in ChainManager");
                return;
            }
        };

        match self.chain_state {
            ChainState {
                chain_info: Some(ref mut chain_info),
                reputation_engine: Some(ref mut reputation_engine),
                ref mut stakes,
                ..
            } => {
                let block_epoch = block.block_header.beacon.checkpoint;
                let block_hash = block.versioned_hash(get_protocol_version(Some(block_epoch)));
                let block_signals = block.block_header.signals;
                let validator_count = stakes.validator_count();

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

                let miner_pkh = block.block_header.proof.proof.pkh();

                // Check total amount staked to make sure we can activate wit/2
                let superblock_period = chain_info.consensus_constants.superblock_period;
                if get_protocol_version(Some(block_epoch)) == ProtocolVersion::V1_8
                    && get_protocol_version_activation_epoch(ProtocolVersion::V2_0) == Epoch::MAX
                    && block_epoch % u32::from(superblock_period) == 0
                {
                    let min_total_stake = self
                        .consensus_constants_wit2
                        .get_wit2_minimum_total_stake_nanowits();
                    let activation_delay = self
                        .consensus_constants_wit2
                        .get_wit2_activation_delay_epochs();
                    let checkpoint_period =
                        self.consensus_constants_wit2.get_checkpoints_period_wit2();
                    if stakes.total_staked() >= Wit::from(min_total_stake) {
                        let scheduled_epoch = block_epoch + activation_delay;
                        log::info!(
                            "The stake threshold for activating protocol V2_0 has been met. Protocol V2_0 will be scheduled for epoch {scheduled_epoch}"
                        );
                        // Register the 2_0 protocol into global state
                        register_protocol_version(
                            ProtocolVersion::V2_0,
                            scheduled_epoch,
                            checkpoint_period,
                        );

                        if let Some(epoch_constants) = &mut self.epoch_constants {
                            match epoch_constants.set_values_for_wit2(
                                checkpoint_period,
                                block_epoch + activation_delay,
                            ) {
                                Ok(_) => (),
                                Err(_) => panic!("Could not set wit/2 checkpoint variables"),
                            };
                        } else {
                            panic!("Could not set wit/2 checkpoint variables");
                        }
                    }
                }

                process_wit2_stakes_changes(
                    &block,
                    &self.consensus_constants_wit2,
                    &self.chain_state.data_request_pool,
                    stakes,
                    self.epoch_constants.unwrap(),
                    &UtxoDiff::new(&self.chain_state.unspent_outputs_pool, block_epoch),
                );

                let rep_info = update_pools(
                    &block,
                    &mut self.chain_state.unspent_outputs_pool,
                    &mut self.chain_state.data_request_pool,
                    &mut self.transactions_pool,
                    utxo_diff,
                    own_pkh,
                    &mut self.chain_state.own_utxos,
                    &mut self.chain_state.node_stats,
                    self.sm_state,
                );

                // Do not update reputation or stakes when consolidating genesis block
                if block_hash != chain_info.consensus_constants.genesis_hash {
                    if ProtocolVersion::from_epoch(block_epoch) < ProtocolVersion::V2_0 {
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

                    let stake_txns_count = block.txns.stake_txns.len();
                    if stake_txns_count > 0 {
                        log::debug!("Processing {stake_txns_count} stake transactions");

                        let minimum_stakeable = self
                            .consensus_constants_wit2
                            .get_validator_min_stake_nanowits(block_epoch);

                        let _ = process_stake_transactions(
                            stakes,
                            block.txns.stake_txns.iter(),
                            block_epoch,
                            minimum_stakeable,
                        );
                    }

                    let unstake_txns_count = block.txns.unstake_txns.len();
                    if unstake_txns_count > 0 {
                        log::debug!("Processing {unstake_txns_count} stake transactions");

                        let minimum_stakeable = self
                            .consensus_constants_wit2
                            .get_validator_min_stake_nanowits(block_epoch);

                        let _ = process_unstake_transactions(
                            stakes,
                            block.txns.unstake_txns.iter(),
                            block_epoch,
                            minimum_stakeable,
                        );
                    }
                }

                // Update bn256 public keys with block information
                self.chain_state.alt_keys.insert_keys_from_block(&block);

                // Insert candidate block into `block_chain` state
                self.chain_state.block_chain.insert(block_epoch, block_hash);

                // Update votes counter for WIPs
                self.chain_state.tapi_engine.update_bit_counter(
                    block_signals,
                    block_epoch,
                    block_epoch,
                    &HashSet::default(),
                );

                match self.sm_state {
                    StateMachine::WaitingConsensus => {
                        // Persist finished data requests into storage
                        let to_be_stored =
                            self.chain_state.data_request_pool.finished_data_requests();

                        if !resynchronizing {
                            self.persist_data_requests(ctx, to_be_stored);
                        }

                        let reveals = self
                            .chain_state
                            .data_request_pool
                            .update_data_request_stages(Some(validator_count), Some(block_epoch));

                        for reveal in reveals {
                            // Send AddTransaction message to self
                            // And broadcast it to all of peers
                            ctx.address().do_send(AddTransaction {
                                transaction: Transaction::Reveal(reveal),
                                broadcast_flag: true,
                            })
                        }

                        if !resynchronizing {
                            ctx.wait(
                                self.persist_items(vec![StoreInventoryItem::Block(Box::new(
                                    block,
                                ))])
                                .map(|_, _, _| ()),
                            );
                        }
                    }
                    StateMachine::Synchronizing => {
                        // In Synchronizing stage, blocks and data requests are persisted
                        // trough batches in AddBlocks handler
                        let reveals = self
                            .chain_state
                            .data_request_pool
                            .update_data_request_stages(Some(validator_count), Some(block_epoch));

                        for reveal in reveals {
                            // Send AddTransaction message to self
                            // And broadcast it to all of peers
                            ctx.address().do_send(AddTransaction {
                                transaction: Transaction::Reveal(reveal),
                                broadcast_flag: true,
                            })
                        }
                    }
                    StateMachine::AlmostSynced | StateMachine::Synced => {
                        // Persist finished data requests into storage
                        let to_be_stored =
                            self.chain_state.data_request_pool.finished_data_requests();
                        for dr_info in &to_be_stored {
                            show_tally_info(dr_info.tally.as_ref().unwrap(), block_epoch);
                        }

                        if !resynchronizing {
                            self.persist_data_requests(ctx, to_be_stored);
                        }

                        let reveals = self
                            .chain_state
                            .data_request_pool
                            .update_data_request_stages(Some(validator_count), Some(block_epoch));

                        show_info_dr(&self.chain_state.data_request_pool, &block);

                        for reveal in reveals {
                            // Send AddTransaction message to self
                            // And broadcast it to all of peers
                            ctx.address().do_send(AddTransaction {
                                transaction: Transaction::Reveal(reveal),
                                broadcast_flag: true,
                            })
                        }
                        // Persist blocks and transactions but do not persist chain_state, it will
                        // be persisted on superblock consolidation
                        // FIXME(#1663): discard persisted and non-consolidated blocks
                        // This means that after a reorganization a call to getBlock or
                        // getTransaction will show the content without any warning that the block
                        // is not on the main chain. To fix this we could remove forked blocks when
                        // a reorganization is detected.
                        if !resynchronizing {
                            ctx.wait(
                                self.persist_items(vec![StoreInventoryItem::Block(Box::new(
                                    block.clone(),
                                ))])
                                .map(|_, _, _| ()),
                            );
                        }

                        // Send notification to JsonRpcServer
                        JsonRpcServer::from_registry().do_send(BlockNotify { block })
                    }
                }

                // Update transaction priority information
                self.priority_engine.push_priorities(priorities);

                if miner_pkh == own_pkh {
                    self.chain_state.node_stats.block_mined_count += 1;
                    if self.sm_state == StateMachine::Synced {
                        log::info!(
                            "Congratulations! Your block was consolidated into the block chain by an apparent majority of peers"
                        );
                    } else {
                        // During synchronization, we assume that every consolidated block has, at least, one proposed block.
                        self.chain_state.node_stats.block_proposed_count += 1;
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
        self.construct_superblock(current_epoch, None)
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
                    .map(|res| {
                        res.map_err(|e| {
                            log::error!("Failed to sign superblock with bn256 key: {e}");
                        })
                    })
                    .into_actor(act)
                    .and_then(move |bn256_keyed_signature, act, _ctx| {
                        // Actually, we don't need to include the BN256 public key because
                        // it is stored in the `alt_keys` mapping, indexed by the
                        // secp256k1 public key hash
                        let bn256_signature = bn256_keyed_signature.signature;
                        superblock_vote.set_bn256_signature(bn256_signature);
                        let secp256k1_message = superblock_vote.secp256k1_signature_message();
                        let sign_bytes = calculate_sha256(&secp256k1_message).0;
                        signature_mngr::sign_data(sign_bytes)
                            .map(move |res| {
                                res.map(|secp256k1_signature| {
                                    superblock_vote.set_secp256k1_signature(secp256k1_signature);

                                    superblock_vote
                                })
                                .map_err(|e| {
                                    log::error!(
                                        "Failed to sign superblock with secp256k1 key: {e}"
                                    );
                                })
                            })
                            .into_actor(act)
                    })
                    .map_ok(move |res, act, ctx| {
                        // Broadcast vote between one and ("superblock_period" - 5) epoch checkpoints later.
                        // This is used to prevent the race condition described in issue #1573
                        // It is also used to spread the CPU load by checking superblock votes along
                        // the superblock period with a safe margin
                        let mut rng = rand::thread_rng();
                        // Should be safe here to just call unwraps
                        let checkpoints_period = act
                            .epoch_constants
                            .unwrap()
                            .get_epoch_period(current_epoch)
                            .unwrap();
                        let superblock_period = act.consensus_constants().superblock_period;
                        let end_range = if superblock_period > 5 {
                            (superblock_period - 5) * checkpoints_period
                        } else {
                            checkpoints_period
                        };
                        let random_waiting = rng.gen_range(checkpoints_period, end_range + 1);
                        ctx.run_later(
                            Duration::from_secs(u64::from(random_waiting)),
                            |act, ctx| act.add_superblock_vote(res, ctx),
                        );
                    })
            })
            .map(|_res: Result<(), ()>, _act, _ctx| ())
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

    fn add_temp_superblock_votes(&mut self, ctx: &mut Context<Self>) {
        let consensus_constants = self.consensus_constants();

        let superblock_period = u32::from(consensus_constants.superblock_period);

        for superblock_vote in std::mem::take(&mut self.temp_superblock_votes) {
            log::debug!("add_temp_superblock_votes {superblock_vote:?}");
            // Check if we already received this vote
            if self.chain_state.superblock_state.contains(&superblock_vote) {
                continue;
            }

            // Validate secp256k1 signature
            signature_mngr::verify_signatures(vec![SignaturesToVerify::SuperBlockVote {
                superblock_vote: superblock_vote.clone(),
            }])
            .map(|res| {
                res.map_err(|e| {
                    log::error!("Verify superblock vote signature: {e}");
                })
            })
            .into_actor(self)
            .and_then(move |(), act, _ctx| {
                // Check if we already received this vote (again, because this future can be executed
                // by multiple tasks concurrently)
                if act.chain_state.superblock_state.contains(&superblock_vote) {
                    return actix::fut::ok(());
                }
                act.chain_state.superblock_state.add_vote(
                    &superblock_vote,
                    act.current_epoch.unwrap_or(0) / superblock_period,
                );

                actix::fut::ok(())
            })
            .map(|_res: Result<(), ()>, _act, _ctx| ())
            .spawn(ctx);
        }
    }

    fn add_superblock_vote(&mut self, superblock_vote: SuperBlockVote, ctx: &mut Context<Self>) {
        log::trace!(
            "AddSuperBlockVote received while StateMachine is in state {:?}",
            self.sm_state
        );
        let consensus_constants = self.consensus_constants();

        let superblock_period = u32::from(consensus_constants.superblock_period);

        if self.sm_state != StateMachine::Synced {
            self.temp_superblock_votes.insert(superblock_vote.clone());
        }

        // Check if we already received this vote
        if self.chain_state.superblock_state.contains(&superblock_vote) {
            return;
        }

        // Validate secp256k1 signature
        signature_mngr::verify_signatures(vec![SignaturesToVerify::SuperBlockVote {
            superblock_vote: superblock_vote.clone(),
        }])
        .into_actor(self)
        .map_err(|e, _act, _ctx| {
            log::error!("Verify superblock vote signature: {e}");
        })
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
            let should_broadcast = match act.chain_state.superblock_state.add_vote(
                &superblock_vote,
                act.current_epoch.unwrap_or(0) / superblock_period,
            ) {
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
                .into_actor(act)
                .map_err(|e, _act, _ctx| {
                    log::error!("Forward superblock vote: {e}");
                })
        })
        .map(|_res: Result<(), ()>, _act, _ctx| ())
        .spawn(ctx);
    }

    #[must_use]
    fn add_transaction(
        &mut self,
        msg: AddTransaction,
        timestamp_now: i64,
    ) -> ResponseActFuture<Self, Result<(), anyhow::Error>> {
        log::trace!(
            "AddTransaction received while StateMachine is in state {:?}",
            self.sm_state
        );
        // Ignore AddTransaction when not in Synced state
        match self.sm_state {
            StateMachine::Synced | StateMachine::AlmostSynced => {}
            _ => match (&msg.transaction, self.own_pkh) {
                (Transaction::Reveal(reveal), Some(own_pkh)) if reveal.body.pkh == own_pkh => {
                    // The node will always include our own reveals, it doesn't matter in which state we are
                }
                _ => {
                    return Box::pin(actix::fut::err(
                        ChainManagerError::NotSynced {
                            current_state: self.sm_state,
                        }
                        .into(),
                    ));
                }
            },
        };

        if self.seen_transactions.contains(&msg.transaction) {
            log::trace!(
                "Transaction is already in the pool: {}",
                msg.transaction.hash()
            );
            return Box::pin(actix::fut::ok(()));
        }
        self.seen_transactions.insert(msg.transaction.clone());

        match self.transactions_pool.contains(&msg.transaction) {
            Ok(false) => {
                self.transactions_pool
                    .insert_unconfirmed_transactions(msg.transaction.hash());
            }
            Ok(true) => {
                log::trace!(
                    "Transaction is already in the pool: {}",
                    msg.transaction.hash()
                );
                return Box::pin(actix::fut::ok(()));
            }
            Err(e) => {
                log::warn!("Cannot add transaction: {e}");
                return Box::pin(actix::fut::err(e.into()));
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
            if let Transaction::Commit(_) | Transaction::Reveal(_) = &msg.transaction {
                let timestamp_mining = epoch_constants
                    .block_mining_timestamp(current_epoch)
                    .unwrap();

                if timestamp_now > timestamp_mining {
                    // Only reveals are saved since commits which arrive too late will be invalid in the next epoch
                    if let Transaction::Reveal(reveal) = &msg.transaction {
                        self.temp_reveals.push(reveal.clone());
                    }
                    return Box::pin(actix::fut::ok(()));
                }
            }

            let mut signatures_to_verify = vec![];
            let mut vrf_input = chain_info.highest_vrf_output;
            vrf_input.checkpoint = current_epoch;
            let active_wips = ActiveWips {
                active_wips: self.chain_state.tapi_engine.wip_activation.clone(),
                // If this transaction will be included in a block, the block epoch must be greater
                // than or equal to the current epoch
                block_epoch: current_epoch,
            };
            // For commit transactions, we want to verify their validity using the protocol version at
            // the time of the data request inclusion. For simplicity's sake when validating signatures,
            // we use the protocol version of the block epoch.
            // For all other transactions which can pass through the add_transaction function and which
            // need the protocol version, we can always use the protocol version of the block epoch.
            let protocol_version = if let Transaction::Commit(co_tx) = &msg.transaction {
                if let Some(dr_state) = self
                    .chain_state
                    .data_request_pool
                    .data_request_state(&co_tx.body.dr_pointer)
                {
                    ProtocolVersion::from_epoch(dr_state.epoch)
                } else {
                    ProtocolVersion::from_epoch(current_epoch)
                }
            } else {
                ProtocolVersion::from_epoch(current_epoch)
            };
            let collateral_age = self
                .consensus_constants_wit2
                .get_collateral_age(&active_wips);
            let required_reward_collateral_ratio =
                PSEUDO_CONSENSUS_CONSTANTS_WIP0022_REWARD_COLLATERAL_RATIO;
            let max_rounds = chain_info.consensus_constants.extra_rounds + 1;
            let fut = future::ready(validate_new_transaction(
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
                collateral_age,
                chain_info.consensus_constants.max_vt_weight,
                chain_info.consensus_constants.max_dr_weight,
                chain_info.consensus_constants.minimum_difficulty,
                required_reward_collateral_ratio,
                &active_wips,
                chain_info.consensus_constants.superblock_period,
                &self.chain_state.stakes,
                protocol_version,
                max_rounds,
                &self.consensus_constants_wit2,
            ))
            .into_actor(self)
            .and_then(move |fee, act, _ctx| {
                signature_mngr::verify_signatures(signatures_to_verify)
                    .map(move |res| res.map(|()| fee))
                    .into_actor(act)
            })
            .then(move |res, act, _ctx| match res {
                Ok(fee) => {
                    // Broadcast valid transaction
                    if msg.broadcast_flag {
                        act.broadcast_item(InventoryItem::Transaction(msg.transaction.clone()));
                    }

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

            Box::pin(fut)
        } else {
            Box::pin(actix::fut::err(ChainManagerError::ChainNotReady.into()))
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
        superblock_epoch: u32,
    ) -> ResponseActFuture<Self, Result<(), ()>> {
        let fut = self.construct_superblock(superblock_epoch, None).and_then(
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

                async {
                    match signature_mngr::bn256_sign(bn256_message).await {
                        Err(e) => {
                            log::error!("Failed to sign superblock with bn256 key: {e}");
                            Err(())
                        }
                        Ok(bn256_keyed_signature) => {
                            // There is no need to include the BN256 public key because it is stored in
                            // the `alt_keys` mapping, indexed by the secp256k1 public key hash
                            superblock_vote.set_bn256_signature(bn256_keyed_signature.signature);
                            let secp256k1_message = superblock_vote.secp256k1_signature_message();
                            let sign_bytes = calculate_sha256(&secp256k1_message).0;
                            signature_mngr::sign_data(sign_bytes)
                                .await
                                .map(move |secp256k1_signature| {
                                    superblock_vote.set_secp256k1_signature(secp256k1_signature);

                                    superblock_vote
                                })
                                .map_err(|e| {
                                    log::error!(
                                        "Failed to sign superblock with secp256k1 key: {e}"
                                    );
                                })
                        }
                    }
                }
                .into_actor(act)
                .and_then(|res, act, ctx| {
                    act.add_superblock_vote(res, ctx);

                    actix::fut::ok(())
                })
            },
        );

        Box::pin(fut)
    }

    /// Try to consolidate superblock process which uses futures
    #[must_use]
    pub fn try_consolidate_superblock(
        &mut self,
        block_epoch: u32,
        sync_target: SyncTarget,
        sync_superblock: Option<SuperBlock>,
    ) -> ResponseActFuture<Self, Result<(), ()>> {
        let fut = self
            .construct_superblock(block_epoch, sync_superblock)
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

                    act.persist_chain_state(Some(sync_target.superblock.checkpoint))
                } else {
                    // The superblock hash is different from what it should be.
                    log::error!(
                        "Mismatching superblock. Target: {:?} Created #{} {} {:?}",
                        sync_target,
                        superblock.index,
                        superblock.hash(),
                        superblock
                    );
                    act.update_state_machine(StateMachine::WaitingConsensus, ctx);
                    act.initialize_from_storage(ctx);
                    log::info!("Restored chain state from storage");

                    // If we are not synchronizing, forget about when we started synchronizing
                    act.sync_waiting_for_add_blocks_since = None;
                    Box::pin(actix::fut::err(()))
                }
            });

        Box::pin(fut)
    }

    /// Construct superblock process which uses futures
    #[must_use]
    pub fn construct_superblock(
        &mut self,
        block_epoch: u32,
        sync_superblock: Option<SuperBlock>,
    ) -> ResponseActFuture<Self, Result<SuperBlock, ()>> {
        let consensus_constants = self.consensus_constants();

        let superblock_period = u32::from(consensus_constants.superblock_period);

        let superblock_index = block_epoch / superblock_period;
        if superblock_index == 0 {
            panic!("Superblock 0 should not be created! Block epoch: {block_epoch}");
        }
        // This is the superblock for which we will be counting votes, and if there is consensus,
        // it will be the new consolidated superblock
        let voted_superblock_beacon = self.chain_state.superblock_state.get_beacon();
        let last_consolidated_beacon = self.chain_state.get_superblock_beacon();

        let inventory_manager = InventoryManager::from_registry();

        let init_epoch = block_epoch - superblock_period;
        let final_epoch = block_epoch.saturating_sub(1);
        let genesis_hash = consensus_constants.genesis_hash;
        let res = self.get_blocks_epoch_range(GetBlocksEpochRange::new_with_limit(
            init_epoch..=final_epoch,
            0,
        ));

        let fut = async move {
            let block_hashes = res.into_iter().map(|(_epoch, hash)| hash);
            let aux = block_hashes.map(move |hash| {
                inventory_manager
                    .send(GetItemBlock { hash })
                    .then(move |res| match res {
                        Ok(Ok(block)) => future::ready(Ok(block.block_header)),
                        Ok(Err(e)) => {
                            log::error!("Error in GetItemBlock {hash}: {e}");
                            future::ready(Err(()))
                        }
                        Err(e) => {
                            log::error!("Error in GetItemBlock {hash}: {e}");
                            future::ready(Err(()))
                        }
                    })
                    .then(|x| future::ready(Ok(x.ok())))
            });

            try_join_all(aux).await
                // Map Option<Vec<T>> to Vec<T>, this returns all the non-error results
                .map(|x| x.into_iter().flatten().collect::<Vec<BlockHeader>>())
        }
            .into_actor(self)
            .and_then(move |block_headers, act, _ctx| {
                let v = act
                    .get_blocks_epoch_range(
                        GetBlocksEpochRange::new_with_limit_from_end(..init_epoch, 1),
                    );
                let last_hash = v.first()
                            .map(|(_epoch, hash)| *hash)
                            .unwrap_or(genesis_hash);

                actix::fut::ok((block_headers, last_hash))
            })
            .and_then(move |(block_headers, last_hash), act, ctx| {
                let consensus = if act.sm_state == StateMachine::Synced || act.sm_state == StateMachine::AlmostSynced {

                    if voted_superblock_beacon.checkpoint == last_consolidated_beacon.checkpoint {
                        log::debug!("Counting votes for an already consolidated superblock index {} when the current superblock index is {}",
                                    voted_superblock_beacon.checkpoint,
                                    superblock_index
                        );

                        if (superblock_index - voted_superblock_beacon.checkpoint) % 2 == 0 {
                            // Desync bug: if the last valid superblock is even, all the future
                            // voted superblocks must be odd.
                            //
                            // superblock index: status
                            // 10: consolidated
                            // 11: consensus unknown, reverted
                            // 12: 10 consolidated again
                            // 13: consensus unknown, reverted
                            //
                            // So superblock voting must always happen after 2n+1 superepochs
                            // (11 and 13 in the example).
                            // If we are in a superblock index that should have votes, we must skip
                            // the consolidation of the already consolidated superblock. This will
                            // trigger a revert, and therefore in the next superepoch we will try to
                            // consolidate the same superblock again, but then the superblock
                            // checkpoint will be valid.
                            log::error!("Superblock index desync! It is not possible to consolidate superblock {} when current superblock index is {}", voted_superblock_beacon.checkpoint, superblock_index);

                            SuperBlockConsensus::Unknown
                        } else {
                            SuperBlockConsensus::SameAsLocal
                        }
                    } else {
                        if voted_superblock_beacon.checkpoint + 1 != superblock_index {
                            // Warn when there is are missing superblocks between the one that will be
                            // consolidated and the one that will be created
                            log::warn!("Counting votes for Superblock {voted_superblock_beacon:?} when the current superblock index is {superblock_index}");
                        }

                        act.chain_state.superblock_state.has_consensus()
                    }

                } else {
                    log::debug!("The node is not synced yet, so assume that superblock {voted_superblock_beacon:?} is valid");

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

                        let fut: Pin<Box<dyn ActorFuture<Self, Output = Result<_, ()>>>> = if act.sm_state == StateMachine::Synced || act.sm_state == StateMachine::AlmostSynced {
                            // Persist previous_chain_state with current superblock_state
                            Box::pin(act.persist_chain_state(Some(voted_superblock_beacon.checkpoint)).map(move |_res: Result<(), ()>, act, _ctx| {
                                act.move_chain_state_forward(superblock_index);

                                Ok((block_headers, last_hash))
                            }))
                        } else {
                            Box::pin(actix::fut::ok((block_headers, last_hash)))
                        };

                        fut
                }
                SuperBlockConsensus::Different(target_superblock_hash) => {
                    // No consensus: move to waiting consensus and restore chain_state from storage
                    // TODO: it could be possible to synchronize with a target superblock hash
                    log::warn!(
                        "Superblock consensus #{}: {} different from current superblock with {} out of {} votes. Committee size: {}",
                        voted_superblock_beacon.checkpoint,
                        target_superblock_hash,
                        act.chain_state.superblock_state.votes_counter_from_superblock(&target_superblock_hash),
                        act.chain_state.superblock_state.valid_votes_counter(),
                        act.chain_state.superblock_state.get_committee_length()
                    );

                    let consensus_superblock = CheckpointBeacon{
                        checkpoint: voted_superblock_beacon.checkpoint,
                        hash_prev_block: target_superblock_hash,
                    };

                    // Include superblock target beacon in SessionsManager
                    // This allow to look for peers that are currently synced in the last superblock consensus
                    let sessions_manager_addr = SessionsManager::from_registry();
                    sessions_manager_addr.do_send(SetSuperBlockTargetBeacon {beacon: Some(consensus_superblock)});

                    // Update last superblock consensus in ChainManager
                    act.last_superblock_consensus = Some(consensus_superblock);

                    act.initialize_from_storage(ctx);
                    act.update_state_machine(StateMachine::WaitingConsensus, ctx);

                    Box::pin(actix::fut::err(()))
                }
                SuperBlockConsensus::NoConsensus => {
                    // No consensus: move to AlmostSynced and restore chain_state from storage
                    if let Some((sb_hash, votes_counter)) = act.chain_state.superblock_state.most_voted_superblock() {
                        log::warn!("No superblock consensus for #{}. Most voted superblock: {} with {} out of {} votes. Committee size: {}",
                                   voted_superblock_beacon.checkpoint,
                                   sb_hash,
                                   votes_counter,
                                   act.chain_state.superblock_state.valid_votes_counter(),
                                   act.chain_state.superblock_state.get_committee_length()
                        );
                    } else {
                        log::warn!("No superblock consensus for #{}. Total votes: {}. Committee size: {}",
                                   voted_superblock_beacon.checkpoint,
                                   act.chain_state.superblock_state.valid_votes_counter(),
                                   act.chain_state.superblock_state.get_committee_length()
                        );
                    }

                    // Remove superblock beacon target in SessionsManager
                    let sessions_manager_addr = SessionsManager::from_registry();
                    sessions_manager_addr.do_send(SetSuperBlockTargetBeacon {beacon: None});

                    act.reinsert_transactions_from_unconfirmed_blocks(init_epoch.saturating_sub(superblock_period)).map(|_res: Result<(), ()>, _act, _ctx| ()).wait(ctx);

                    act.initialize_from_storage(ctx);
                    act.update_state_machine(StateMachine::AlmostSynced, ctx);

                    Box::pin(actix::fut::err(()))
                }
                SuperBlockConsensus::Unknown => {
                    // Consensus unknown: move to waiting consensus and restore chain_state from storage
                    if let Some((sb_hash, votes_counter)) = act.chain_state.superblock_state.most_voted_superblock() {
                        log::warn!("Superblock consensus unknown for #{}. Most voted superblock: {} with {} out of {} votes. Committee size: {}",
                                   voted_superblock_beacon.checkpoint,
                                   sb_hash,
                                   votes_counter,
                                   act.chain_state.superblock_state.valid_votes_counter(),
                                   act.chain_state.superblock_state.get_committee_length()
                        );
                    } else {
                        log::warn!("Superblock consensus unknown for #{}. Total votes: {}. Committee size: {}",
                                   voted_superblock_beacon.checkpoint,
                                   act.chain_state.superblock_state.valid_votes_counter(),
                                   act.chain_state.superblock_state.get_committee_length()
                        );
                    }

                    // Remove superblock beacon target in SessionsManager
                    let sessions_manager_addr = SessionsManager::from_registry();
                    sessions_manager_addr.do_send(SetSuperBlockTargetBeacon {beacon: None});

                    act.reinsert_transactions_from_unconfirmed_blocks(init_epoch.saturating_sub(superblock_period)).map(|_res: Result<(), ()>, _act, _ctx| ()).wait(ctx);

                    act.initialize_from_storage(ctx);
                    act.update_state_machine(StateMachine::WaitingConsensus, ctx);

                    Box::pin(actix::fut::err(()))
                }
            }
        })
            .and_then(move |(block_headers, last_hash), act, _ctx|  {
                if let Some(consolidated_superblock) = act.chain_state.superblock_state.get_current_superblock() {
                    let sb_hash = consolidated_superblock.hash();
                    // Let JSON-RPC clients know that the blocks in the previous superblock can now
                    // be considered consolidated
                    act.notify_superblock_consolidation(consolidated_superblock);

                    log::info!("Consensus reached for Superblock #{} with {} out of {} votes. Committee size: {}",
                                       voted_superblock_beacon.checkpoint,
                                       act.chain_state.superblock_state.votes_counter_from_superblock(&sb_hash),
                                       act.chain_state.superblock_state.valid_votes_counter(),
                                       act.chain_state.superblock_state.get_committee_length(),
                            );
                    log::debug!("Current tip of the chain: {:?}", act.get_chain_beacon());
                    log::debug!(
                                "The last block of the consolidated superblock is {last_hash}"
                            );

                    // Update mempool after superblock consolidation
                    act.transactions_pool.update_unconfirmed_transactions();
                }

                let chain_info = act.chain_state.chain_info.as_ref().unwrap();
                let reputation_engine = act.chain_state.reputation_engine.as_ref().unwrap();
                let last_superblock_signed_by_bootstrap = last_superblock_signed_by_bootstrap(&chain_info.consensus_constants);

                let ars_members =
                    // Before reaching the epoch activity_period + collateral_age the bootstrap committee signs the superblock
                    // collateral_age is measured in blocks instead of epochs, but this only means that the period in which
                    // the bootstrap committee signs is at least epoch activity_period + collateral_age
                    if let Some(ars_members) = in_emergency_period(superblock_index, get_environment()) {
                        // Bootstrap committee
                        ars_members
                    } else if superblock_index >= last_superblock_signed_by_bootstrap {
                        reputation_engine.get_rep_ordered_ars_list()
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
                let ars_identities = ARSIdentities::new(ars_members);

                // After the second hard fork, the superblock committee size must be at least 50
                let min_committee_size = if after_second_hard_fork(block_epoch, get_environment()) {
                    50
                } else {
                    // Before that hard fork, the minimum was 1 identity
                    1
                };

                // Committee size should decrease if sufficient epochs have elapsed since last confirmed superblock
                let committee_size = current_committee_size_requirement(
                    consensus_constants.superblock_signing_committee_size,
                    act.chain_state.superblock_state.get_committee_length(),
                    consensus_constants.superblock_committee_decreasing_period,
                    consensus_constants.superblock_committee_decreasing_step,
                    chain_info.highest_superblock_checkpoint.checkpoint,
                    superblock_index,
                    last_superblock_signed_by_bootstrap,
                    min_committee_size,
                );

                let superblock = act.chain_state.superblock_state.build_superblock(
                    &block_headers,
                    ars_identities,
                    committee_size,
                    superblock_index,
                    last_hash,
                    &act.chain_state.alt_keys,
                    sync_superblock,
                    block_epoch,
                );

                // Put the local superblock into chain state
                act.chain_state
                    .superblock_state
                    .set_current_superblock(superblock.clone());

                // Update last superblock consensus in ChainManager
                act.last_superblock_consensus = Some(voted_superblock_beacon);

                // Set last beacon in sessions manager
                let sessions_manager_addr = SessionsManager::from_registry();
                let chain_beacon = act.get_chain_beacon();
                sessions_manager_addr.do_send(SetLastBeacon {
                    beacon: LastBeacon{
                        highest_block_checkpoint: chain_beacon,
                        highest_superblock_checkpoint: voted_superblock_beacon,
                    },
                });

                // Remove superblock beacon target in order to use our own SuperBlockBeacon that
                // in this case is the same that the consensus one
                sessions_manager_addr.do_send(SetSuperBlockTargetBeacon {beacon: None});

                actix::fut::ok(superblock)
            });

        Box::pin(fut)
    }

    /// Block validation process which uses futures
    ///
    /// This is only used for mining (namely `ChainManager::try_mine_block()`), but not for
    /// candidate validation. Hence why no transaction visitor is used here.
    #[must_use]
    pub fn future_process_validations(
        &mut self,
        block: Block,
        previous_block_epoch: Epoch,
        current_epoch: Epoch,
        vrf_input: CheckpointVRF,
        chain_beacon: CheckpointBeacon,
        epoch_constants: EpochConstants,
    ) -> ResponseActFuture<Self, Result<Diff, anyhow::Error>> {
        let block_number = self.chain_state.block_number();
        let mut signatures_to_verify = vec![];
        let consensus_constants = self.consensus_constants();
        let active_wips = ActiveWips {
            active_wips: self.chain_state.tapi_engine.wip_activation.clone(),
            block_epoch: block.block_header.beacon.checkpoint,
        };
        let protocol_version = ProtocolVersion::from_epoch(block.block_header.beacon.checkpoint);
        let replication_factor = self
            .consensus_constants_wit2
            .get_replication_factor(block.block_header.beacon.checkpoint, previous_block_epoch);
        let res = validate_block(
            &block,
            current_epoch,
            vrf_input,
            chain_beacon,
            &mut signatures_to_verify,
            self.chain_state.reputation_engine.as_ref().unwrap(),
            &consensus_constants,
            &active_wips,
            &self.chain_state.stakes,
            protocol_version,
            replication_factor,
        );

        let fut = async move {
            // Short-circuit if validation failed
            res?;

            signature_mngr::verify_signatures(signatures_to_verify).await
        }
        .into_actor(self)
        .and_then(move |(), act, _ctx| {
            let mut signatures_to_verify = vec![];
            let res = validate_block_transactions(
                &act.chain_state.unspent_outputs_pool,
                &mut act.chain_state.data_request_pool,
                &block,
                vrf_input,
                &mut signatures_to_verify,
                act.chain_state.reputation_engine.as_ref().unwrap(),
                epoch_constants,
                block_number,
                &consensus_constants,
                &act.consensus_constants_wit2,
                &active_wips,
                None,
                &act.chain_state.stakes,
            );
            async move {
                // Short-circuit if validation failed
                let diff = res?;

                signature_mngr::verify_signatures(signatures_to_verify)
                    .await
                    .map(|()| diff)
            }
            .into_actor(act)
        });

        Box::pin(fut)
    }

    /// Transition the ChainManager state machine into a new state.
    ///
    /// This is expected to be the only means for updating the state machine, so debugging is easier
    /// and to ensure that every transition gets logged in a predictable format.
    fn update_state_machine(&mut self, next_state: StateMachine, ctx: &mut Context<Self>) {
        let same_state = self.sm_state == next_state;
        match (&self.sm_state, &next_state) {
            (old, _new) if same_state => log::debug!("State machine staying in state {old:?}"),
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

        if !same_state && next_state == StateMachine::AlmostSynced {
            self.add_temp_superblock_votes(ctx)
        }

        self.notify_node_status(next_state);
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
            .then(|res, act, ctx| match res {
                Ok(Ok(())) => actix::fut::ready(()),
                _ => {
                    // On error case go back to WaitingConsensus state
                    log::warn!("Failed to send LastBeacon to random peer");
                    if act.sm_state == StateMachine::Synchronizing {
                        act.update_state_machine(StateMachine::WaitingConsensus, ctx);
                        act.sync_waiting_for_add_blocks_since = None;
                    }

                    actix::fut::ready(())
                }
            })
            .spawn(ctx);
        let epoch = self.current_epoch.unwrap();
        self.sync_waiting_for_add_blocks_since = Some(epoch);
    }

    fn request_sync_target_superblock(
        &mut self,
        ctx: &mut Context<Self>,
        superblock_beacon: CheckpointBeacon,
    ) {
        let CheckpointBeacon {
            checkpoint: superblock_index,
            hash_prev_block: superblock_hash,
        } = superblock_beacon;

        if superblock_index == 0 {
            // No need to request the bootstrap superblock, because it does not exist
            return;
        }

        let already_have_this_superblock = self
            .sync_superblock
            .as_ref()
            .map(|(hash, _superblock)| hash == &superblock_hash)
            .unwrap_or(false);

        if !already_have_this_superblock {
            // Reset the old superblock, if any
            self.sync_superblock = None;

            SessionsManager::from_registry()
                .send(Anycast {
                    command: SendInventoryRequest {
                        items: vec![InventoryEntry::SuperBlock(superblock_index)],
                    },
                    safu: true,
                })
                .into_actor(self)
                .then(move |res, _act, ctx| match res {
                    Ok(Ok(())) => actix::fut::ready(()),
                    _ => {
                        // On error case go back to WaitingConsensus state
                        log::debug!("Failed to send InventoryRequest(Superblock) to random peer, retrying...");
                        ctx.run_later(Duration::from_secs(1), move |act, ctx| {
                            act.request_sync_target_superblock(ctx, superblock_beacon)
                        });

                        actix::fut::ready(())
                    }
                })
                .spawn(ctx);
        }
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
            if let Err(e) = self.process_requested_block(ctx, block.clone(), false) {
                log::error!("Error processing block: {e}");
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
            show_sync_progress(
                beacon,
                sync_target,
                self.epoch_constants.unwrap(),
                self.current_epoch.unwrap(),
            );
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
            self.process_blocks_batch(ctx, sync_target, blocks);

        if !batch_succeeded {
            log::error!("Received invalid blocks batch");
            self.update_state_machine(StateMachine::WaitingConsensus, ctx);
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
            <= self.chain_state.superblock_state.get_beacon().checkpoint
        {
            None
        } else {
            Some(sync_target.superblock.checkpoint * superblock_period)
        }
    }

    fn superblock_candidate_is_needed(
        &self,
        candidate_superblock_epoch: u32,
        superblock_period: u32,
    ) -> Option<u32> {
        if candidate_superblock_epoch <= self.chain_state.superblock_state.get_beacon().checkpoint {
            None
        } else {
            Some(candidate_superblock_epoch * superblock_period)
        }
    }

    /// Let JSON-RPC clients know that the blocks in the previous superblock can now
    /// be considered consolidated
    fn notify_superblock_consolidation(&mut self, superblock: SuperBlock) {
        let superblock_period = u32::from(self.consensus_constants().superblock_period);
        let final_epoch = superblock
            .index
            .checked_mul(superblock_period)
            .expect("Multiplying a superblock index by `superblock_period` should never overflow");
        let initial_epoch = final_epoch.saturating_sub(superblock_period);
        let beacons = self.get_blocks_epoch_range(GetBlocksEpochRange::new_with_limit(
            initial_epoch..final_epoch,
            0,
        ));

        // If there is a superblock to consolidate, and we got the confirmed block beacons, send
        // notification
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

    /// Let JSON-RPC clients know when the node changes its status
    fn notify_node_status(&mut self, node_status: StateMachine) {
        let new_node_status = NodeStatusNotify { node_status };
        JsonRpcServer::from_registry().do_send(new_node_status);
    }

    /// Get a list of (epoch, block_hash)
    fn get_blocks_epoch_range(
        &self,
        GetBlocksEpochRange {
            range,
            limit,
            limit_from_end,
        }: GetBlocksEpochRange,
    ) -> Vec<(Epoch, Hash)> {
        log::debug!("GetBlocksEpochRange received {range:?}");

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
            block_chain_range.collect()
        } else if limit_from_end {
            let mut hashes: Vec<(Epoch, Hash)> = block_chain_range
                // Take the last "limit" blocks
                .rev()
                .take(limit)
                .collect();

            // Reverse again to return them in non-reversed order
            hashes.reverse();

            hashes
        } else {
            block_chain_range
                // Take the first "limit" blocks
                .take(limit)
                .collect()
        }
    }

    /// This function takes the transactions included in unconfirmed blocks and it allows them to be
    /// included again in the mempool when the node wold be Synced again
    #[must_use]
    fn reinsert_transactions_from_unconfirmed_blocks(
        &mut self,
        epoch: Epoch,
    ) -> ResponseActFuture<Self, Result<(), ()>> {
        let inventory_manager = InventoryManager::from_registry();

        // Get all blocks since epoch
        let res = self.get_blocks_epoch_range(GetBlocksEpochRange::new_with_limit(epoch.., 0));

        let fut = async {
            let block_hashes = res.into_iter().map(|(_epoch, hash)| hash);
            // For each block, collect all the transactions that may be valid if this block is
            // reverted. This includes value transfer transactions and data request transactions.
            let aux = block_hashes.map(move |hash| {
                inventory_manager
                    .send(GetItemBlock { hash })
                    .then(move |res| match res {
                        Ok(Ok(block)) => {
                            let transactions: Vec<Transaction> = block
                                .txns
                                .value_transfer_txns
                                .iter()
                                .map(|vtt| Transaction::ValueTransfer(vtt.clone()))
                                .chain(
                                    block
                                        .txns
                                        .data_request_txns
                                        .iter()
                                        .map(|drt| Transaction::DataRequest(drt.clone())),
                                )
                                .collect();

                            // We do not reinsert RevealTransactions due to each node resend
                            // their reveal in case of a data request would be in REVEAL stage

                            future::ready(Ok(transactions))
                        }
                        Ok(Err(e)) => {
                            log::error!("Error in GetItemBlock {hash}: {e}");
                            future::ready(Err(()))
                        }
                        Err(e) => {
                            log::error!("Error in GetItemBlock {hash}: {e}");
                            future::ready(Err(()))
                        }
                    })
                    // TODO: make sure that we want to ignore errors
                    .then(|x| future::ready(Ok(x.ok())))
            });
            try_join_all(aux)
                .await
                // Map Option<Vec<Vec<T>>> to Vec<T>, this returns all the non-error results
                .map(|x| {
                    x.into_iter()
                        .flatten()
                        .flatten()
                        .collect::<Vec<Transaction>>()
                })
        }
        .into_actor(self)
        .and_then(move |transactions, act, _ctx| {
            // Include in temporal vts and drs to include them later
            act.temp_vts_and_drs.extend(transactions);

            actix::fut::ok(())
        })
        .map_err(|(), _, _| {
            // Errors at this point should be impossible because we explicitly ignore them
            panic!("Unknown error in reinsert_transactions_from_unconfirmed_blocks");
        });

        Box::pin(fut)
    }

    /// Send a message to `SessionsManager` to drop all outbound peers.
    pub fn drop_all_outbounds(&self) {
        let peers_to_unregister = self
            .last_received_beacons
            .iter()
            .map(|(addr, _)| *addr)
            .collect();
        let sessions_manager_addr = SessionsManager::from_registry();
        sessions_manager_addr.do_send(DropOutboundPeers {
            peers_to_drop: peers_to_unregister,
        });
    }

    /// Send a message to `PeersManager` to ice a specific peer.
    pub fn ice_peer(&self, addr: Option<SocketAddr>) {
        if let Some(addr) = addr {
            let peers_manager_addr = PeersManager::from_registry();
            peers_manager_addr.do_send(RemoveAddressesFromTried {
                addresses: vec![addr],
                ice: true,
            });
        }
    }

    /// Execute `drop_all_outbounds` and `ice_peer` at once.
    ///
    /// This is called when we receive an invalid batch of blocks. It will throw away our outbound
    /// peers in order to find new ones that can give us the blocks consolidated by the network,
    /// and ice the node that sent the invalid batch.
    pub fn drop_all_outbounds_and_ice_sender(&self, sender: Option<SocketAddr>) {
        self.drop_all_outbounds();
        // Ice the invalid blocks' batch sender
        self.ice_peer(sender);
    }

    /// Update new wip votes
    fn update_new_wip_votes(
        &mut self,
        init: Epoch,
        end: Epoch,
        old_wips: HashSet<String>,
    ) -> ResponseActFuture<Self, Result<(), ()>> {
        let inventory_manager = InventoryManager::from_registry();

        let res = self.get_blocks_epoch_range(GetBlocksEpochRange::new_with_limit(init..=end, 0));

        let fut = async move {
            let block_hashes = res.into_iter().map(|(_epoch, hash)| hash);
            log::debug!("Updating TAPI votes from blocks since #{init}");
            let mut block_counter = 0;
            let aux = block_hashes.map(move |hash| {
                block_counter += 1;
                inventory_manager
                    .send(GetItemBlock { hash })
                    .then(move |res| match res {
                        Ok(Ok(block)) => {
                            if block_counter % 1000 == 0 {
                                let block_epoch = block.block_header.beacon.checkpoint;
                                log::debug!("[{block_epoch}/{end}] Updating TAPI votes");
                            }
                            future::ready(Ok(block.block_header))
                        }
                        Ok(Err(e)) => {
                            log::error!("Error in GetItemBlock {hash}: {e}");
                            future::ready(Err(()))
                        }
                        Err(e) => {
                            log::error!("Error in GetItemBlock {hash}: {e}");
                            future::ready(Err(()))
                        }
                    })
                    .then(|x| future::ready(Ok(x.ok())))
            });

            try_join_all(aux)
                .await
                // Map Vec<Option<T>> to Vec<T>, this returns all the non-error results and ignores
                // the errors.
                .map(|x| x.into_iter().flatten())
        }
        .into_actor(self)
        .and_then(move |block_headers, act, _ctx| {
            for block_header in block_headers {
                act.chain_state.tapi_engine.update_bit_counter(
                    block_header.signals,
                    block_header.beacon.checkpoint,
                    block_header.beacon.checkpoint,
                    &old_wips,
                );
            }

            actix::fut::ok(())
        });

        Box::pin(fut)
    }

    /// Return the value of the version field for a block in this epoch
    fn tapi_signals_mask(&self, epoch: Epoch) -> u32 {
        let Tapi { oppose_wip0028 } = &self.tapi;

        let mut v = 0;
        // Bit 0
        // FIXME(#2051): Assess when remove achieved bit signaling
        let bit = 0;
        v |= 1 << bit;

        // Bit 1
        let bit = 1;
        v |= 1 << bit;

        // Bit 2
        let bit = 2;
        v |= 1 << bit;

        // Bit 3
        let bit = 3;
        v |= 1 << bit;

        // Bit 4
        let bit = 4;
        v |= 1 << bit;

        // Bit 5
        let bit = 5;
        v |= 1 << bit;

        // Bit 6
        let bit = 6;
        v |= 1 << bit;

        // Bit 7
        let bit = 7;
        v |= 1 << bit;

        // Bit 8
        let bit = 8;
        v |= 1 << bit;

        // Bit 9
        let bit = 9;
        if !oppose_wip0028
            && self
                .chain_state
                .tapi_engine
                .in_voting_range(epoch, "WIP0028")
        {
            v |= 1 << bit;
        }

        v
    }

    /// Update own utxos as that is necessary when importing a new master key
    fn update_own_utxos(&mut self) {
        log::info!("Updating own UTXO's for {}", self.own_pkh.unwrap());

        self.chain_state.own_utxos.drain();
        for (output_pointer, value_transfer_output) in self.chain_state.unspent_outputs_pool.iter()
        {
            if value_transfer_output.0.pkh == self.own_pkh.unwrap() {
                self.chain_state.own_utxos.insert(output_pointer, 0);
            }
        }
    }
}

/// The different errors that can occur while loading a chain snapshot.
#[derive(Debug, Display, Error)]
pub enum ImportError {
    /// An error while importing blocks. A more specific error is contained within.
    #[display(fmt = "Error importing blocks")]
    AtBlocks(Box<Self>),
    /// An error while importing chain state. A more specific error is contained within.
    #[display(fmt = "Error importing chain state")]
    AtChainState(Box<Self>),
    /// An error while importing superblocks. A more specific error is contained within.
    #[display(fmt = "Error importing superblocks")]
    AtSuperblocks(Box<Self>),
    /// A (de)serialization error.
    #[display(fmt = "bincode error")]
    Bincode(bincode::Error),
    /// The chain to be imported is behind our local chain.
    ///
    /// This error can be defeated by using the `force` flag.
    #[display(
        fmt = "The chain to be imported is behind our local chain ({imported} > {local}). If you still want to import it, use the `force` flag."
    )]
    ChainTip {
        /// The epoch of the imported chain tip.
        imported: Epoch,
        /// The local chain tip.
        local: Epoch,
    },
    /// A file cannot be deserialized.
    #[display(fmt = "Error deserializing file at {path}")]
    Deserialize {
        /// The path of the file that cannot be deserialized.
        path: String,
    },
    /// A file cannot be read.
    #[display(fmt = "Error reading file at {path}")]
    FileRead {
        /// The path of the file that cannot be read.
        path: String,
    },
    /// Any kind of I/O error.
    #[display(fmt = "std::io error")]
    Io(std::io::Error),
}

impl From<std::io::Error> for ImportError {
    fn from(value: std::io::Error) -> Self {
        ImportError::Io(value)
    }
}

impl From<bincode::Error> for ImportError {
    fn from(value: bincode::Error) -> Self {
        ImportError::Bincode(value)
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
    /// Superblock period, used for debugging
    pub superblock_period: u16,
}

impl ChainStateSnapshot {
    // Returns false if the snapshot did already exist
    // Returns true if the snapshot did not already exist
    // Panics if a different chain state was already saved for this super epoch
    fn take(&mut self, superblock_index: u32, state: &ChainState) -> bool {
        let chain_beacon = state.get_chain_beacon();
        let superblock_beacon = state.get_superblock_beacon();

        log::debug!(
            "Taking snapshot at superblock #{superblock_index}. Chain beacon {chain_beacon:?}, superblock beacon {superblock_beacon:?}"
        );

        let last_block_according_to_superblock =
            (superblock_index * u32::from(self.superblock_period)).saturating_sub(1);
        if chain_beacon.checkpoint > last_block_according_to_superblock {
            panic!(
                "Invalid snapshot: superblock #{} can only consolidate blocks up to #{}, but this chain state has block #{}",
                superblock_index, last_block_according_to_superblock, chain_beacon.checkpoint
            );
        }

        if let Some((prev_chain_state, prev_super_epoch)) = self.previous_chain_state.as_mut() {
            if *prev_super_epoch == superblock_index {
                log::warn!("ChainState snapshot {superblock_index} already exists");
                if prev_chain_state == state {
                    false
                } else {
                    // Only allow overwriting a different chain state if the superblock index is 0
                    if superblock_index == 0 {
                        log::warn!(
                            "ChainState mismatch in superblock #{superblock_index}. Overwritting old with new"
                        );
                        *prev_chain_state = state.clone();

                        true
                    } else {
                        // Two snapshots of the same superblock should be identical, this is a bug
                        panic!(
                            "ChainState mismatch for superblock #{superblock_index}: `{prev_chain_state:?} != {state:?}`"
                        );
                    }
                }
            } else {
                log::warn!(
                    "Overwriting old chain state snapshot, it was superblock #{prev_super_epoch}"
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
                "Tried to persist chain state for superblock #{super_epoch} but it is already persisted"
            );

            None
        } else if self.highest_persisted_superblock > super_epoch {
            panic!(
                "Tried to persist chain state for superblock #{} but it is already persisted. The highest persisted superblock is #{}",
                super_epoch, self.highest_persisted_superblock
            );
        } else {
            let skipped_superblocks = super_epoch - self.highest_persisted_superblock - 1;
            if skipped_superblocks > 0 {
                // This can happen when a new node is synchronizing: it will consolidate the top of
                // chain without consolidating all the previous superblocks
                log::debug!("Skipped {skipped_superblocks} superblocks in consolidation");
            }

            // Replace self.previous_chain_state with None to prevent consolidating the same chain
            // state more than once
            if let Some((chain_state, prev_super_epoch)) = self.previous_chain_state.take() {
                if prev_super_epoch != super_epoch {
                    panic!(
                        "Cannot persist chain state. There is no snapshot for superblock #{super_epoch}. The current snapshot is for superblock #{prev_super_epoch}"
                    );
                }

                self.highest_persisted_superblock = super_epoch;

                Some(chain_state)
            } else {
                panic!(
                    "Cannot persist chain state. There is no snapshot for superblock #{}. The highest persisted superblock is #{}",
                    super_epoch, self.highest_persisted_superblock
                );
            }
        }
    }

    // Remove the taken snapshot
    fn clear(&mut self) {
        self.previous_chain_state = None;
    }
}

/// Block validation process which doesn't use futures
///
/// This uses a `Visitor` that will visit each transaction as well as its fee and weight.
#[allow(clippy::too_many_arguments)]
pub fn process_validations(
    block: &Block,
    current_epoch: Epoch,
    vrf_input: CheckpointVRF,
    chain_beacon: CheckpointBeacon,
    rep_eng: &ReputationEngine,
    epoch_constants: EpochConstants,
    utxo_set: &UnspentOutputsPool,
    dr_pool: &mut DataRequestPool,
    vrf_ctx: &mut VrfCtx,
    block_number: u32,
    consensus_constants: &ConsensusConstants,
    consensus_constants_wit2: &ConsensusConstantsWit2,
    resynchronizing: bool,
    active_wips: &ActiveWips,
    transaction_visitor: Option<&mut dyn Visitor<Visitable = (Transaction, u64, u32)>>,
    stakes: &StakesTracker,
    protocol_version: ProtocolVersion,
) -> Result<Diff, anyhow::Error> {
    let replication_factor = consensus_constants_wit2.get_replication_factor(
        block.block_header.beacon.checkpoint,
        chain_beacon.checkpoint,
    );
    if !resynchronizing {
        let mut signatures_to_verify = vec![];
        validate_block(
            block,
            current_epoch,
            vrf_input,
            chain_beacon,
            &mut signatures_to_verify,
            rep_eng,
            consensus_constants,
            active_wips,
            stakes,
            protocol_version,
            replication_factor,
        )?;
        log::trace!("Verifying {} block signatures", signatures_to_verify.len());
        verify_signatures(signatures_to_verify, vrf_ctx)?;
    }

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
        consensus_constants_wit2,
        active_wips,
        transaction_visitor,
        stakes,
    )?;

    if !resynchronizing {
        log::trace!(
            "Verifying {} transaction signatures",
            signatures_to_verify.len()
        );
        verify_signatures(signatures_to_verify, vrf_ctx)?;
    }

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
        own_pkh: PublicKeyHash,
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
        if out_of_consensus.contains(&own_pkh) && !error_committers.contains(&own_pkh) {
            node_stats.slashed_count += 1;
        }
    }
}

fn process_wit2_stakes_changes(
    block: &Block,
    consensus_constants_wit2: &ConsensusConstantsWit2,
    data_request_pool: &DataRequestPool,
    stakes: &mut StakesTracker,
    epoch_constants: EpochConstants,
    utxo_diff: &UtxoDiff<'_>,
) {
    let block_epoch = block.block_header.beacon.checkpoint;
    let miner_pkh = block.block_header.proof.proof.pkh();

    // Reset the coin age of the miner for all staked coins
    if ProtocolVersion::from_epoch(block_epoch) == ProtocolVersion::V2_0 {
        let minimum_stakeable =
            consensus_constants_wit2.get_validator_min_stake_nanowits(block_epoch);

        let mut total_commit_reward = 0;
        for co_tx in &block.txns.commit_txns {
            if data_request_is_pre_wit2(data_request_pool, &Transaction::Commit(co_tx.clone())) {
                continue;
            }

            let commit_pkh = co_tx.body.proof.proof.pkh();
            total_commit_reward +=
                if let Some(dr_output) = data_request_pool.get_dr_output(&co_tx.body.dr_pointer) {
                    log::debug!(
                        "Reserving {} wit collateral from {}",
                        Wit::from(dr_output.collateral),
                        commit_pkh,
                    );
                    // Subtract collateral from staked balance
                    stakes
                        .reserve_collateral(
                            commit_pkh,
                            Wit::from(dr_output.collateral),
                            Wit::from(minimum_stakeable),
                        )
                        .unwrap();

                    dr_output.commit_and_reveal_fee
                } else {
                    0
                };
        }
        // Add commit reward
        if total_commit_reward > 0 {
            log::debug!(
                "Rewarding {} with {} wit for including commits",
                miner_pkh,
                Wit::from(total_commit_reward),
            );
            stakes
                .add_reward(miner_pkh, Wit::from(total_commit_reward), block_epoch)
                .unwrap();
        }

        // Add reveal rewards
        let mut total_reveal_reward = 0;
        for re_tx in &block.txns.reveal_txns {
            if data_request_is_pre_wit2(data_request_pool, &Transaction::Reveal(re_tx.clone())) {
                continue;
            }

            total_reveal_reward +=
                if let Some(dr_output) = data_request_pool.get_dr_output(&re_tx.body.dr_pointer) {
                    dr_output.commit_and_reveal_fee
                } else {
                    0
                };
        }
        // Add reveal reward
        if total_reveal_reward > 0 {
            log::debug!(
                "Rewarding {} with {} wit for including reveals",
                miner_pkh,
                Wit::from(total_reveal_reward),
            );
            stakes
                .add_reward(miner_pkh, Wit::from(total_reveal_reward), block_epoch)
                .unwrap();
        }

        for ta_tx in &block.txns.tally_txns {
            if data_request_is_pre_wit2(data_request_pool, &Transaction::Tally(ta_tx.clone())) {
                continue;
            }

            let (collateral, reward) =
                if let Some(dr_output) = data_request_pool.get_dr_output(&ta_tx.dr_pointer) {
                    (dr_output.collateral, dr_output.witness_reward)
                } else {
                    (0, 0)
                };
            let commits: Vec<_> = data_request_pool.data_request_pool[&ta_tx.dr_pointer]
                .info
                .commits
                .keys()
                .cloned()
                .collect();

            // Reward honest validators
            let honest_pkhs: Vec<PublicKeyHash> = commits
                .iter()
                .filter(|pkh| {
                    !ta_tx.error_committers.contains(pkh) && !ta_tx.out_of_consensus.contains(pkh)
                })
                .cloned()
                .collect();
            for honest_pkh in honest_pkhs {
                log::debug!(
                    "Refunding and rewarding {} for {} wit for solving data request {}",
                    honest_pkh,
                    collateral + reward,
                    ta_tx.dr_pointer
                );
                stakes
                    .add_reward(honest_pkh, Wit::from(collateral + reward), block_epoch)
                    .unwrap();
            }

            // Refund errored validators
            for error_pkh in &ta_tx.error_committers {
                log::debug!(
                    "Refunding {} with {} wit because it revealed an error for data request {}",
                    error_pkh,
                    collateral,
                    ta_tx.dr_pointer
                );
                stakes
                    .add_reward(*error_pkh, Wit::from(collateral), block_epoch)
                    .unwrap();
            }
        }

        // Add all transaction fees plus the block reward
        let mut transaction_fees = 0;
        for vt_tx in &block.txns.value_transfer_txns {
            transaction_fees += vt_transaction_fee(vt_tx, utxo_diff, block_epoch, epoch_constants)
                .unwrap_or_default();
        }
        for dr_tx in &block.txns.data_request_txns {
            transaction_fees += dr_transaction_fee(dr_tx, utxo_diff, block_epoch, epoch_constants)
                .unwrap_or_default();
        }
        for st_tx in &block.txns.stake_txns {
            transaction_fees += st_transaction_fee(st_tx, utxo_diff, block_epoch, epoch_constants)
                .unwrap_or_default();
        }
        for ut_tx in &block.txns.unstake_txns {
            transaction_fees += ut_tx.body.fee;
        }

        let block_reward = consensus_constants_wit2.get_validator_block_reward(block_epoch);

        log::debug!(
            "Rewarding {} with {} wit transaction fees and {} wit block reward for proposing a block",
            miner_pkh,
            Wit::from(transaction_fees),
            Wit::from(block_reward),
        );
        stakes
            .add_reward(
                miner_pkh,
                Wit::from(transaction_fees) + Wit::from(block_reward),
                block_epoch,
            )
            .unwrap();

        // IMPORTANT: Always perform age resets after adding rewards

        // Reset mining power to block proposer, and higher ranked eligible candidates:
        log::debug!(
            "Resetting mining age for {} to {}",
            miner_pkh,
            block_epoch + 1
        );
        stakes.reset_mining_age(miner_pkh, block_epoch).unwrap();

        // Reset witnessing power
        for co_tx in &block.txns.commit_txns {
            if data_request_is_pre_wit2(data_request_pool, &Transaction::Commit(co_tx.clone())) {
                continue;
            }

            let commit_pkh = co_tx.body.proof.proof.pkh();
            log::debug!(
                "Resetting witnessing age for {} to {}",
                commit_pkh,
                block_epoch + 1,
            );
            stakes
                .reset_age(commit_pkh, Capability::Witnessing, block_epoch + 1, 1)
                .unwrap();
        }

        // Slash lieing validators
        // Collateral was already reserved, so not returning it results in losing it
        // Reset the age for witnessing power to 10 epochs in the future
        for ta_tx in &block.txns.tally_txns {
            if data_request_is_pre_wit2(data_request_pool, &Transaction::Tally(ta_tx.clone())) {
                continue;
            }

            let liar_pkhs: Vec<PublicKeyHash> = ta_tx
                .out_of_consensus
                .iter()
                .filter(|&pkh| !ta_tx.error_committers.contains(pkh))
                .cloned()
                .collect();
            for liar_pkh in &liar_pkhs {
                log::debug!(
                    "Slashing {} because it revealed a lie for data request {}",
                    liar_pkh,
                    ta_tx.dr_pointer,
                );
                stakes
                    .reset_age(*liar_pkh, Capability::Witnessing, block_epoch + 10, 1)
                    .unwrap();
            }
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
    own_pkh: PublicKeyHash,
    own_utxos: &mut OwnUnspentOutputsPool,
    node_stats: &mut NodeStats,
    state_machine: StateMachine,
) -> ReputationInfo {
    let mut rep_info = ReputationInfo::new();
    let epoch = block.block_header.beacon.checkpoint;
    let protocol = ProtocolVersion::from_epoch(epoch);
    let block_hash = block.versioned_hash(protocol);

    let mut data_requests_with_too_many_witnesses = HashSet::<Hash>::new();
    for ta_tx in &block.txns.tally_txns {
        // Track data requests which were already processed with a TooManyWitnesses error
        if RadonTypes::try_from(ta_tx.tally.as_slice())
            == Ok(RadonTypes::RadonError(
                RadonError::try_from(TooManyWitnesses).unwrap(),
            ))
        {
            data_requests_with_too_many_witnesses.insert(ta_tx.dr_pointer);

            if let Some(dr_state) = data_request_pool.data_request_state_mutable(&ta_tx.dr_pointer)
            {
                if dr_state.stage != DataRequestStage::TALLY {
                    dr_state.update_stage(0, true);
                }
            }
        }

        if ProtocolVersion::from_epoch(epoch) < ProtocolVersion::V2_0 {
            // Process tally transactions: used to update reputation engine
            rep_info.update(ta_tx, data_request_pool, own_pkh, node_stats);
        }

        // IMPORTANT: Update the data request pool after updating reputation info
        if let Err(e) = data_request_pool.process_tally(ta_tx, &block_hash) {
            log::error!("Error processing tally transaction:\n{e}");
        }

        transactions_pool.clear_reveals_from_finished_dr(&ta_tx.dr_pointer);
    }

    for vt_tx in &block.txns.value_transfer_txns {
        transactions_pool.vt_remove(vt_tx);
    }

    for dr_tx in &block.txns.data_request_txns {
        let dr_hash = dr_tx.versioned_hash(protocol);
        if data_requests_with_too_many_witnesses.contains(&dr_hash) {
            log::debug!(
                "Skipping data request {dr_hash} as it was already processed with a TooManyWitnesses error"
            );
            transactions_pool.dr_remove(dr_tx);
            continue;
        }
        if let Err(e) = data_request_pool.process_data_request(
            dr_tx,
            block.block_header.beacon.checkpoint,
            Some(block_hash),
        ) {
            log::error!("Error processing data request transaction:\n{e}");
        } else {
            transactions_pool.dr_remove(dr_tx);
        }
    }

    for co_tx in &block.txns.commit_txns {
        if let Err(e) = data_request_pool.process_commit(co_tx, epoch, &block_hash) {
            log::error!("Error processing commit transaction:\n{e}");
        } else {
            if co_tx.body.proof.proof.pkh() == own_pkh {
                node_stats.commits_count += 1;
                if state_machine != StateMachine::Synced {
                    // During synchronization, we assume that every consolidated commit had,
                    // at least, one data requests valid proof and one commit proposed
                    node_stats.dr_eligibility_count += 1;
                    node_stats.commits_proposed_count += 1;
                }
            }
            transactions_pool.remove_inputs(&co_tx.body.collateral);
        }
    }

    for re_tx in &block.txns.reveal_txns {
        if let Err(e) = data_request_pool.process_reveal(re_tx, epoch, &block_hash) {
            log::error!("Error processing reveal transaction:\n{e}");
        }
        let re_hash = re_tx.versioned_hash(protocol);
        transactions_pool.remove_one_reveal(&re_tx.body.dr_pointer, &re_tx.body.pkh, &re_hash);
    }

    for st_tx in &block.txns.stake_txns {
        transactions_pool.st_remove(st_tx);
    }

    for ut_tx in &block.txns.unstake_txns {
        transactions_pool.ut_remove(ut_tx);
    }

    // Update own_utxos
    utxo_diff.visit(
        own_utxos,
        |own_utxos, output_pointer, output| {
            // Insert new outputs
            if output.pkh == own_pkh {
                own_utxos.insert(*output_pointer, 0);
            }
        },
        |own_utxos, output_pointer| {
            // Remove spent inputs
            own_utxos.remove(output_pointer);
        },
    );

    utxo_diff.apply(unspent_outputs_pool);

    rep_info
}

fn data_request_is_pre_wit2(
    data_request_pool: &DataRequestPool,
    transaction: &Transaction,
) -> bool {
    // Data requests which were included in a block prior to the activation of wit/2 do
    // not need to be processed using the wit/2 business logic.
    match transaction {
        Transaction::Commit(co_tx) => {
            if let Some(dr_state) = data_request_pool.data_request_state(&co_tx.body.dr_pointer) {
                if get_protocol_version(Some(dr_state.epoch)) < ProtocolVersion::V2_0 {
                    return true;
                }
            }
        }
        Transaction::Reveal(re_tx) => {
            if let Some(dr_state) = data_request_pool.data_request_state(&re_tx.body.dr_pointer) {
                if get_protocol_version(Some(dr_state.epoch)) < ProtocolVersion::V2_0 {
                    return true;
                }
            }
        }
        Transaction::Tally(ta_tx) => {
            if let Some(dr_state) = data_request_pool.data_request_state(&ta_tx.dr_pointer) {
                if get_protocol_version(Some(dr_state.epoch)) < ProtocolVersion::V2_0 {
                    return true;
                }
            }
        }
        _ => (),
    }

    false
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
    let old_alpha = rep_eng.current_alpha();
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
    let revealers = result_count.into_keys();
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
            log::info!("Your reputation score has been slashed by {slashed_rep} points");
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
                log::info!("Your reputation score has increased by {rep_reward} points");
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
        log::error!("Error updating reputation in consolidation: {e}");
    }

    // Retain identities that exist in the ARS
    secp_bls_mapping.retain(|k| rep_eng.is_ars_member(k));

    rep_eng.set_current_alpha(new_alpha);
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
            if v.stage == DataRequestStage::COMMIT || v.stage == DataRequestStage::REVEAL {
                let current_round = if v.stage == DataRequestStage::COMMIT {
                    v.info.current_commit_round
                } else {
                    v.info.current_reveal_round
                };
                format!(
                    "{}\n\t* {} Stage: {} ({}/{}), Commits: {}, Reveals: {}",
                    acc,
                    White.bold().paint(k.to_string()),
                    White.bold().paint(format!("{:?}", v.stage)),
                    current_round,
                    data_request_pool.extra_rounds + 1,
                    v.info.commits.len(),
                    v.info.reveals.len()
                )
            } else {
                format!(
                    "{}\n\t* {} Stage: {}, Commits: {}, Reveals: {}",
                    acc,
                    White.bold().paint(k.to_string()),
                    White.bold().paint(format!("{:?}", v.stage)),
                    v.info.commits.len(),
                    v.info.reveals.len()
                )
            }
        });

    // Generate a string enumerating the TAPI bits that are enabled in this block's header
    let tapi = if block.block_header.signals == 0 {
        String::from("No TAPI signaling bits")
    } else {
        let signals = (0..32)
            .filter_map(|i| {
                let mask = 1 << i;
                if block.block_header.signals & mask != 0 {
                    Some(i.to_string())
                } else {
                    None
                }
            })
            .collect::<Vec<_>>()
            .join(",");

        format!(
            "{}: {}",
            Purple.bold().paint("TAPI signaling bits"),
            signals
        )
    };

    log::debug!("{tapi}");

    if info.is_empty() {
        log::info!(
            "{} Block {} consolidated for epoch #{} {}",
            Purple.bold().paint("[Chain]"),
            Purple.bold().paint(block_hash.to_string()),
            Purple.bold().paint(block_epoch.to_string()),
            White.paint("with no data requests".to_string())
        );
    } else {
        log::info!(
            "{} Block {} consolidated for epoch #{} {}\n{}",
            Purple.bold().paint("[Chain]"),
            Purple.bold().paint(block_hash.to_string()),
            Purple.bold().paint(block_epoch.to_string()),
            White.bold().paint("Data Requests: "),
            White.bold().paint(info)
        );
    }
}

fn show_sync_progress(
    beacon: CheckpointBeacon,
    sync_target: &SyncTarget,
    epoch_constants: EpochConstants,
    current_epoch: u32,
) {
    let target_checkpoint = sync_target.block.checkpoint;
    // Show progress log
    let mut percent_done_float =
        f64::from(beacon.checkpoint) / f64::from(target_checkpoint) * 100.0;

    // Never show 100% unless it's actually done
    if beacon.checkpoint != target_checkpoint && percent_done_float > 99.99 {
        percent_done_float = 99.99;
    }
    let percent_done_string = format!("{percent_done_float:.2}%");

    // Block age is actually the difference in age: it assumes that the last
    // block is 0 seconds old
    let block_age = current_epoch.saturating_sub(beacon.checkpoint)
        * u32::from(epoch_constants.checkpoints_period);

    let human_age = seconds_to_human_string(u64::from(block_age));
    log::info!(
        "Synchronization progress: {} ({:>6}/{:>6}). Latest synced block is {} old.",
        percent_done_string,
        beacon.checkpoint,
        target_checkpoint,
        human_age
    );
}

fn last_superblock_signed_by_bootstrap(consensus_constants: &ConsensusConstants) -> u32 {
    // This needs to use the old value of collateral_age, because it is calculating a superblock
    // index from the early days of the chain
    (consensus_constants.collateral_age + consensus_constants.activity_period)
        / u32::from(consensus_constants.superblock_period)
}
// Returns the committee size to be applied given the default committee size, decreasing period
// and  step, last consolidated epoch and the current checkpoint
#[allow(clippy::too_many_arguments)]
fn current_committee_size_requirement(
    default_committee_size: u32,
    last_committee_size: u32,
    decreasing_period: u32,
    decreasing_step: u32,
    last_consolidated_checkpoint: u32,
    current_checkpoint: u32,
    last_checkpoint_signed_by_bootstrap: u32,
    min_committee_size: u32,
) -> u32 {
    assert!(
        last_consolidated_checkpoint <= current_checkpoint,
        "Something went wrong as the last consolidated checkpoint is bigger than our current checkpoint {last_consolidated_checkpoint} > {current_checkpoint}"
    );
    // If the last consolidated superblock or the current checkpoint is below last_checkpoint_signed_by_bootstrap, return the default committee size
    if last_consolidated_checkpoint <= last_checkpoint_signed_by_bootstrap {
        default_committee_size
    } else if current_checkpoint - last_consolidated_checkpoint >= decreasing_period {
        // Decrease committee size. The minimum committee size must be at least 1.
        let min_committee_size = max(min_committee_size, 1);
        // Calculate the difference between the last consolidated superblock checkpoint and the current one
        // If this difference exceeds the decreasing_period, reduce the committee size by decreasing_step * difference
        max(
            last_committee_size.saturating_sub(
                (current_checkpoint.saturating_sub(last_consolidated_checkpoint)
                    / decreasing_period)
                    * decreasing_step,
            ),
            min_committee_size,
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

/// When the TransactionsPool is full, inserting a transaction can result in removing other
/// transactions. This will log the removed transactions.
pub fn log_removed_transactions(removed_transactions: &[Transaction], inserted_tx_hash: Hash) {
    if removed_transactions.is_empty() {
        log::trace!("Transaction {inserted_tx_hash} added successfully");
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
            log::trace!("Transaction {inserted_tx_hash} was not added because the fee was too low");
        } else {
            log::trace!("Transaction {inserted_tx_hash} added successfully");
        }

        log::debug!("Removed the following transactions: {removed_tx_hashes:?}");
    }
}

/// Run data request locally
pub fn run_dr_locally(dr: &DataRequestOutput) -> Result<RadonTypes, anyhow::Error> {
    // Validate RADON: if the dr cannot be included in a witnet block, this should fail.
    // This does not validate other data request parameters such as number of witnesses, weight, or
    // collateral, so it is still possible that this request is considered invalid by miners.
    let active_wips = current_active_wips();
    validate_rad_request(&dr.data_request, &active_wips)?;

    // TODO: remove blocking calls, this code is no longer part of the CLI
    // Block on data request retrieval because the CLI application blocks everywhere anyway
    let run_retrieval_blocking = |retrieve| {
        futures::executor::block_on(witnet_rad::run_retrieval(retrieve, active_wips.clone()))
    };

    let mut retrieval_results = vec![];
    for r in &dr.data_request.retrieve {
        log::info!("Running retrieval for {}", r.url);
        retrieval_results.push(run_retrieval_blocking(r)?);
    }

    log::info!("Running aggregation with values {retrieval_results:?}");
    let aggregation_result = witnet_rad::run_aggregation(
        retrieval_results,
        dr.data_request.aggregate.clone(),
        &active_wips,
    )?;
    log::info!("Aggregation result: {aggregation_result:?}");

    // Assume that all the required witnesses will report the same value
    let reported_values: Result<Vec<RadonTypes>, _> = vec![aggregation_result; dr.witnesses.into()]
        .into_iter()
        .map(RadonTypes::try_from)
        .collect();
    log::info!("Running tally with values {reported_values:?}");
    let tally_result =
        witnet_rad::run_tally(reported_values?, &dr.data_request.tally, &active_wips)?;
    log::info!("Tally result: {tally_result:?}");

    Ok(tally_result)
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use witnet_config::{
        config::{Config, StorageBackend, consensus_constants_from_partial},
        defaults::Testnet,
    };
    use witnet_crypto::{
        secp256k1::{
            PublicKey as Secp256k1_PublicKey, Secp256k1, SecretKey as Secp256k1_SecretKey,
        },
        signature::sign,
    };
    use witnet_data_structures::{
        chain::{
            BlockMerkleRoots, BlockTransactions, ChainInfo, DataRequestState, Environment, Input,
            KeyedSignature, OutputPointer, PartialConsensusConstants, PublicKey, SecretKey,
            Signature, StakeOutput, ValueTransferOutput,
        },
        proto::versioning::VersionedHashable,
        transaction::{
            CommitTransaction, DRTransaction, MintTransaction, RevealTransaction, StakeTransaction,
            StakeTransactionBody, UnstakeTransaction, UnstakeTransactionBody, VTTransaction,
            VTTransactionBody,
        },
        vrf::BlockEligibilityClaim,
    };
    use witnet_protected::Protected;
    use witnet_validations::validations::block_reward;

    use crate::{
        actors::chain_manager::mining::build_block,
        config_mngr,
        utils::{ActorFutureToNormalFuture, test_actix_system},
    };

    use super::*;

    #[test]
    fn test_rep_info_update() {
        let mut rep_info = ReputationInfo::default();
        let mut dr_pool = DataRequestPool::default();
        let epoch = 0;

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

        dr_pool.add_data_request(1, dr_tx, None).unwrap();
        dr_pool
            .process_commit(&co_tx, epoch, &Hash::default())
            .unwrap();
        dr_pool
            .process_commit(&co_tx2, epoch, &Hash::default())
            .unwrap();
        dr_pool
            .process_commit(&co_tx3, epoch, &Hash::default())
            .unwrap();
        dr_pool.update_data_request_stages(None, None);
        dr_pool
            .process_reveal(&re_tx1, epoch, &Hash::default())
            .unwrap();
        dr_pool
            .process_reveal(&re_tx2, epoch, &Hash::default())
            .unwrap();

        rep_info.update(
            &ta_tx,
            &dr_pool,
            PublicKeyHash::default(),
            &mut NodeStats::default(),
        );

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
        test_actix_system(|| async {
            // Setup testing: use in-memory database instead of rocksdb
            let mut config = Config::default();
            config.storage.backend = StorageBackend::HashMap;
            let config = Arc::new(config);
            // Start relevant actors
            config_mngr::start(config);
            storage_mngr::start();

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
        });
    }

    #[test]
    fn test_current_committee_size_requirement() {
        let mut size = current_committee_size_requirement(5, 5, 4, 1, 1, 2, 0, 1);

        assert_eq!(size, 5);

        size = current_committee_size_requirement(5, 5, 4, 1, 0, 301, 1, 1);

        assert_eq!(size, 5);

        size = current_committee_size_requirement(5, 5, 4, 1, 3, 4, 0, 1);

        assert_eq!(size, 5);

        size = current_committee_size_requirement(5, 5, 4, 1, 3, 7, 0, 1);

        assert_eq!(size, 4);

        size = current_committee_size_requirement(5, 5, 4, 1, 3, 12, 0, 1);

        assert_eq!(size, 3);

        size = current_committee_size_requirement(5, 5, 4, 1, 3, 200, 0, 1);

        assert_eq!(size, 1);

        size = current_committee_size_requirement(100, 100, 5, 5, 5, 50, 0, 1);

        assert_eq!(size, 55);

        size = current_committee_size_requirement(100, 55, 5, 5, 5, 6, 0, 1);

        assert_eq!(size, 60);

        size = current_committee_size_requirement(100, 98, 5, 5, 5, 6, 0, 1);

        assert_eq!(size, 100);

        size = current_committee_size_requirement(100, 100, 5, 5, 5, 6, 0, 1);

        assert_eq!(size, 100);

        size = current_committee_size_requirement(100, 3, 5, 5, 8, 10, 9, 1);

        assert_eq!(size, 100);

        size = current_committee_size_requirement(100, 3, 5, 5, 9, 10, 9, 1);

        assert_eq!(size, 100);
    }

    #[test]
    #[should_panic(
        expected = "Something went wrong as the last consolidated checkpoint is bigger than our current checkpoint 2 > 1"
    )]
    fn test_wrong_checkpoints() {
        current_committee_size_requirement(5, 5, 4, 1, 2, 1, 0, 1);
    }

    #[test]
    fn test_current_committee_size_requirement_sequence() {
        let default_size = 100;
        let decreasing_period = 5;
        let decreasing_step = 2;
        let mut last_consolidated_checkpoint = 0;
        let last_checkpoint_signed_by_bootstrap = 0;
        let mut current_checkpoint = 0;
        let mut size = 0;

        let mut next_size = |has_superblock| {
            let s = current_committee_size_requirement(
                default_size,
                size,
                decreasing_period,
                decreasing_step,
                last_consolidated_checkpoint,
                current_checkpoint,
                last_checkpoint_signed_by_bootstrap,
                1,
            );
            if has_superblock {
                last_consolidated_checkpoint = current_checkpoint;
                size = s;
            };
            current_checkpoint += 1;
            s
        };

        // Check that the committee size is 100 during the first epochs if there are superblocks
        let mut initial = vec![];
        for _ in 0..10 {
            initial.push(next_size(true));
        }
        assert_eq!(initial, vec![100; 10]);

        // Check the decreasing pattern from 100 with period 5 and step 2
        let mut decreasing = vec![];
        for _ in 0..30 {
            decreasing.push(next_size(false));
        }
        assert_eq!(
            decreasing,
            vec![
                100, 100, 100, 100, 98, 98, 98, 98, 98, 96, 96, 96, 96, 96, 94, 94, 94, 94, 94, 92,
                92, 92, 92, 92, 90, 90, 90, 90, 90, 88
            ]
        );

        // Set the current committee size back to 1
        for i in 0.. {
            if next_size(false) == 1 {
                break;
            }

            if i == 1000 {
                panic!("Never reached commitee size 1");
            }
        }

        // Check the increasing pattern from 1 to 100 with step 2
        let mut increasing = vec![];
        for _ in 0..55 {
            increasing.push(next_size(true));
        }
        assert_eq!(
            increasing,
            vec![
                1, 3, 5, 7, 9, 11, 13, 15, 17, 19, 21, 23, 25, 27, 29, 31, 33, 35, 37, 39, 41, 43,
                45, 47, 49, 51, 53, 55, 57, 59, 61, 63, 65, 67, 69, 71, 73, 75, 77, 79, 81, 83, 85,
                87, 89, 91, 93, 95, 97, 99, 100, 100, 100, 100, 100
            ]
        );

        // Set the current committee size back to 1
        for i in 0.. {
            if next_size(false) == 1 {
                break;
            }

            if i == 1000 {
                panic!("Never reached commitee size 1");
            }
        }

        // Check the sequence when there is 1 superblock and 5 missing in circle
        let mut circular_1 = vec![];
        for _ in 0..3 {
            circular_1.push(next_size(true));
            for _ in 0..decreasing_period {
                circular_1.push(next_size(false));
            }
        }
        assert_eq!(
            circular_1,
            vec![1, 3, 3, 3, 3, 1, 1, 3, 3, 3, 3, 1, 1, 3, 3, 3, 3, 1]
        );

        // Check the sequence when there is 1 missing superblock and 5 superblocks in circle
        let mut circular_2 = vec![];
        for _ in 0..3 {
            circular_2.push(next_size(false));
            for _ in 0..decreasing_period {
                circular_2.push(next_size(true));
            }
        }
        assert_eq!(
            circular_2,
            vec![
                1, 1, 3, 5, 7, 9, 11, 11, 13, 15, 17, 19, 21, 21, 23, 25, 27, 29
            ]
        );

        // Set the current committee size back to 1
        for i in 0.. {
            if next_size(false) == 1 {
                break;
            }

            if i == 1000 {
                panic!("Never reached commitee size 1");
            }
        }

        // Check the sequence when there is 1 missing superblock and 3 superblocks in circle
        let mut circular_3 = vec![];
        for _ in 0..3 {
            circular_3.push(next_size(false));
            for _ in 2..decreasing_period {
                circular_3.push(next_size(true));
            }
        }
        assert_eq!(circular_3, vec![1, 1, 3, 5, 7, 7, 9, 11, 13, 13, 15, 17]);

        // Set the current committee size back to 1
        for i in 0.. {
            if next_size(false) == 1 {
                break;
            }

            if i == 1000 {
                panic!("Never reached commitee size 1");
            }
        }

        // Check the sequence when there is 1 superblock, 6 superblocks missing and then 2 superblocks
        let mut sequence = vec![];
        for _ in 0..3 {
            sequence.push(next_size(true));
        }
        for _ in 3..9 {
            sequence.push(next_size(false));
        }

        for _ in 9..11 {
            sequence.push(next_size(true));
        }
        assert_eq!(sequence, vec![1, 3, 5, 7, 7, 7, 7, 3, 3, 3, 5]);
    }

    #[test]
    fn test_current_committee_size_requirement_sequence_abrupt_change() {
        let default_size = 100;
        let decreasing_period = 5;
        let decreasing_step = 2;
        let mut last_consolidated_checkpoint = 0;
        let last_checkpoint_signed_by_bootstrap = 20;
        let mut current_checkpoint = 0;
        let mut size = 0;

        let mut next_size = |has_superblock| {
            let s = current_committee_size_requirement(
                default_size,
                size,
                decreasing_period,
                decreasing_step,
                last_consolidated_checkpoint,
                current_checkpoint,
                last_checkpoint_signed_by_bootstrap,
                1,
            );
            if has_superblock {
                last_consolidated_checkpoint = current_checkpoint;
                size = s;
            };
            current_checkpoint += 1;
            s
        };

        // Check that the committee size is 100 during the first 19 epochs if there are superblocks
        let mut initial = vec![];
        for _ in 0..19 {
            initial.push(next_size(true));
        }
        assert_eq!(initial, vec![100; 19]);

        // Check that, as long as there is no further consolidared superblock after last_checkpoint_signed_by_bootstrap, the committee size is 10
        let mut idle = vec![];
        for _ in 0..20 {
            idle.push(next_size(false));
        }
        assert_eq!(idle, vec![100; 20]);

        // Check that, as long as there is one consolidated_superblock after last_checkpoint_signed_by_bootstrap, the comittee decreases

        let mut decreasing = vec![next_size(true)];
        for _ in 0..30 {
            decreasing.push(next_size(false));
        }
        assert_eq!(
            decreasing,
            vec![
                100, 100, 100, 100, 100, 98, 98, 98, 98, 98, 96, 96, 96, 96, 96, 94, 94, 94, 94,
                94, 92, 92, 92, 92, 92, 90, 90, 90, 90, 90, 88
            ]
        );
    }

    static PRIV_KEY_1: [u8; 32] = [0xcd; 32];
    static PRIV_KEY_2: [u8; 32] = [0x43; 32];
    static PRIV_KEY_3: [u8; 32] = [0x57; 32];
    static PRIV_KEY_4: [u8; 32] = [0xa5; 32];

    fn sign_tx<H: VersionedHashable>(
        mk: [u8; 32],
        tx: &H,
        protocol_version: ProtocolVersion,
    ) -> KeyedSignature {
        let Hash::SHA256(data) = tx.versioned_hash(protocol_version);

        let secret_key =
            Secp256k1_SecretKey::from_slice(&mk).expect("32 bytes, within curve order");
        let public_key = Secp256k1_PublicKey::from_secret_key_global(&secret_key);
        let public_key = PublicKey::from(public_key);

        let signature = sign(secret_key, &data).unwrap();

        KeyedSignature {
            signature: Signature::from(signature),
            public_key,
        }
    }

    fn pkh(mk: &[u8; 32]) -> PublicKeyHash {
        let secp = &Secp256k1::new();
        let secret_key = Secp256k1_SecretKey::from_slice(mk).expect("32 bytes, within curve order");
        let public_key = Secp256k1_PublicKey::from_secret_key(secp, &secret_key);
        let public_key = PublicKey::from(public_key);

        public_key.pkh()
    }

    fn create_valid_block(chain_manager: &mut ChainManager, priv_key: &[u8; 32]) -> Block {
        let vrf = &mut VrfCtx::secp256k1().unwrap();
        let current_epoch = chain_manager.current_epoch.unwrap();
        let protocol_version = ProtocolVersion::from_epoch(current_epoch);

        let consensus_constants = chain_manager.consensus_constants();
        let secret_key = SecretKey {
            bytes: Protected::from(priv_key.to_vec()),
        };
        let last_block_hash = chain_manager
            .chain_state
            .chain_info
            .as_ref()
            .unwrap()
            .highest_block_checkpoint
            .hash_prev_block;
        let last_vrf_input = chain_manager
            .chain_state
            .chain_info
            .as_ref()
            .unwrap()
            .highest_vrf_output
            .hash_prev_vrf;
        let block_beacon = CheckpointBeacon {
            checkpoint: current_epoch,
            hash_prev_block: last_block_hash,
        };

        let vrf_input = CheckpointVRF {
            checkpoint: current_epoch,
            hash_prev_vrf: last_vrf_input,
        };

        let my_pkh = PublicKeyHash::default();

        let txns = BlockTransactions {
            mint: MintTransaction::new(
                current_epoch,
                vec![ValueTransferOutput {
                    time_lock: 0,
                    pkh: my_pkh,
                    value: block_reward(
                        current_epoch,
                        consensus_constants.initial_block_reward,
                        consensus_constants.halving_period,
                    ),
                }],
            ),
            ..BlockTransactions::default()
        };

        let block_header = BlockHeader {
            merkle_roots: BlockMerkleRoots::from_transactions(&txns, protocol_version),
            beacon: block_beacon,
            proof: BlockEligibilityClaim::create(vrf, &secret_key, vrf_input).unwrap(),
            ..Default::default()
        };
        let block_sig = sign_tx(*priv_key, &block_header, protocol_version);

        Block::new(block_header, block_sig, txns)
    }

    // TODO: cannot use struct update syntax with ChainManager because it implements the
    // Drop trait, but clippy seems to miss that?
    #[ignore]
    #[allow(clippy::field_reassign_with_default)]
    #[test]
    fn test_process_candidate_malleability() {
        let _ = env_logger::builder().is_test(true).try_init();
        test_actix_system(|| async {
            // Setup testing: use in-memory database instead of rocksdb
            let mut config = Config::default();
            config.storage.backend = StorageBackend::HashMap;
            let config = Arc::new(config);
            // Start relevant actors
            config_mngr::start(config);
            storage_mngr::start();

            let mut chain_manager = ChainManager::default();

            chain_manager.current_epoch = Some(2000000);
            // 1 epoch = 1000 seconds, for easy testing
            chain_manager.epoch_constants = Some(EpochConstants {
                checkpoint_zero_timestamp: 0,
                checkpoints_period: 1_000,
                checkpoint_zero_timestamp_wit2: i64::MAX,
                checkpoints_period_wit2: 1,
            });
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
            chain_manager.chain_state.reputation_engine = Some(ReputationEngine::new(1000));
            chain_manager.vrf_ctx = Some(VrfCtx::secp256k1().unwrap());
            chain_manager.sm_state = StateMachine::Synced;

            let block_1 = create_valid_block(&mut chain_manager, &PRIV_KEY_2);
            let block_2 = create_valid_block(&mut chain_manager, &PRIV_KEY_1);

            // block_1 should be better candidate than block_2
            let vrf_ctx = &mut VrfCtx::secp256k1().unwrap();
            let vrf_hash_1 = block_1
                .block_header
                .proof
                .proof
                .proof_to_hash(vrf_ctx)
                .unwrap();
            let vrf_hash_2 = block_2
                .block_header
                .proof
                .proof
                .proof_to_hash(vrf_ctx)
                .unwrap();
            assert_eq!(
                compare_block_candidates(
                    block_1.hash(),
                    Reputation(0),
                    vrf_hash_1,
                    false,
                    Default::default(),
                    block_2.hash(),
                    Reputation(0),
                    vrf_hash_2,
                    false,
                    Default::default(),
                    &VrfSlots::new(vec![Hash::default()]),
                    ProtocolVersion::V1_7,
                ),
                Ordering::Greater
            );

            let mut block_mal_1 = block_1.clone();
            // Malleability!
            block_mal_1.txns.mint.outputs.clear();
            // Changing block txns field does not change block hash
            assert_eq!(block_1.hash(), block_mal_1.hash());
            // But the blocks are different
            assert_ne!(block_1, block_mal_1);

            // Process the modified candidate first
            chain_manager.process_candidate(block_mal_1);
            // The best candidate should be None because this block is invalid
            let best_cand = chain_manager.best_candidate.as_ref().map(|bc| &bc.block);
            assert_eq!(best_cand, None);

            // Process candidate with the same hash, but this one is valid
            chain_manager.process_candidate(block_1.clone());
            // The best candidate should be block_1
            let best_cand = chain_manager.best_candidate.as_ref().map(|bc| &bc.block);
            assert_eq!(best_cand, Some(&block_1));

            // Process another valid candidate, but worse than the other one
            chain_manager.process_candidate(block_2);
            // The best candidate should still be block_1
            let best_cand = chain_manager.best_candidate.as_ref().map(|bc| &bc.block);
            assert_eq!(best_cand, Some(&block_1));
        });
    }

    fn create_valid_transaction(
        _chain_manager: &mut ChainManager,
        priv_key: &[u8; 32],
        protocol_version: ProtocolVersion,
    ) -> Transaction {
        let my_pkh = pkh(priv_key);

        let vti = Input::new(OutputPointer {
            transaction_id: "0000000000000000000000000000000000000000000000000000000000000001"
                .parse()
                .unwrap(),
            output_index: 0,
        });
        let vto = ValueTransferOutput {
            time_lock: 0,
            pkh: my_pkh,
            value: 1000,
        };

        let inputs = vec![vti];
        let outputs = vec![vto];

        let vt_body = VTTransactionBody::new(inputs, outputs);
        let signatures = vec![sign_tx(*priv_key, &vt_body, protocol_version)];
        let vtt = VTTransaction::new(vt_body, signatures);

        Transaction::ValueTransfer(vtt)
    }

    #[test]
    fn test_add_transaction_malleability() {
        let _ = env_logger::builder().is_test(true).try_init();
        test_actix_system(|| async {
            // Setup testing: use in-memory database instead of rocksdb
            let mut config = Config::default();
            config.storage.backend = StorageBackend::HashMap;
            let config = Arc::new(config);
            let epoch = Some(2000000);
            let protocol_version = get_protocol_version(epoch);
            // Start relevant actors
            config_mngr::start(config);
            storage_mngr::start();

            let mut ctx = Context::new();
            let mut chain_manager = ChainManager::default();

            chain_manager.current_epoch = epoch;
            // 1 epoch = 1000 seconds, for easy testing
            chain_manager.epoch_constants = Some(EpochConstants {
                checkpoint_zero_timestamp: 0,
                checkpoints_period: 1_000,
                checkpoint_zero_timestamp_wit2: i64::MAX,
                checkpoints_period_wit2: 1,
            });
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
            let out_ptr = OutputPointer {
                transaction_id: "0000000000000000000000000000000000000000000000000000000000000001"
                    .parse()
                    .unwrap(),
                output_index: 0,
            };
            let pkh_1 = pkh(&PRIV_KEY_1);
            let vto1 = ValueTransferOutput {
                pkh: pkh_1,
                value: 10000,
                time_lock: 0,
            };
            chain_manager
                .chain_state
                .unspent_outputs_pool
                .insert(out_ptr, vto1, 0);
            chain_manager.chain_state.reputation_engine = Some(ReputationEngine::new(1000));
            chain_manager.vrf_ctx = Some(VrfCtx::secp256k1().unwrap());
            chain_manager.sm_state = StateMachine::Synced;

            let t1 = create_valid_transaction(&mut chain_manager, &PRIV_KEY_1, protocol_version);
            let mut t1_mal = t1.clone();
            // Malleability!
            match &mut t1_mal {
                Transaction::ValueTransfer(vtt) => {
                    // Invalidate signature
                    match &mut vtt.signatures[0].signature {
                        Signature::Secp256k1(secp_sig) => {
                            // Flip 1 bit
                            secp_sig.der[10] ^= 0x01;
                        }
                    }
                }
                _ => {
                    panic!(
                        "Expected `create_valid_transaction` to return value transfer transaction"
                    );
                }
            }

            // Changing signatures field does not change transaction hash
            assert_eq!(
                t1.versioned_hash(protocol_version),
                t1_mal.versioned_hash(protocol_version)
            );
            // But the transactions are different
            assert_ne!(t1, t1_mal);

            let now = 0;
            // Process the modified transaction first
            let fut = chain_manager.add_transaction(
                AddTransaction {
                    transaction: t1_mal,
                    broadcast_flag: false,
                },
                now,
            );
            let res = fut.into_normal_future(&mut chain_manager, &mut ctx).await;
            // Invalid signature
            assert!(res.is_err());
            // Transaction is not added to the pool
            assert_eq!(chain_manager.transactions_pool.vt_len(), 0);

            // Process original transaction
            let fut = chain_manager.add_transaction(
                AddTransaction {
                    transaction: t1,
                    broadcast_flag: false,
                },
                now,
            );
            let res = fut.into_normal_future(&mut chain_manager, &mut ctx).await;
            // Transaction is valid
            assert!(res.is_ok(), "{res:?}");
            // Transaction is added to the pool
            assert_eq!(chain_manager.transactions_pool.vt_len(), 1);
        });
    }

    #[test]
    fn test_stakes_tracker_snapshot() {
        let mut stakes = StakesTracker::default();

        let pkh_1 = pkh(&PRIV_KEY_1);
        stakes
            .add_stake(
                StakeKey {
                    validator: pkh_1,
                    withdrawer: pkh_1,
                },
                100_000_000_000.into(),
                100,
                true,
                1_000_000_000.into(),
            )
            .unwrap();

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
        chain_manager.chain_state.stakes = stakes;

        // Take a snapshot
        let mut snapshot = ChainStateSnapshot::default();
        snapshot.take(1, &chain_manager.chain_state);

        // Check staker power
        assert_eq!(
            snapshot
                .previous_chain_state
                .as_ref()
                .unwrap()
                .0
                .stakes
                .query_power(pkh_1, Capability::Mining, 300),
            Ok(20_000)
        );

        // Reset capability age of staker
        chain_manager
            .chain_state
            .stakes
            .reset_age(pkh_1, Capability::Mining, 300, 1)
            .unwrap();

        // Check its power is zero
        assert_eq!(
            chain_manager
                .chain_state
                .stakes
                .query_power(pkh_1, Capability::Mining, 300),
            Ok(0)
        );

        // Power in the snapshotted stakes tracker should not have changed
        assert_eq!(
            snapshot
                .previous_chain_state
                .as_ref()
                .unwrap()
                .0
                .stakes
                .query_power(pkh_1, Capability::Mining, 300),
            Ok(20_000)
        );
    }

    static LAST_VRF_INPUT: &str =
        "4da71b67e7e50ae4ad06a71e505244f8b490da55fc58c50386c908f7146d2239";

    fn build_block_with_tally_transactions(
        block_epoch: u32,
        data_request_pool: &mut DataRequestPool,
        tally_transactions: Vec<TallyTransaction>,
        stakes: &StakesTracker,
    ) -> Block {
        let mut transaction_pool = TransactionsPool::default();
        let unspent_outputs_pool = UnspentOutputsPool::default();

        let beacon = CheckpointBeacon {
            checkpoint: block_epoch,
            ..Default::default()
        };

        // Add valid vrf proof
        let vrf = &mut VrfCtx::secp256k1().unwrap();
        let secret_key = SecretKey {
            bytes: Protected::from(PRIV_KEY_1),
        };
        let vrf_input = CheckpointVRF {
            hash_prev_vrf: LAST_VRF_INPUT.parse().unwrap(),
            checkpoint: block_epoch - 1,
        };
        let block_proof = BlockEligibilityClaim::create(vrf, &secret_key, vrf_input).unwrap();

        let active_wips: ActiveWips = ActiveWips {
            active_wips: HashMap::default(),
            block_epoch,
        };

        let (block_header, txns) = build_block(
            (
                &mut transaction_pool,
                &unspent_outputs_pool,
                data_request_pool,
            ),
            0, // max_vt_weight
            0, // max_dr_weight
            beacon,
            block_proof,
            &tally_transactions,
            PublicKeyHash::default(),
            EpochConstants::default(),
            block_epoch,
            1_000_000_000, // collateral_minimum
            None,
            None,
            0,
            250_000_000_000, // initial_block_reward
            1_000_000_000,   // checkpoint_zero_timestamp
            1_000_000,       // halving_period
            0,
            &active_wips,
            Some(stakes.validator_count()),
            stakes,
            &ConsensusConstantsWit2::default(),
        );

        // Create a KeyedSignature
        let Hash::SHA256(data) = block_header.versioned_hash(get_protocol_version(Some(0)));
        let secret_key =
            Secp256k1_SecretKey::from_slice(&PRIV_KEY_1).expect("32 bytes, within curve order");
        let public_key = Secp256k1_PublicKey::from_secret_key_global(&secret_key);
        let signature = sign(secret_key, &data).unwrap();
        let witnet_pk = PublicKey::from(public_key);
        let witnet_signature = Signature::from(signature);

        // Create the block
        Block::new(
            block_header,
            KeyedSignature {
                signature: witnet_signature,
                public_key: witnet_pk,
            },
            txns,
        )
    }

    #[test]
    fn test_stake_changes() {
        register_protocol_version(ProtocolVersion::V2_0, 80, 15);

        let pkh_1 = pkh(&PRIV_KEY_1);
        let pkh_2 = pkh(&PRIV_KEY_2);
        let pkh_3 = pkh(&PRIV_KEY_3);
        let pkh_4 = pkh(&PRIV_KEY_4);

        // Create stakes tracker
        let mut stakes = StakesTracker::default();
        let mut validators = vec![];
        for pkh in [pkh_1, pkh_2, pkh_3, pkh_4] {
            validators.push(StakeKey {
                validator: pkh,
                withdrawer: pkh,
            });
        }
        for validator in &validators {
            stakes
                .add_stake(
                    validator.clone(),
                    100_000_000_000_000.into(),
                    100,
                    true,
                    10_000_000_000_000.into(),
                )
                .unwrap();
        }

        // Build pools
        let epoch = 200;
        let mut data_request_pool = DataRequestPool::default();
        let data_request_state = DataRequestState {
            data_request: DataRequestOutput {
                collateral: 1_000_000_000,
                witness_reward: 1_000_000_000_000, // Hey, big spender
                ..Default::default()
            },
            info: DataRequestInfo {
                commits: HashMap::from([
                    (pkh_1, CommitTransaction::default()),
                    (pkh_2, CommitTransaction::default()),
                    (pkh_3, CommitTransaction::default()),
                    (pkh_4, CommitTransaction::default()),
                ]),
                ..Default::default()
            },
            stage: DataRequestStage::TALLY,
            epoch,
            ..Default::default()
        };
        data_request_pool.data_request_pool =
            HashMap::from([(Hash::from(vec![1; 32]), data_request_state)]);

        let unspent_outputs_pool = UnspentOutputsPool::default();

        // Test stake changes with a block with a tally with four honests
        let tally_transactions = vec![TallyTransaction::new(
            Hash::from(vec![1; 32]), // dr_pointer
            vec![],                  // tally
            vec![],                  // outputs
            vec![],                  // out_of_consensus
            vec![],                  // error_committers
        )];
        let block_one_succesful_tally = build_block_with_tally_transactions(
            epoch + 3,
            &mut data_request_pool,
            tally_transactions,
            &stakes,
        );

        process_wit2_stakes_changes(
            &block_one_succesful_tally,
            &ConsensusConstantsWit2::default(),
            &data_request_pool,
            &mut stakes,
            EpochConstants::default(),
            &UtxoDiff::new(&unspent_outputs_pool, epoch),
        );

        // The first staker receives a 50 WIT block reward, his 1 WIT collateral and a 1_000 WIT data request reward
        let new_coins = stakes
            .query_stakes(QueryStakesKey::Key(validators[0].clone()))
            .unwrap()
            .first()
            .unwrap()
            .value
            .coins;
        assert_eq!(new_coins, 101_051_000_000_000.into());
        // The other stakers all receive their 1 WIT collateral back and a 1_000 WIT data request reward
        for validator in &validators[1..3] {
            let new_coins = stakes
                .query_stakes(QueryStakesKey::Key(validator.clone()))
                .unwrap()
                .first()
                .unwrap()
                .value
                .coins;
            assert_eq!(new_coins, 101_001_000_000_000.into());
        }

        // Test stake changes with a block with a tally with all errors
        let tally_transactions = vec![TallyTransaction::new(
            Hash::from(vec![1; 32]),          // dr_pointer
            vec![],                           // tally
            vec![],                           // outputs
            vec![],                           // out_of_consensus
            vec![pkh_1, pkh_2, pkh_3, pkh_4], // error_committers
        )];
        let block_tally_all_errors = build_block_with_tally_transactions(
            epoch,
            &mut data_request_pool,
            tally_transactions,
            &stakes,
        );

        process_wit2_stakes_changes(
            &block_tally_all_errors,
            &ConsensusConstantsWit2::default(),
            &data_request_pool,
            &mut stakes,
            EpochConstants::default(),
            &UtxoDiff::new(&unspent_outputs_pool, epoch),
        );

        // The first staker receives a 50 WIT block reward and his 1 WIT collateral back
        let new_coins = stakes
            .query_stakes(QueryStakesKey::Key(validators[0].clone()))
            .unwrap()
            .first()
            .unwrap()
            .value
            .coins;
        assert_eq!(new_coins, 101_102_000_000_000.into());
        // The other stakers all receive their 1 WIT collateral back
        for validator in &validators[1..3] {
            let new_coins = stakes
                .query_stakes(QueryStakesKey::Key(validator.clone()))
                .unwrap()
                .first()
                .unwrap()
                .value
                .coins;
            assert_eq!(new_coins, 101_002_000_000_000.into());
        }

        // Test stake changes with a block with a tally with one error and three honests
        let tally_transactions = vec![TallyTransaction::new(
            Hash::from(vec![1; 32]), // dr_pointer
            vec![],                  // tally
            vec![],                  // outputs
            vec![pkh_2],             // out_of_consensus
            vec![pkh_2],             // error_committers
        )];
        let block_tally_one_error = build_block_with_tally_transactions(
            epoch,
            &mut data_request_pool,
            tally_transactions,
            &stakes,
        );

        process_wit2_stakes_changes(
            &block_tally_one_error,
            &ConsensusConstantsWit2::default(),
            &data_request_pool,
            &mut stakes,
            EpochConstants::default(),
            &UtxoDiff::new(&unspent_outputs_pool, epoch),
        );

        // The first staker receives a 50 WIT block reward, his 1 WIT collateral and a reward of 1_000 WIT
        let new_coins = stakes
            .query_stakes(QueryStakesKey::Key(validators[0].clone()))
            .unwrap()
            .first()
            .unwrap()
            .value
            .coins;
        assert_eq!(new_coins, 102_153_000_000_000.into());
        // The second staker revealed an error so he only gets his 1 WIT collateral back
        let new_coins = stakes
            .query_stakes(QueryStakesKey::Key(validators[1].clone()))
            .unwrap()
            .first()
            .unwrap()
            .value
            .coins;
        assert_eq!(new_coins, 101_003_000_000_000.into());
        // The other stakers all receive their 1 WIT collateral back and a reward of 1_000 WIT
        for validator in &validators[2..3] {
            let new_coins = stakes
                .query_stakes(QueryStakesKey::Key(validator.clone()))
                .unwrap()
                .first()
                .unwrap()
                .value
                .coins;
            assert_eq!(new_coins, 102_003_000_000_000.into());
        }

        // Test stake changes with a block with a tally with one liar and three honests
        let tally_transactions = vec![TallyTransaction::new(
            Hash::from(vec![1; 32]), // dr_pointer
            vec![],                  // tally
            vec![],                  // outputs
            vec![pkh_2],             // out_of_consensus
            vec![],                  // error_committers
        )];
        let block_tally_one_liar = build_block_with_tally_transactions(
            epoch,
            &mut data_request_pool,
            tally_transactions,
            &stakes,
        );

        process_wit2_stakes_changes(
            &block_tally_one_liar,
            &ConsensusConstantsWit2::default(),
            &data_request_pool,
            &mut stakes,
            EpochConstants::default(),
            &UtxoDiff::new(&unspent_outputs_pool, epoch),
        );

        // The first staker receives a 50 WIT block reward, his 1 WIT collateral and a reward of 1_000 WIT
        let new_coins = stakes
            .query_stakes(QueryStakesKey::Key(validators[0].clone()))
            .unwrap()
            .first()
            .unwrap()
            .value
            .coins;
        assert_eq!(new_coins, 103_204_000_000_000.into());
        // The second staker revealed a lie so he receives nothing
        let new_coins = stakes
            .query_stakes(QueryStakesKey::Key(validators[1].clone()))
            .unwrap()
            .first()
            .unwrap()
            .value
            .coins;
        assert_eq!(new_coins, 101_003_000_000_000.into());
        // The other stakers all receive their 1 WIT collateral back and a reward of 1_000 WIT
        for validator in &validators[2..3] {
            let new_coins = stakes
                .query_stakes(QueryStakesKey::Key(validator.clone()))
                .unwrap()
                .first()
                .unwrap()
                .value
                .coins;
            assert_eq!(new_coins, 103_004_000_000_000.into());
        }

        // Test stake changes with a block with a tally with one error, one liar and two honests
        let tally_transactions = vec![TallyTransaction::new(
            Hash::from(vec![1; 32]), // dr_pointer
            vec![],                  // tally
            vec![],                  // outputs
            vec![pkh_2, pkh_3],      // out_of_consensus
            vec![pkh_3],             // error_committers
        )];
        let block_tally_one_liar_one_error = build_block_with_tally_transactions(
            epoch,
            &mut data_request_pool,
            tally_transactions,
            &stakes,
        );

        process_wit2_stakes_changes(
            &block_tally_one_liar_one_error,
            &ConsensusConstantsWit2::default(),
            &data_request_pool,
            &mut stakes,
            EpochConstants::default(),
            &UtxoDiff::new(&unspent_outputs_pool, epoch),
        );

        // The first staker receives a 50 WIT block reward, his 1 WIT collateral and a reward of 1_000 WIT
        let new_coins = stakes
            .query_stakes(QueryStakesKey::Key(validators[0].clone()))
            .unwrap()
            .first()
            .unwrap()
            .value
            .coins;
        assert_eq!(new_coins, 104_255_000_000_000.into());
        // The second staker revealed a lie so he receives nothing
        let new_coins = stakes
            .query_stakes(QueryStakesKey::Key(validators[1].clone()))
            .unwrap()
            .first()
            .unwrap()
            .value
            .coins;
        assert_eq!(new_coins, 101_003_000_000_000.into());
        // The third staker revealed an error so he receives his 1 WIT collateral back
        let new_coins = stakes
            .query_stakes(QueryStakesKey::Key(validators[2].clone()))
            .unwrap()
            .first()
            .unwrap()
            .value
            .coins;
        assert_eq!(new_coins, 103_005_000_000_000.into());
        // The fourth staker revealed receives his 1 WIT collateral and a reward of 1_000 WIT
        let new_coins = stakes
            .query_stakes(QueryStakesKey::Key(validators[3].clone()))
            .unwrap()
            .first()
            .unwrap()
            .value
            .coins;
        assert_eq!(new_coins, 104_005_000_000_000.into());
    }

    #[test]
    fn test_unstake_unique_nonce() {
        register_protocol_version(ProtocolVersion::V2_0, 100, 15);

        // Create inputs
        let vti_1 = Input::new(
            "1111111111111111111111111111111111111111111111111111111111111111:0"
                .parse()
                .unwrap(),
        );
        let vti_2 = Input::new(
            "2222222222222222222222222222222222222222222222222222222222222222:0"
                .parse()
                .unwrap(),
        );

        // Start epoch
        let mut block_epoch = 101;

        // Create addresses and validators
        let pkh_1 = pkh(&PRIV_KEY_1);
        let pkh_2 = pkh(&PRIV_KEY_2);

        let validator_1 = StakeKey {
            validator: pkh_1,
            withdrawer: pkh_1,
        };
        let validator_2 = StakeKey {
            validator: pkh_2,
            withdrawer: pkh_2,
        };

        // Create stakes tracker
        let mut stakes = StakesTracker::default();

        let stake_txns_1 = vec![
            StakeTransaction::new(
                StakeTransactionBody::new(
                    vec![vti_1],
                    StakeOutput {
                        value: 1_000_000_000_000_000,
                        key: validator_1.clone(),
                        ..Default::default()
                    },
                    None,
                ),
                vec![],
            ),
            StakeTransaction::new(
                StakeTransactionBody::new(
                    vec![vti_2],
                    StakeOutput {
                        value: 2_000_000_000_000_000,
                        key: validator_2.clone(),
                        ..Default::default()
                    },
                    None,
                ),
                vec![],
            ),
        ];

        // Process all stake transactions
        process_stake_transactions(
            &mut stakes,
            stake_txns_1.iter(),
            block_epoch,
            10_000_000_000_000,
        )
        .unwrap();

        let unstake_txn_1 = UnstakeTransaction::new(
            UnstakeTransactionBody::new(
                pkh_2,
                ValueTransferOutput {
                    pkh: pkh_2,
                    value: 2_000_000_000_000_000,
                    time_lock: 1000,
                },
                0,
                stakes.query_nonce(validator_2.clone()).unwrap(),
            ),
            KeyedSignature::default(),
        );

        // Check unstake nonce
        assert_eq!(stakes.query_nonce(validator_2.clone()), Ok(101));

        // Advance time
        block_epoch += 1;

        // Unstake all for validator 2
        process_unstake_transactions(
            &mut stakes,
            [unstake_txn_1.clone()].iter(),
            block_epoch,
            10_000_000_000_000,
        )
        .unwrap();

        // Check validator 2 is removed from stakes
        assert_eq!(
            stakes.query_stakes(QueryStakesKey::Key(validator_2.clone())),
            Err(StakesError::EntryNotFound {
                key: validator_2.clone()
            })
        );

        // Advance time
        block_epoch += 1;

        // Create new stake transaction for the same validator 2 and process it
        let stake_txns_2 = vec![StakeTransaction::new(
            StakeTransactionBody::new(
                vec![vti_1],
                StakeOutput {
                    value: 3_000_000_000_000_000,
                    key: validator_2.clone(),
                    ..Default::default()
                },
                None,
            ),
            vec![],
        )];

        process_stake_transactions(
            &mut stakes,
            stake_txns_2.iter(),
            block_epoch,
            10_000_000_000_000,
        )
        .unwrap();

        // Advance time
        block_epoch += 1;

        let unstake_txn_2 = UnstakeTransaction::new(
            UnstakeTransactionBody::new(
                pkh_2,
                ValueTransferOutput {
                    pkh: pkh_2,
                    value: 3_000_000_000_000_000,
                    time_lock: 1000,
                },
                0,
                stakes.query_nonce(validator_2.clone()).unwrap(),
            ),
            KeyedSignature::default(),
        );

        // Check unstake nonce
        assert_eq!(stakes.query_nonce(validator_2.clone()), Ok(103));

        // Check hashes are unique due to unique nonces
        assert_ne!(unstake_txn_1.hash(), unstake_txn_2.hash());

        // Unstake all again for validator 2
        process_unstake_transactions(
            &mut stakes,
            [unstake_txn_2].iter(),
            block_epoch,
            10_000_000_000_000,
        )
        .unwrap();

        // Check validator 2 is removed from stakes again
        assert_eq!(
            stakes.query_stakes(QueryStakesKey::Key(validator_2.clone())),
            Err(StakesError::EntryNotFound { key: validator_2 })
        );
    }
}
