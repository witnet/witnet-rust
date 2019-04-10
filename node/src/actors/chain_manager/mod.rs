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
use std::collections::{HashMap, HashSet};

use actix::prelude::*;
use actix::{
    ActorFuture, AsyncContext, Context, ContextFutureSpawner, Supervised, System, SystemService,
    WrapFuture,
};
use ansi_term::Color::{Purple, White, Yellow};
use failure::Fail;
use log::{debug, error, info, warn};

use crate::actors::{
    inventory_manager::InventoryManager,
    json_rpc::JsonRpcServer,
    messages::{AddItem, AddTransaction, Broadcast, NewBlock, SendInventoryItem},
    sessions_manager::SessionsManager,
    storage_keys::CHAIN_STATE_KEY,
};
use crate::storage_mngr;
use witnet_data_structures::{
    chain::{
        Block, ChainState, CheckpointBeacon, DataRequestReport, Epoch, Hash, Hashable,
        InventoryItem, Output, OutputPointer, PublicKeyHash, TransactionsPool, UnspentOutputsPool,
    },
    data_request::DataRequestPool,
    serializers::decoders::TryFrom,
};
use witnet_rad::types::RadonTypes;

use witnet_validations::validations::{validate_block, validate_candidate, Diff};

mod actor;
mod handlers;
mod mining;

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
    fn persist_data_request(
        &self,
        ctx: &mut Context<Self>,
        (output_pointer, data_request_report): &(OutputPointer, DataRequestReport),
    ) {
        storage_mngr::put(output_pointer, data_request_report)
            .into_actor(self)
            .map_err(|e, _, _| error!("Failed to persist block_chain into storage: {}", e))
            .and_then(|_, _, _| {
                debug!("Successfully persisted block_chain into storage");
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
        if let (Some(current_epoch), Some(chain_info)) =
            (self.current_epoch, self.chain_state.chain_info.as_ref())
        {
            let chain_beacon = chain_info.highest_block_checkpoint;

            match validate_block(
                block,
                current_epoch,
                chain_beacon,
                self.genesis_block_hash,
                &self.chain_state.unspent_outputs_pool,
                &self.chain_state.data_request_pool,
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
        if let Some(current_epoch) = self.current_epoch {
            let hash_block = block.hash();

            if !self.candidates.contains_key(&hash_block) {
                match validate_candidate(&block, current_epoch) {
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
                update_pools(
                    &block,
                    &mut self.chain_state.unspent_outputs_pool,
                    &mut self.chain_state.data_request_pool,
                    &mut self.transactions_pool,
                    utxo_diff,
                    self.own_pkh,
                    &mut self.chain_state.own_utxos,
                );

                // Insert candidate block into `block_chain` state
                self.chain_state.block_chain.insert(block_epoch, block_hash);

                if let StateMachine::Synced = self.sm_state {
                    // Persist finished data requests into storage
                    let to_be_stored = self.chain_state.data_request_pool.finished_data_requests();
                    to_be_stored.into_iter().for_each(|dr| {
                        self.persist_data_request(ctx, &dr);
                        show_info_tally(&self.chain_state.unspent_outputs_pool, dr, block_epoch);
                    });

                    show_info_dr(&self.chain_state.data_request_pool, &block);

                    log::trace!("{:?}", block);
                    debug!("Mint transaction hash: {:?}", block.txns[0].hash());

                    let reveals = self
                        .chain_state
                        .data_request_pool
                        .update_data_request_stages();

                    for reveal in reveals {
                        // Send AddTransaction message to self
                        // And broadcast it to all of peers
                        ctx.address().do_send(AddTransaction {
                            transaction: reveal,
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
            }
            None => {
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

// Helper methods
fn update_pools(
    block: &Block,
    unspent_outputs_pool: &mut UnspentOutputsPool,
    data_request_pool: &mut DataRequestPool,
    transactions_pool: &mut TransactionsPool,
    utxo_diff: Diff,
    own_pkh: Option<PublicKeyHash>,
    own_utxos: &mut HashSet<OutputPointer>,
) {
    for transaction in block.txns.iter() {
        data_request_pool.process_transaction(
            transaction,
            block.block_header.beacon.checkpoint,
            &block.hash(),
        );
        transactions_pool.remove(&transaction.hash());

        // Update own_utxos:
        if let Some(own_pkh) = own_pkh {
            // Remove spent inputs
            for input in &transaction.body.inputs {
                own_utxos.remove(&input.output_pointer());
            }
            // Insert new outputs
            for (i, output) in transaction.body.outputs.iter().enumerate() {
                if let Output::ValueTransfer(x) = output {
                    if x.pkh == own_pkh {
                        own_utxos.insert(OutputPointer {
                            transaction_id: transaction.hash(),
                            output_index: i as u32,
                        });
                    }
                }
            }
        }
    }

    utxo_diff.apply(unspent_outputs_pool);
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
