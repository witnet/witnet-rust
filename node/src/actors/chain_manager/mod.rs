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
use std::collections::HashMap;

use actix::{
    ActorFuture, AsyncContext, Context, ContextFutureSpawner, Supervised, System, SystemService,
    WrapFuture,
};
use ansi_term::Color::{Purple, White, Yellow};
use log::{debug, error, info, warn};
use witnet_rad::types::RadonTypes;

use witnet_data_structures::{
    chain::{
        ActiveDataRequestPool, Block, ChainState, CheckpointBeacon, DataRequestReport, Epoch, Hash,
        Hashable, InventoryItem, Output, OutputPointer, Transaction, TransactionsPool,
        UnspentOutputsPool,
    },
    data_request::DataRequestPool,
    serializers::decoders::TryFrom,
    validations::{validate_block, validate_candidate},
};
use witnet_storage::{error::StorageError, storage::Storable};
use witnet_util::error::WitnetError;

use crate::actors::{
    inventory_manager::InventoryManager,
    messages::{AddItem, AddTransaction, Broadcast, Put, SendInventoryItem},
    sessions_manager::SessionsManager,
    storage_keys::CHAIN_STATE_KEY,
    storage_manager::StorageManager,
};

mod actor;
mod handlers;
mod mining;

/// Maximum blocks number to be sent during synchronization process
pub const MAX_BLOCKS_SYNC: usize = 500;

/// Possible errors when interacting with ChainManager
#[derive(Debug)]
pub enum ChainManagerError {
    /// A block being processed was already known to this node
    BlockAlreadyExists,
    /// A block does not exist
    BlockDoesNotExist,
    /// StorageError
    StorageError(WitnetError<StorageError>),
}

