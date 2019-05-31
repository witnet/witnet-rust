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
    collections::{HashMap, HashSet},
    convert::TryFrom,
};

use actix::{
    prelude::*, ActorFuture, AsyncContext, Context, ContextFutureSpawner, Supervised, System,
    SystemService, WrapFuture,
};
use ansi_term::Color::{Purple, White, Yellow};
use failure::Fail;
use itertools::Itertools;
use log::{debug, error, info, warn};

use crate::{
    actors::{
        inventory_manager::InventoryManager,
        json_rpc::JsonRpcServer,
        messages::{AddItem, AddTransaction, Broadcast, NewBlock, SendInventoryItem},
        sessions_manager::SessionsManager,
        storage_keys::CHAIN_STATE_KEY,
    },
    storage_mngr,
};
use witnet_data_structures::{
    chain::{
        penalize_factor, reputation_issuance, Alpha, Block, ChainState, CheckpointBeacon,
        ConsensusConstants, DataRequestReport, Epoch, Hash, Hashable, InventoryItem, OutputPointer,
        PublicKeyHash, Reputation, ReputationEngine, TransactionsPool, UnspentOutputsPool,
    },
    data_request::DataRequestPool,
    transaction::{RevealTransaction, TallyTransaction, Transaction},
    vrf::VrfCtx,
};
use witnet_rad::types::RadonTypes;
use witnet_validations::validations::{validate_block, validate_candidate, Diff};

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
    /// StorageError
    #[fail(display = "ChainManager is not ready yet")]
    ChainNotReady,
}

/// State Machine
#[derive(Debug)]
pub enum StateMachine {
    /// First state, ChainManager is waiting to consensus between its peers
    WaitingConsensus,
    /// Second state, ChainManager synchronization process
    Synchronizing,
    /// Third state, ChainManager is ready to mine and consolidated blocks
    Synced,
}

impl Default for StateMachine {
    fn default() -> Self {
        StateMachine::WaitingConsensus
    }
}

////////////////////////////////////////////////////////////////////////////////////////
// ACTOR BASIC STRUCTURE
////////////////////////////////////////////////////////////////////////////////////////
/// ChainManager actor
#[derive(Debug, Default)]
pub struct ChainManager {
    /// Blockchain state data structure
    chain_state: ChainState,
    /// Current Epoch
    current_epoch: Option<Epoch>,
    /// Transactions Pool (_mempool_)
    transactions_pool: TransactionsPool,
    /// Maximum weight each block can have
    max_block_weight: u32,
    /// Mining enabled
    mining_enabled: bool,
    /// Hash of the genesis block
    genesis_block_hash: Hash,
    /// state of the state machine
    sm_state: StateMachine,
    /// The best beacon known to this node—to which it will try to catch up
    target_beacon: Option<CheckpointBeacon>,
    /// Map that stores candidate blocks for further validation and consolidation as tip of the blockchain
    candidates: HashMap<Hash, Block>,
    /// Our public key hash, used to create the mint transaction
    own_pkh: Option<PublicKeyHash>,
    /// VRF context
    vrf_ctx: Option<VrfCtx>,
}

/// Required trait for being able to retrieve ChainManager address from registry
impl Supervised for ChainManager {}

/// Required trait for being able to retrieve ChainManager address from registry
impl SystemService for ChainManager {}

/// Auxiliary methods for ChainManager actor
impl ChainManager {
    /// Method to persist chain_info into storage
    fn persist_chain_state(&self, ctx: &mut Context<Self>) {
        match self.chain_state.chain_info.as_ref() {
            Some(x) => x,
            None => {
                error!("Trying to persist an empty chain state value");
                return;
            }
        };

        storage_mngr::put(&CHAIN_STATE_KEY, &self.chain_state)
            .into_actor(self)
            .and_then(|_, _, _| {
                debug!("Successfully persisted chain_info into storage");
                fut::ok(())
            })
            .map_err(|err, _, _| error!("Failed to persist chain_info into storage: {}", err))
            .wait(ctx);
    }

    /// Method to Send an Item to Inventory Manager
    fn persist_item(&self, ctx: &mut Context<Self>, item: InventoryItem) {
        // Get InventoryManager address
        let inventory_manager_addr = System::current().registry().get::<InventoryManager>();

        // Persist block into storage through InventoryManager. `AsyncContext::wait` registers
        // future within context, but context waits until this future resolves
        // before processing any other events.
        inventory_manager_addr
            .send(AddItem { item })
            .into_actor(self)
            .then(|res, _act, _ctx| match res {
                // Process the response from InventoryManager
                Err(e) => {
                    // Error when sending message
                    error!("Unsuccessful communication with InventoryManager: {}", e);
                    actix::fut::err(())
                }
                Ok(res) => match res {
                    Err(e) => {
                        // InventoryManager error
                        error!("Error while getting block from InventoryManager: {}", e);
                        actix::fut::err(())
                    }
                    Ok(_) => actix::fut::ok(()),
                },
            })
            .wait(ctx)
    }

    /// Method to persist a Data Request into the Storage
    fn persist_data_request(&self, ctx: &mut Context<Self>, dr_report: &DataRequestReport) {
        let dr_pointer = &dr_report.tally.dr_pointer;
        let dr_pointer_string = dr_pointer.to_string();
        storage_mngr::put(dr_pointer, dr_report)
            .into_actor(self)
            .map_err(|e, _, _| error!("Failed to persist data request report into storage: {}", e))
            .and_then(move |_, _, _| {
                debug!(
                    "Successfully persisted report for data request {} into storage",
                    dr_pointer_string
                );
                fut::ok(())
            })
            .wait(ctx);
    }

    fn broadcast_item(&self, item: InventoryItem) {
        // Get SessionsManager address
        let sessions_manager_addr = System::current().registry().get::<SessionsManager>();

        sessions_manager_addr.do_send(Broadcast {
            command: SendInventoryItem { item },
            only_inbound: false,
        });
    }

    fn process_requested_block(
        &mut self,
        ctx: &mut Context<Self>,
        block: &Block,
    ) -> Result<(), failure::Error> {
        if let (Some(current_epoch), Some(chain_info), Some(rep_engine)) = (
            self.current_epoch,
            self.chain_state.chain_info.as_ref(),
            self.chain_state.reputation_engine.as_ref(),
        ) {
            let chain_beacon = chain_info.highest_block_checkpoint;
            let total_identities = rep_engine.ars.active_identities_number() as u32;

            match validate_block(
                block,
                current_epoch,
                chain_beacon,
                self.genesis_block_hash,
                &self.chain_state.unspent_outputs_pool,
                &self.chain_state.data_request_pool,
                self.vrf_ctx.as_mut().unwrap(),
                total_identities,
            ) {
                Ok(utxo_diff) => {
                    // Persist block and update ChainState
                    self.consolidate_block(ctx, block, utxo_diff);

                    Ok(())
                }
                Err(e) => Err(e),
            }
        } else {
            Err(ChainManagerError::ChainNotReady)?
        }
    }

    fn process_candidate(&mut self, block: Block) {
        if let (Some(current_epoch), Some(rep_engine)) = (
            self.current_epoch,
            self.chain_state.reputation_engine.as_ref(),
        ) {
            let hash_block = block.hash();
            let total_identities = rep_engine.ars.active_identities_number() as u32;

            if !self.candidates.contains_key(&hash_block) {
                match validate_candidate(
                    &block,
                    current_epoch,
                    self.vrf_ctx.as_mut().unwrap(),
                    total_identities,
                ) {
                    Ok(()) => {
                        self.candidates.insert(hash_block, block.clone());
                        self.broadcast_item(InventoryItem::Block(block));
                    }
                    Err(e) => warn!("{}", e),
                }
            }
        } else {
            warn!("ChainManager doesn't have current epoch");
        }
    }

    fn persist_blocks_batch(
        &self,
        ctx: &mut Context<Self>,
        blocks: Vec<Block>,
        target_beacon: CheckpointBeacon,
    ) {
        for block in blocks {
            let block_hash = block.hash();
            self.persist_item(ctx, InventoryItem::Block(block));

            if block_hash == target_beacon.hash_prev_block {
                break;
            }
        }
    }