impl From<WitnetError<StorageError>> for ChainManagerError {
    fn from(x: WitnetError<StorageError>) -> Self {
        ChainManagerError::StorageError(x)
    }
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
#[derive(Default)]
pub struct ChainManager {
    /// Blockchain state data structure
    chain_state: ChainState,
    /// Current Epoch
    current_epoch: Option<Epoch>,
    /// Transactions Pool (_mempool_)
    transactions_pool: TransactionsPool,
    /// Maximum weight each block can have
    max_block_weight: u32,
    // Random value to help with debugging because there is no signature
    // and all the mined blocks have the same hash.
    // This random value helps to distinguish blocks mined on different nodes
    // To be removed when we implement real signing.
    // TODO: Remove after create signatures
    random: u64,
    /// Mining enabled
    mining_enabled: bool,
    /// Hash of the genesis block
    genesis_block_hash: Hash,
    /// Pool of active data requests
    data_request_pool: DataRequestPool,
    /// state of the state machine
    sm_state: StateMachine,
    /// The best beacon known to this node—to which it will try to catch up
    target_beacon: Option<CheckpointBeacon>,
    /// Map that stores candidate blocks for further validation and consolidation as tip of the blockchain
    candidates: HashMap<Hash, Block>,
}

/// Required trait for being able to retrieve ChainManager address from registry
impl Supervised for ChainManager {}

/// Required trait for being able to retrieve ChainManager address from registry
impl SystemService for ChainManager {}

/// Auxiliary methods for ChainManager actor
impl ChainManager {
    /// Method to persist chain_info into storage
    fn persist_chain_state(&self, ctx: &mut Context<Self>) {
        // Get StorageManager address
        let storage_manager_addr = System::current().registry().get::<StorageManager>();

        match self.chain_state.chain_info.as_ref() {
            Some(x) => x,
            None => {
                error!("Trying to persist an empty chain state value");
                return;
            }
        };

        // Persist chain_info into storage. `AsyncContext::wait` registers
        // future within context, but context waits until this future resolves
        // before processing any other events.
        let msg = Put::from_value(CHAIN_STATE_KEY, &self.chain_state).unwrap();
        storage_manager_addr
            .send(msg)
            .into_actor(self)
            .then(|res, _act, _ctx| {
                match res {
                    Ok(Ok(_)) => debug!("Successfully persisted chain_info into storage"),
                    _ => {
                        error!("Failed to persist chain_info into storage");
                        // FIXME(#72): handle errors
                    }
                }
                actix::fut::ok(())
            })
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
    fn persist_data_request(
        &self,
        ctx: &mut Context<Self>,
        (output_pointer, data_request_report): &(OutputPointer, DataRequestReport),
    ) {
        // Get StorageManager address
        let storage_manager_addr = System::current().registry().get::<StorageManager>();

        // Persist block_chain into storage. `AsyncContext::wait` registers
        // future within context, but context waits until this future resolves
        // before processing any other events.
        let msg = Put::from_value(output_pointer.to_bytes().unwrap(), data_request_report).unwrap();
        storage_manager_addr
            .send(msg)
            .into_actor(self)
            .then(|res, _act, _ctx| {
                match res {
                    Ok(Ok(_)) => debug!("Successfully persisted block_chain into storage"),
                    _ => {
                        error!("Failed to persist block_chain into storage");
                        // FIXME(#72): handle errors
                    }
                }
                actix::fut::ok(())
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

    // TODO: use proper error type with failure::Error
    fn process_requested_block(&mut self, ctx: &mut Context<Self>, block: Block) -> Result<(), ()> {
        if let (Some(current_epoch), Some(chain_info)) =
            (self.current_epoch, self.chain_state.chain_info.as_ref())
        {
            let chain_beacon = chain_info.highest_block_checkpoint;

            if let Ok(block_in_chain) = validate_block(
                &block,
                current_epoch,
                chain_beacon,
                self.genesis_block_hash,
                &self.chain_state.unspent_outputs_pool,
                &self.transactions_pool,
                &self.data_request_pool,
            ) {
                // Persist block and update ChainState
                self.consolidate_block(
                    ctx,
                    block_in_chain.block,
                    block_in_chain.utxo_set,
                    block_in_chain.data_request_pool,
                    false,
                );

                Ok(())
            } else {
                Err(())
            }
        } else {
            Err(())
        }
    }

    fn process_candidate(&mut self, block: Block) {
        if let Some(current_epoch) = self.current_epoch {
            let hash_block = block.hash();

            if !self.candidates.contains_key(&hash_block)
                && validate_candidate(&block, current_epoch).is_ok()
            {
                self.candidates.insert(hash_block, block.clone());
                self.broadcast_item(InventoryItem::Block(block));
            }
        } else {
            warn!("ChainManager doesn't have current epoch");
        }
    }

    fn consolidate_block(
        &mut self,
        ctx: &mut Context<Self>,
        block: Block,
        utxo_set: UnspentOutputsPool,
        dr_pool: DataRequestPool,
        info_flag: bool,
    ) {
        // Update chain_info
        match self.chain_state.chain_info.as_mut() {
            Some(chain_info) => {
                let block_hash = block.hash();
                let block_epoch = block.block_header.beacon.checkpoint;

                // Update `highest_block_checkpoint`
                let beacon = CheckpointBeacon {
                    checkpoint: block_epoch,
                    hash_prev_block: block_hash,
                };
                chain_info.highest_block_checkpoint = beacon;

                // Update UnspentOutputsPool
                self.chain_state.unspent_outputs_pool = utxo_set;

                // Update TransactionPool
                update_transaction_pool(&mut self.transactions_pool, block.txns.as_ref());

                // Update DataRequestPool
                self.data_request_pool = dr_pool;
                let reveals = self.data_request_pool.update_data_request_stages();
                for reveal in reveals {
                    // Send AddTransaction message to self
                    // And broadcast it to all of peers
                    ctx.address().do_send(AddTransaction {
                        transaction: reveal,
                    })
                }
                // Persist finished data requests into storage
                let to_be_stored = self.data_request_pool.finished_data_requests();
                to_be_stored.into_iter().for_each(|dr| {
                    self.persist_data_request(ctx, &dr);
                    if info_flag {
                        show_info_tally(&self.chain_state.unspent_outputs_pool, dr, block_epoch);
                    }
                });
                // FIXME: Revisit to avoid data redundancies
                // Store active data requests
                self.chain_state.data_request_pool = ActiveDataRequestPool {
                    waiting_for_reveal: self.data_request_pool.waiting_for_reveal.clone(),
                    data_requests_by_epoch: self.data_request_pool.data_requests_by_epoch.clone(),
                    data_request_pool: self.data_request_pool.data_request_pool.clone(),
                    to_be_stored: self.data_request_pool.to_be_stored.clone(),
                    dr_pointer_cache: self.data_request_pool.dr_pointer_cache.clone(),
                };
                if info_flag {
                    show_info_dr(&self.data_request_pool, &block);

                    debug!("{:?}", block);
                    debug!("Mint transaction hash: {:?}", block.txns[0].hash());
                }

                // Insert candidate block into `block_chain` and persist it
                self.chain_state.block_chain.insert(block_epoch, block_hash);
                self.persist_item(ctx, InventoryItem::Block(block));

                // Persist chain_info into storage
                self.persist_chain_state(ctx);
            }
            None => {
                error!("No ChainInfo loaded in ChainManager");
            }
        }
    }
}

// Helper methods
fn update_transaction_pool(transactions_pool: &mut TransactionsPool, transactions: &[Transaction]) {
    for transaction in transactions {
        transactions_pool.remove(&transaction.hash());
    }
}

fn show_info_tally(
    unspent_outputs_pool: &UnspentOutputsPool,
    dr: (OutputPointer, DataRequestReport),
    block_epoch: Epoch,
) {
    let tally_output_pointer = dr.1.tally;
    let tr = unspent_outputs_pool.get(&tally_output_pointer);
    if let Some(Output::Tally(tally_output)) = tr {
        let result = RadonTypes::try_from(tally_output.result.as_slice())
            .map(|x| x.to_string())
            .unwrap_or_else(|_| "RADError".to_string());
        info!(
            "{} {} completed at epoch #{} with result: {}",
            Yellow.bold().paint("[Data Request]"),
            Yellow.bold().paint(&dr.0.to_string()),
            Yellow.bold().paint(block_epoch.to_string()),
            Yellow.bold().paint(result),
        );
    }
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