    fn consolidate_block(&mut self, ctx: &mut Context<Self>, block: &Block, utxo_diff: Diff) {
        // Update chain_info and reputation_engine
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

                // Print reputation logs on debug level on synced state,
                // but on trace level while synchronizing
                let log_level = if let StateMachine::Synced = self.sm_state {
                    log::Level::Debug
                } else {
                    log::Level::Trace
                };

                chain_info.highest_block_checkpoint = beacon;
                let rep_info = update_pools(
                    &block,
                    &mut self.chain_state.unspent_outputs_pool,
                    &mut self.chain_state.data_request_pool,
                    &mut self.transactions_pool,
                    utxo_diff,
                    self.own_pkh,
                    &mut self.chain_state.own_utxos,
                );

                update_reputation(
                    reputation_engine,
                    &chain_info.consensus_constants,
                    rep_info,
                    log_level,
                );

                // Insert candidate block into `block_chain` state
                self.chain_state.block_chain.insert(block_epoch, block_hash);

                match self.sm_state {
                    StateMachine::Synchronizing => {
                        let _reveals = self
                            .chain_state
                            .data_request_pool
                            .update_data_request_stages();
                    }
                    StateMachine::Synced => {
                        // Persist finished data requests into storage
                        let to_be_stored =
                            self.chain_state.data_request_pool.finished_data_requests();
                        to_be_stored.into_iter().for_each(|dr_report| {
                            show_info_tally(&dr_report.tally, block_epoch);
                            self.persist_data_request(ctx, &dr_report);
                        });

                        show_info_dr(&self.chain_state.data_request_pool, &block);

                        log::trace!("{:?}", block);
                        debug!("Mint transaction hash: {:?}", block.txns.mint.hash());

                        let reveals = self
                            .chain_state
                            .data_request_pool
                            .update_data_request_stages();

                        for reveal in reveals {
                            // Send AddTransaction message to self
                            // And broadcast it to all of peers
                            ctx.address().do_send(AddTransaction {
                                transaction: Transaction::Reveal(reveal),
                            })
                        }
                        self.persist_item(ctx, InventoryItem::Block(block.clone()));

                        // Persist chain_info into storage
                        self.persist_chain_state(ctx);

                        // Send notification to JsonRpcServer
                        JsonRpcServer::from_registry().do_send(NewBlock {
                            block: block.clone(),
                        })
                    }
                    _ => {}
                }
            }
            _ => {
                error!("No ChainInfo loaded in ChainManager");
            }
        }
    }

    fn get_chain_beacon(&self) -> CheckpointBeacon {
        self.chain_state
            .chain_info
            .as_ref()
            .unwrap()
            .highest_block_checkpoint
    }
}

#[derive(Debug, Default)]
struct ReputationInfo {
    // Counter of "witnessing acts".
    // For every data request with a tally in this block, increment alpha_diff
    // by the number of witnesses specified in the data request.
    alpha_diff: Alpha,

    // Map used to count the number of lies of every identity that participated
    // in data requests with a tally in this block.
    // Honest identities are also inserted into this map, with lie count = 0.
    lie_count: HashMap<PublicKeyHash, u32>,
}

impl ReputationInfo {
    fn new() -> Self {
        Self::default()
    }

    fn update(
        &mut self,
        tally_transaction: &TallyTransaction,
        data_request_pool: &DataRequestPool,
    ) {
        let dr_pointer = tally_transaction.dr_pointer;
        let dr_state = &data_request_pool.data_request_pool[&dr_pointer];
        let reveals = &dr_state.info.reveals;
        let replication_factor = dr_state.data_request.witnesses;
        self.alpha_diff += Alpha(u32::from(replication_factor));

        let tally = &tally_transaction.tally;

        for (pkh, reveal_tx) in reveals {
            // FIXME(#640): replace with real truthness check function from radon engine
            // (currently we assume that all nodes are honest)
            fn true_revealer(_reveal: &RevealTransaction, _tally: &[u8]) -> bool {
                true
            }
            let liar = if true_revealer(reveal_tx, tally) {
                0
            } else {
                1
            };
            // Insert all the revealers, and increment their lie count by 1 if they lied.
            // lie_count can contain identities which never lied, with lie_count = 0
            *self.lie_count.entry(*pkh).or_insert(0) += liar;
        }
    }
}

// Helper methods
fn update_pools(
    block: &Block,
    unspent_outputs_pool: &mut UnspentOutputsPool,
    data_request_pool: &mut DataRequestPool,
    transactions_pool: &mut TransactionsPool,
    utxo_diff: Diff,
    own_pkh: Option<PublicKeyHash>,
    own_utxos: &mut HashSet<OutputPointer>,
) -> ReputationInfo {
    let mut rep_info = ReputationInfo::new();

    for ta_tx in &block.txns.tally_txns {
        // Process tally transactions: used to update reputation engine
        rep_info.update(&ta_tx, data_request_pool);

        // IMPORTANT: Update the data request pool after updating reputation info
        if let Err(e) = data_request_pool.process_tally(&ta_tx, &block.hash()) {
            log::error!("Error updating pools:\n{}", e);
        }
    }

    for vt_tx in &block.txns.value_transfer_txns {
        transactions_pool.vt_remove(&vt_tx.hash());
    }

    for dr_tx in &block.txns.data_request_txns {
        data_request_pool.process_data_request(&dr_tx, block.block_header.beacon.checkpoint);

        transactions_pool.dr_remove(&dr_tx.hash());
    }

    for co_tx in &block.txns.commit_txns {
        if let Err(e) = data_request_pool.process_commit(&co_tx, &block.hash()) {
            log::error!("Error updating pools:\n{}", e);
        }
    }

    for re_tx in &block.txns.reveal_txns {
        if let Err(e) = data_request_pool.process_reveal(&re_tx, &block.hash()) {
            log::error!("Error updating pools:\n{}", e);
        }
    }

    // Remove commits and reveals because they expire in one epoch
    transactions_pool.clear_commits_reveals();

    // Update own_utxos:
    if let Some(own_pkh) = own_pkh {
        utxo_diff.visit(
            own_utxos,
            |own_utxos, output_pointer, output| {
                // Insert new outputs
                if output.pkh == own_pkh {
                    own_utxos.insert(output_pointer.clone());
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

fn separate_honest_liars<K, V, I>(rep_info: I) -> (Vec<K>, Vec<(K, V)>)
where
    V: Default + PartialEq,
    I: IntoIterator<Item = (K, V)>,
{
    let mut honests = vec![];
    let mut liars = vec![];
    for (pkh, num_lies) in rep_info {
        if num_lies == V::default() {
            honests.push(pkh);
        } else {
            liars.push((pkh, num_lies));
        }
    }

    (honests, liars)
}

fn update_reputation(
    rep_eng: &mut ReputationEngine,
    consensus_constants: &ConsensusConstants,
    ReputationInfo {
        alpha_diff,
        lie_count,
    }: ReputationInfo,
    log_level: log::Level,
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
    for (pkh, num_lies) in lie_count
        .iter()
        .sorted_by(|a, b| a.0.to_string().cmp(&b.0.to_string()))
    {
        log::log!(log_level, "    {}: {}", pkh, num_lies);
    }
    log::log!(log_level, "}}");
    let (honest, liars) = separate_honest_liars(lie_count.clone());
    let revealers = lie_count.into_iter().map(|(pkh, _num_lies)| pkh);
    // Leftover reputation from the previous epoch
    let extra_rep_previous_epoch = rep_eng.extra_reputation;
    // Expire in old_alpha to maximize reputation lost in penalizations.
    // Example: we are in old_alpha 10000, new_alpha 5 and some reputation expires in
    // alpha 10002. This reputation will expire in the next epoch.
    let expired_rep = rep_eng.trs.expire(&old_alpha);
    // There is some reputation issued for every witnessing act
    let issued_rep = reputation_issuance(
        Reputation(consensus_constants.reputation_issuance),
        Alpha(consensus_constants.reputation_issuance_stop),
        old_alpha,
        new_alpha,
    );
    // Penalize liars and accumulate the reputation
    // The penalization depends on the number of lies from the last epoch
    let liars_and_penalize_function = liars.iter().map(|(pkh, num_lies)| {
        (
            pkh,
            penalize_factor(
                consensus_constants.reputation_penalization_factor,
                *num_lies,
            ),
        )
    });
    let penalized_rep = rep_eng
        .trs
        .penalize_many(liars_and_penalize_function)
        .unwrap();

    let mut reputation_bounty = extra_rep_previous_epoch;
    reputation_bounty += expired_rep;
    reputation_bounty += issued_rep;
    reputation_bounty += penalized_rep;

    let num_honest = honest.len() as u32;

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
        let honest_gain = honest.into_iter().map(|pkh| (pkh, Reputation(rep_reward)));
        rep_eng.trs.gain(expire_alpha, honest_gain).unwrap();

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
    rep_eng.ars.push_activity(revealers);

    log::log!(log_level, "Total Reputation: {{");
    for (pkh, rep) in rep_eng
        .trs
        .identities()
        .sorted_by(|a, b| a.0.to_string().cmp(&b.0.to_string()))
    {
        let active = if rep_eng.ars.contains(pkh) { 'A' } else { ' ' };
        log::log!(log_level, "    [{}] {}: {}", active, pkh, rep.0);
    }
    log::log!(log_level, "}}");

    rep_eng.current_alpha = new_alpha;
}

fn show_info_tally(tally_tx: &TallyTransaction, block_epoch: Epoch) {
    let result = RadonTypes::try_from(tally_tx.tally.as_slice())
        .map(|x| x.to_string())
        .unwrap_or_else(|_| "RADError".to_string());
    info!(
        "{} {} completed at epoch #{} with result: {}",
        Yellow.bold().paint("[Data Request]"),
        Yellow.bold().paint(tally_tx.dr_pointer.to_string()),
        Yellow.bold().paint(block_epoch.to_string()),
        Yellow.bold().paint(result),
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
        info!(
            "{} Block {} consolidated for epoch #{} {}",
            Purple.bold().paint("[Chain]"),
            Purple.bold().paint(block_hash.to_string()),
            Purple.bold().paint(block_epoch.to_string()),
            White.paint("with no data requests".to_string()),
        );
    } else {
        info!(
            "{} Block {} consolidated for epoch #{}\n{}{}",
            Purple.bold().paint("[Chain]"),
            Purple.bold().paint(block_hash.to_string()),
            Purple.bold().paint(block_epoch.to_string()),
            White.bold().paint("Data Requests: "),
            White.bold().paint(info),
        );
    }
}
