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
use actix::{
    ActorFuture, Context, ContextFutureSpawner, MailboxError, Supervised, System, SystemService,
    WrapFuture,
};
use ansi_term::Color::Purple;
use log::{debug, error, info, warn};
use std::collections::{HashMap, HashSet};

use self::messages::InventoryEntriesResult;

use crate::actors::{
    inventory_manager::{messages::AddItem, InventoryManager},
    reputation_manager::{messages::ValidatePoE, ReputationManager},
    session::messages::{AnnounceItems, RequestBlock, SendInventoryItem},
    sessions_manager::{
        messages::{Anycast, Broadcast},
        SessionsManager,
    },
    storage_keys::{BLOCK_CHAIN_KEY, CHAIN_STATE_KEY},
    storage_manager::{messages::Put, StorageManager},
};

use self::data_request::DataRequestPool;
use self::validations::{validate_merkle_tree, validate_transactions};

use witnet_data_structures::chain::{
    Block, Blockchain, ChainState, CheckpointBeacon, DataRequestReport, Epoch, Hash, Hashable,
    InventoryEntry, InventoryItem, OutputPointer, Transaction, TransactionsPool,
    UnspentOutputsPool,
};

use crate::actors::chain_manager::validations::block_reward;
use witnet_storage::{error::StorageError, storage::Storable};
use witnet_util::error::WitnetError;

mod actor;
mod data_request;
mod handlers;
mod mining;
mod validations;

/// Messages for ChainManager
pub mod messages;

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

////////////////////////////////////////////////////////////////////////////////////////
// ACTOR BASIC STRUCTURE
////////////////////////////////////////////////////////////////////////////////////////
/// ChainManager actor
#[derive(Default)]
pub struct ChainManager {
    /// Flag indicating if network is ready
    network_ready: bool,
    /// Blockchain state data structure
    chain_state: ChainState,
    /// Map that relates an epoch with the hashes of the blocks for that epoch (blocks index)
    // One epoch can have more than one block
    block_chain: Blockchain,
    /// Map that stores blocks by their hash
    blocks: HashMap<Hash, Block>,
    /// Map that stores blocks without validation by their hash
    blocks_to_validate: HashMap<Hash, Block>,
    /// Current Epoch
    current_epoch: Option<Epoch>,
    /// Transactions Pool (_mempool_)
    transactions_pool: TransactionsPool,
    /// Candidate to update chain_info, unspent_outputs_pool and transactions_pool in the next epoch
    best_candidate: Option<Candidate>,
    /// Maximum weight each block can have
    max_block_weight: u32,
    // Random value to help with debugging because there is no signature
    // and all the mined blocks have the same hash.
    // This random value helps to distinguish blocks mined on different nodes
    // To be removed when we implement real signing.
    random: u64,
    /// Mining enabled
    mining_enabled: bool,
    /// Hash of the genesis block
    genesis_block_hash: Hash,
    /// Pool of active data requests
    data_request_pool: DataRequestPool,
}

/// Struct that keeps a block candidate and its modifications in the blockchain
#[derive(Debug)]
pub struct Candidate {
    block: Block,
    utxo_set: UnspentOutputsPool,
    data_request_pool: DataRequestPool,
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

    /// Method to persist `block_chain` into Storage
    fn persist_block_chain(&self, ctx: &mut Context<Self>) {
        // Get StorageManager address
        let storage_manager_addr = System::current().registry().get::<StorageManager>();

        // Persist block_chain into storage. `AsyncContext::wait` registers
        // future within context, but context waits until this future resolves
        // before processing any other events.
        let msg = Put::from_value(BLOCK_CHAIN_KEY, &self.block_chain).unwrap();
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

    fn accept_block(&mut self, block: Block) -> Result<Hash, ChainManagerError> {
        // Calculate the hash of the block
        let hash: Hash = block.hash();

        // Check if we already have a block with that hash
        if let Some(_block) = self.blocks.get(&hash) {
            Err(ChainManagerError::BlockAlreadyExists)
        } else {
            // This is a new block, insert it into the internal maps
            {
                // Insert the new block into the map that relates epochs to block hashes
                let beacon = &block.block_header.beacon;
                let hash_set = &mut self
                    .block_chain
                    .entry(beacon.checkpoint)
                    .or_insert_with(HashSet::new);
                hash_set.insert(hash);

                info!(
                    "{} Epoch #{} has {} block candidates now",
                    Purple.bold().paint("[Chain]"),
                    Purple.bold().paint(beacon.checkpoint.to_string()),
                    Purple.bold().paint(hash_set.len().to_string())
                );
            }

            // Insert the new block into the map of known blocks
            self.blocks.insert(hash, block);

            Ok(hash)
        }
    }

    fn keep_block_without_validation(&mut self, block: Block) -> Result<Hash, ChainManagerError> {
        // Calculate the hash of the block
        let hash: Hash = block.hash();

        // Check if we already have a block with that hash
        if let Some(_block) = self.blocks_to_validate.get(&hash) {
            Err(ChainManagerError::BlockAlreadyExists)
        } else {
            // Insert the new block into the map of blocks to validate
            self.blocks_to_validate.insert(hash, block);

            debug!(
                "{} There are {} blocks to validate now",
                Purple.bold().paint("[Chain]"),
                Purple
                    .bold()
                    .paint(self.blocks_to_validate.len().to_string())
            );

            Ok(hash)
        }
    }

    fn broadcast_announce_items(&mut self, hash: Hash) {
        // Get SessionsManager address
        let sessions_manager_addr = System::current().registry().get::<SessionsManager>();

        // Tell SessionsManager to announce the new block through every consolidated Session
        let items = vec![InventoryEntry::Block(hash)];
        sessions_manager_addr.do_send(Broadcast {
            command: AnnounceItems { items },
        });
    }

    fn broadcast_item(&self, item: InventoryItem) {
        // Get SessionsManager address
        let sessions_manager_addr = System::current().registry().get::<SessionsManager>();

        sessions_manager_addr.do_send(Broadcast {
            command: SendInventoryItem { item },
        });
    }

    fn request_block(&mut self, block_hash: Hash) {
        // Get SessionsManager address
        let sessions_manager_addr = System::current().registry().get::<SessionsManager>();

        let block_entry = InventoryEntry::Block(block_hash);
        sessions_manager_addr.do_send(Anycast {
            command: RequestBlock { block_entry },
        });
    }

    fn try_to_get_block(&mut self, hash: Hash) -> Result<Block, ChainManagerError> {
        // Check if we have a block with that hash
        self.blocks.get(&hash).map_or_else(
            || Err(ChainManagerError::BlockDoesNotExist),
            |block| Ok(block.clone()),
        )
    }

    fn discard_existing_inventory_entries(
        &mut self,
        inv_entries: Vec<InventoryEntry>,
    ) -> InventoryEntriesResult {
        // Missing inventory entries
        let missing_inv_entries = inv_entries
            .into_iter()
            .filter(|inv_entry| {
                // Get hash from inventory vector
                let hash = match inv_entry {
                    InventoryEntry::Error(hash)
                    | InventoryEntry::Block(hash)
                    | InventoryEntry::Tx(hash)
                    | InventoryEntry::DataRequest(hash)
                    | InventoryEntry::DataResult(hash) => hash,
                };

                // Add the inventory vector to the missing vectors if it is not found
                self.blocks.get(&hash).is_none()
            })
            .collect();

        Ok(missing_inv_entries)
    }

    fn process_block(&mut self, ctx: &mut Context<Self>, block: Block) {
        // Block verification process
        let reputation_manager_addr = System::current().registry().get::<ReputationManager>();

        let block_epoch = block.block_header.beacon.checkpoint;
        let hash_prev_block = block.block_header.beacon.hash_prev_block;

        //Discard blocks whose hash is bigger or equal than the candidate
        let our_candidate_is_better = Some(block_epoch) == self.current_epoch
            && match self.best_candidate.as_ref() {
                Some(candidate) => candidate.block.hash() <= block.hash(),
                None => false,
            };

        self.current_epoch
            .map(|current_epoch| {
                // Remove from blocks_to_validate HashMap if it exists
                self.blocks_to_validate.remove(&block.hash());

                // Check beforehand if a block exists to validate
                if let Some(previous_block) = self.blocks_to_validate.remove(&hash_prev_block) {
                    self.process_block(ctx, previous_block);
                }

                if !validate_merkle_tree(&block) {
                    warn!("Block merkle tree not valid");
                } else if block_epoch > current_epoch {
                    warn!(
                        "Block epoch from the future: current: {}, block: {}",
                        current_epoch, block_epoch
                    );
                } else if self.chain_state.chain_info.is_some() && self.chain_state.chain_info.as_ref().unwrap().highest_block_checkpoint.checkpoint > block_epoch {
                    warn!(
                        "Block epoch {} older than highest block checkpoint {}",
                        block_epoch, self.chain_state.chain_info.as_ref().unwrap().highest_block_checkpoint.checkpoint
                    );
                } else if our_candidate_is_better {
                    if let Some(candidate) = self.best_candidate.as_ref() {
                        debug!(
                            "We already had a better candidate ({:?} overpowers {:?})",
                            candidate.block.hash(),
                            block.hash()
                        );
                    }
                } else if hash_prev_block != self.genesis_block_hash && !self.blocks.contains_key(&hash_prev_block) {
//                } else if hash_prev_block != self.genesis_block_hash &&
//                    !self.block_chain.get(&block_epoch).map(|x| x.contains(&hash_prev_block)).unwrap_or(false) {

                    // Request the lost block with the hash_prev_block indicated in this block
                    // Except when that block is the genesis block
                    self.request_block(hash_prev_block);

                    match self.keep_block_without_validation(block) {
                        Ok(hash) => warn!(
                            "Block [{:?}] has a previous hash [{:?}] not known",
                            hash, hash_prev_block
                        ),
                        Err(ChainManagerError::BlockAlreadyExists) => {
                            warn!("Block without previous hash known already exists in blocks_to_validate");
                        }
                        Err(_) => {
                            error!("Unexpected error");
                        }
                    }
                } else if let Err(e) = block.validate(block_reward(block_epoch), &self.chain_state.unspent_outputs_pool) {
                    warn!("Block's mint transaction is not valid: {:?}", e);
                } else {
                    if block_epoch < current_epoch {
                        // FIXME(#235): check proof of eligibility from the past
                        // ReputationManager should have a method to validate PoE from a past epoch
                        warn!(
                            "Block epoch from the past: current: {}, block: {}",
                            current_epoch, block_epoch
                        );
                    }
                    // Request proof of eligibility validation to ReputationManager
                    reputation_manager_addr
                        .send(ValidatePoE {
                            beacon: block.block_header.beacon,
                            proof: block.proof.clone(),
                        })
                        .into_actor(self)
                        .then(|res, act, ctx| {
                            act.process_poe_validation_response(res, ctx, block);

                            actix::fut::ok(())
                        })
                        .wait(ctx);
                }
            })
            .unwrap_or_else(|| {
                warn!("ChainManager doesn't have current epoch");
            });
    }

    fn update_transaction_pool(&mut self, transactions: &[Transaction]) {
        for transaction in transactions {
            self.transactions_pool.remove(&transaction.hash());
        }
    }

    fn process_poe_validation_response(
        &mut self,
        res: Result<bool, MailboxError>,
        ctx: &mut Context<Self>,
        block: Block,
    ) {
        match res {
            Err(e) => {
                // Error when sending message
                error!("Unsuccessful communication with reputation manager: {}", e);
            }
            Ok(false) => {
                warn!("Block PoE not valid");
            }
            Ok(true) => {
                let mut utxo_set = self.chain_state.unspent_outputs_pool.clone();
                let mut data_request_pool = self.data_request_pool.clone();

                if validate_transactions(
                    &mut utxo_set,
                    &self.transactions_pool,
                    &mut data_request_pool,
                    &block,
                ) {
                    // Insert in blocks mempool
                    let res = self.accept_block(block.clone());
                    match res {
                        Ok(hash) => {
                            let block_epoch = block.block_header.beacon.checkpoint;
                            if Some(block_epoch) == self.current_epoch {
                                // Update block candidate
                                self.best_candidate = Some(Candidate {
                                    block: block.clone(),
                                    utxo_set,
                                    data_request_pool,
                                });
                                //Broadcast blocks in current epoch
                                self.broadcast_item(InventoryItem::Block(block));
                            } else {
                                //TODO: Now we assume there are no forked older blocks

                                // Announce and persist older blocks
                                self.broadcast_announce_items(hash);

                                // Persist block item
                                self.persist_item(ctx, InventoryItem::Block(block.clone()));

                                // Update utxo set with block's transactions
                                self.chain_state.unspent_outputs_pool =
                                    self.chain_state.generate_unspent_outputs_pool(&block);
                                self.update_transaction_pool(block.txns.as_ref());

                                // Update chain_info
                                match self.chain_state.chain_info.as_mut() {
                                    Some(chain_info) => {
                                        let beacon = CheckpointBeacon {
                                            checkpoint: block_epoch,
                                            hash_prev_block: hash,
                                        };

                                        chain_info.highest_block_checkpoint = beacon;
                                    }

                                    None => {
                                        error!("No ChainInfo loaded in ChainManager");
                                    }
                                }
                            }
                        }
                        Err(ChainManagerError::BlockAlreadyExists) => {
                            warn!("Block already exists");
                        }
                        Err(_) => {
                            error!("Unexpected error");
                        }
                    };
                } else {
                    warn!("Transactions not valid")
                }
            }
        };
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn add_block() {
        let mut bm = ChainManager::default();

        // Build hardcoded block
        let checkpoint = 2;
        let block_a = build_hardcoded_block(checkpoint, 99999, Hash::SHA256([111; 32]));

        // Add block to ChainManager
        let hash_a = bm.accept_block(block_a.clone()).unwrap();

        // Check the block is added into the blocks map
        assert_eq!(bm.blocks.len(), 1);
        assert_eq!(bm.blocks.get(&hash_a).unwrap(), &block_a);

        // Check the block is added into the epoch-to-hash map
        assert_eq!(bm.block_chain.get(&checkpoint).unwrap().len(), 1);
        assert_eq!(
            bm.block_chain
                .get(&checkpoint)
                .unwrap()
                .iter()
                .next()
                .unwrap(),
            &hash_a
        );
    }

    #[test]
    fn add_same_block_twice() {
        let mut bm = ChainManager::default();

        // Build hardcoded block
        let block = build_hardcoded_block(2, 99999, Hash::SHA256([111; 32]));

        // Only the first block will be inserted
        assert!(bm.accept_block(block.clone()).is_ok());
        assert!(bm.accept_block(block).is_err());
        assert_eq!(bm.blocks.len(), 1);
    }

    #[test]
    fn add_blocks_same_epoch() {
        let mut bm = ChainManager::default();

        // Build hardcoded blocks
        let checkpoint = 2;
        let block_a = build_hardcoded_block(checkpoint, 99999, Hash::SHA256([111; 32]));
        let block_b = build_hardcoded_block(checkpoint, 12345, Hash::SHA256([112; 32]));

        // Add blocks to the ChainManager
        let hash_a = bm.accept_block(block_a).unwrap();
        let hash_b = bm.accept_block(block_b).unwrap();
        assert_ne!(hash_a, hash_b);

        // Check that both blocks are stored in the same epoch
        assert_eq!(bm.block_chain.get(&checkpoint).unwrap().len(), 2);
        assert!(bm.block_chain.get(&checkpoint).unwrap().contains(&hash_a));
        assert!(bm.block_chain.get(&checkpoint).unwrap().contains(&hash_b));
    }

    #[test]
    fn get_existing_block() {
        // Create empty ChainManager
        let mut bm = ChainManager::default();

        // Create a hardcoded block
        let block_a = build_hardcoded_block(2, 99999, Hash::SHA256([111; 32]));

        // Add the block to the ChainManager
        let hash_a = bm.accept_block(block_a.clone()).unwrap();

        // Try to get the block from the ChainManager
        let stored_block = bm.try_to_get_block(hash_a).unwrap();

        assert_eq!(stored_block, block_a);
    }

    #[test]
    fn get_non_existent_block() {
        // Create empty ChainManager
        let mut bm = ChainManager::default();

        // Try to get a block with an invented hash
        let result = bm.try_to_get_block(Hash::SHA256([1; 32]));

        // Check that an error was obtained
        assert!(result.is_err());
    }

    #[test]
    fn add_block_to_validate() {
        let mut bm = ChainManager::default();

        // Build hardcoded block
        let checkpoint = 2;
        let hash_prev_block = Hash::SHA256([111; 32]);
        let block_a = build_hardcoded_block(checkpoint, 99999, hash_prev_block);

        // Add block to ChainManager without complete validation
        let hash_a = bm.keep_block_without_validation(block_a.clone()).unwrap();

        // Check the block is added into the blocks_to_validate map
        assert_eq!(bm.blocks_to_validate.len(), 1);
        assert_eq!(bm.blocks_to_validate.get(&hash_a).unwrap(), &block_a);
    }

    #[test]
    fn add_same_block_to_validate_twice() {
        let mut bm = ChainManager::default();

        // Build hardcoded block
        let block = build_hardcoded_block(2, 99999, Hash::SHA256([111; 32]));

        // Only the first block will be inserted
        assert!(bm.keep_block_without_validation(block.clone()).is_ok());
        assert!(bm.keep_block_without_validation(block).is_err());
        assert_eq!(bm.blocks_to_validate.len(), 1);
    }

    #[test]
    fn add_blocks_to_validate_same_previous_hash() {
        let mut bm = ChainManager::default();

        // Build hardcoded blocks
        let checkpoint = 2;
        let hash_prev_block = Hash::SHA256([111; 32]);
        let block_a = build_hardcoded_block(checkpoint, 99999, hash_prev_block);
        let block_b = build_hardcoded_block(checkpoint, 12345, hash_prev_block);

        // Add blocks to the ChainManager without complete validation
        let hash_a = bm.keep_block_without_validation(block_a).unwrap();
        let hash_b = bm.keep_block_without_validation(block_b).unwrap();
        assert_ne!(hash_a, hash_b);
    }

    #[test]
    fn discard_all() {
        // Create empty ChainManager
        let mut bm = ChainManager::default();

        // Build blocks
        let block_a = build_hardcoded_block(2, 99999, Hash::SHA256([111; 32]));
        let block_b = build_hardcoded_block(1, 10000, Hash::SHA256([112; 32]));
        let block_c = build_hardcoded_block(3, 72138, Hash::SHA256([113; 32]));

        // Add blocks to the ChainManager
        let hash_a = bm.accept_block(block_a.clone()).unwrap();
        let hash_b = bm.accept_block(block_b.clone()).unwrap();
        let hash_c = bm.accept_block(block_c.clone()).unwrap();

        // Build vector of inventory entries from hashes
        let mut inv_entries = Vec::new();
        inv_entries.push(InventoryEntry::Block(hash_a));
        inv_entries.push(InventoryEntry::Block(hash_b));
        inv_entries.push(InventoryEntry::Block(hash_c));

        // Filter inventory entries
        let missing_inv_entries = bm.discard_existing_inventory_entries(inv_entries).unwrap();

        // Check there is no missing inventory entry
        assert!(missing_inv_entries.is_empty());
    }

    #[test]
    fn discard_some() {
        // Create empty ChainManager
        let mut bm = ChainManager::default();

        // Build blocks
        let block_a = build_hardcoded_block(2, 99999, Hash::SHA256([111; 32]));
        let block_b = build_hardcoded_block(1, 10000, Hash::SHA256([112; 32]));
        let block_c = build_hardcoded_block(3, 72138, Hash::SHA256([113; 32]));

        // Add blocks to the ChainManager
        let hash_a = bm.accept_block(block_a.clone()).unwrap();
        let hash_b = bm.accept_block(block_b.clone()).unwrap();
        let hash_c = bm.accept_block(block_c.clone()).unwrap();

        // Missing inventory vector
        let missing_inv_entries = InventoryEntry::Block(Hash::SHA256([1; 32]));

        // Build vector of inventory vectors from hashes
        let mut inv_entries = Vec::new();
        inv_entries.push(InventoryEntry::Block(hash_a));
        inv_entries.push(InventoryEntry::Block(hash_b));
        inv_entries.push(InventoryEntry::Block(hash_c));
        inv_entries.push(missing_inv_entries.clone());

        // Filter inventory vectors
        let expected_missing_inv_entries =
            bm.discard_existing_inventory_entries(inv_entries).unwrap();

        // Check the expected missing inventory vectors
        assert_eq!(vec![missing_inv_entries], expected_missing_inv_entries);
    }

    #[test]
    fn discard_none() {
        // Create empty ChainManager
        let mut bm = ChainManager::default();

        // Build blocks
        let block_a = build_hardcoded_block(2, 99999, Hash::SHA256([111; 32]));
        let block_b = build_hardcoded_block(1, 10000, Hash::SHA256([112; 32]));
        let block_c = build_hardcoded_block(3, 72138, Hash::SHA256([113; 32]));

        // Add blocks to the ChainManager
        bm.accept_block(block_a.clone()).unwrap();
        bm.accept_block(block_b.clone()).unwrap();
        bm.accept_block(block_c.clone()).unwrap();

        // Missing inventory vector
        let missing_inv_entries_1 = InventoryEntry::Block(Hash::SHA256([1; 32]));
        let missing_inv_entries_2 = InventoryEntry::Block(Hash::SHA256([2; 32]));
        let missing_inv_entries_3 = InventoryEntry::Block(Hash::SHA256([3; 32]));

        // Build vector of missing inventory vectors from hashes
        let mut inv_entries = Vec::new();
        inv_entries.push(missing_inv_entries_1);
        inv_entries.push(missing_inv_entries_2);
        inv_entries.push(missing_inv_entries_3);

        // Filter inventory vectors
        let missing_inv_entries = bm
            .discard_existing_inventory_entries(inv_entries.clone())
            .unwrap();

        // Check there is no missing inventory vector
        assert_eq!(missing_inv_entries, inv_entries);
    }

    #[cfg(test)]
    fn build_hardcoded_block(checkpoint: u32, influence: u64, hash_prev_block: Hash) -> Block {
        use witnet_data_structures::chain::*;
        let signature = Signature::Secp256k1(Secp256k1Signature {
            r: [0; 32],
            s: [0; 32],
            v: 0,
        });
        let keyed_signature = vec![KeyedSignature {
            public_key: [0; 32],
            signature: signature.clone(),
        }];

        let reveal_input = Input::Reveal(RevealInput {
            output_index: 0,
            transaction_id: Hash::SHA256([0; 32]),
        });

        let commit_input = Input::Commit(CommitInput {
            nonce: 0,
            output_index: 0,
            reveal: [0; 32].to_vec(),
            transaction_id: Hash::SHA256([0; 32]),
        });

        let data_request_input = Input::DataRequest(DataRequestInput {
            output_index: 0,
            poe: [0; 32],
            transaction_id: Hash::SHA256([0; 32]),
        });
        let value_transfer_output = Output::ValueTransfer(ValueTransferOutput {
            pkh: [0; 20],
            value: 0,
        });
        let rad_aggregate = RADAggregate { script: vec![0] };
        let rad_retrieve_1 = RADRetrieve {
            kind: RADType::HttpGet,
            url: "https://openweathermap.org/data/2.5/weather?id=2950159&appid=b6907d289e10d714a6e88b30761fae22".to_string(),
            script: vec![0],
        };
        let rad_retrieve_2 = RADRetrieve {
            kind: RADType::HttpGet,
            url: "https://openweathermap.org/data/2.5/weather?id=2950159&appid=b6907d289e10d714a6e88b30761fae22".to_string(),
            script: vec![0],
        };
        let rad_consensus = RADConsensus { script: vec![0] };
        let rad_deliver_1 = RADDeliver {
            kind: RADType::HttpGet,
            url: "https://hooks.zapier.com/hooks/catch/3860543/l2awcd/".to_string(),
        };
        let rad_deliver_2 = RADDeliver {
            kind: RADType::HttpGet,
            url: "https://hooks.zapier.com/hooks/catch/3860543/l1awcw/".to_string(),
        };
        let rad_request = RADRequest {
            aggregate: rad_aggregate,
            not_before: 0,
            retrieve: vec![rad_retrieve_1, rad_retrieve_2],
            consensus: rad_consensus,
            deliver: vec![rad_deliver_1, rad_deliver_2],
        };
        let data_request_output = Output::DataRequest(DataRequestOutput {
            backup_witnesses: 0,
            commit_fee: 0,
            data_request: rad_request,
            pkh: [0; 20],
            reveal_fee: 0,
            tally_fee: 0,
            time_lock: 0,
            value: 0,
            witnesses: 0,
        });
        let commit_output = Output::Commit(CommitOutput {
            commitment: Hash::SHA256([0; 32]),
            value: 0,
        });
        let reveal_output = Output::Reveal(RevealOutput {
            pkh: [0; 20],
            reveal: [0; 32].to_vec(),
            value: 0,
        });
        let consensus_output = Output::Tally(TallyOutput {
            pkh: [0; 20],
            result: [0; 32].to_vec(),
            value: 0,
        });

        let inputs = vec![commit_input, data_request_input, reveal_input];
        let outputs = vec![
            value_transfer_output,
            data_request_output,
            commit_output,
            reveal_output,
            consensus_output,
        ];
        let txns: Vec<Transaction> = vec![Transaction {
            inputs,
            signatures: keyed_signature,
            outputs,
            version: 0,
        }];
        let proof = LeadershipProof {
            block_sig: Some(signature),
            influence,
        };

        Block {
            block_header: BlockHeader {
                version: 1,
                beacon: CheckpointBeacon {
                    checkpoint,
                    hash_prev_block,
                },
                hash_merkle_root: Hash::SHA256([222; 32]),
            },
            proof,
            txns,
        }
    }

    #[test]
    fn block_storable() {
        use witnet_data_structures::chain::*;
        use witnet_storage::storage::Storable;

        let b = InventoryItem::Block(build_hardcoded_block(0, 0, Hash::SHA256([111; 32])));
        let msp = b.to_bytes().unwrap();
        assert_eq!(InventoryItem::from_bytes(&msp).unwrap(), b);

        println!("{:?}", b);
        println!("{:?}", msp);
        /*
        use witnet_data_structures::chain::Hash::SHA256;
        use witnet_data_structures::chain::Signature::Secp256k1;
        let mined_block = InventoryItem::Block(Block {
            block_header: BlockHeader {
                version: 0,
                beacon: CheckpointBeacon {
                    checkpoint: 400,
                    hash_prev_block: SHA256([
                        47, 17, 139, 130, 7, 164, 151, 185, 64, 43, 88, 183, 53, 213, 38, 89, 76,
                        66, 231, 53, 78, 216, 230, 217, 245, 184, 150, 33, 182, 15, 111, 38,
                    ]),
                },
                hash_merkle_root: SHA256([
                    227, 176, 196, 66, 152, 252, 28, 20, 154, 251, 244, 200, 153, 111, 185, 36, 39,
                    174, 65, 228, 100, 155, 147, 76, 164, 149, 153, 27, 120, 82, 184, 85,
                ]),
            },
            proof: LeadershipProof {
                block_sig: Some(Secp256k1(Secp256k1Signature {
                    r: [
                        128, 205, 5, 48, 74, 223, 4, 72, 223, 231, 60, 90, 128, 196, 37, 74, 225,
                        60, 123, 112, 167, 2, 28, 201, 210, 41, 9, 128, 136, 223, 228, 35,
                    ],
                    s: [
                        128, 205, 5, 48, 74, 223, 4, 72, 223, 231, 60, 90, 128, 196, 37, 74, 225,
                        60, 123, 112, 167, 2, 28, 201, 210, 41, 9, 128, 136, 223, 228, 35,
                    ],
                    v: 0,
                })),
                influence: 0,
            },
            txns: vec![],
        });
        let raw_block = [146, 1, 145, 147, 147, 0, 146, 205, 1, 144, 146, 0, 145, 220, 0, 32, 47, 17, 204, 139, 204, 130, 7, 204, 164, 204, 151, 204, 185, 64, 43, 88, 204, 183, 53, 204, 213, 38, 89, 76, 66, 204, 231, 53, 78, 204, 216, 204, 230, 204, 217, 204, 245, 204, 184, 204, 150, 33, 204, 182, 15, 111, 38, 146, 0, 145, 220, 0, 32, 204, 227, 204, 176, 204, 196, 66, 204, 152, 204, 252, 28, 20, 204, 154, 204, 251, 204, 244, 204, 200, 204, 153, 111, 204, 185, 36, 39, 204, 174, 65, 204, 228, 100, 204, 155, 204, 147, 76, 204, 164, 204, 149, 204, 153, 27, 120, 82, 204, 184, 85, 146, 146, 0, 145, 147, 220, 0, 32, 204, 128, 204, 205, 5, 48, 74, 204, 223, 4, 72, 204, 223, 204, 231, 60, 90, 204, 128, 204, 196, 37, 74, 204, 225, 60, 123, 112, 204, 167, 2, 28, 204, 201, 204, 210, 41, 9, 204, 128, 204, 136, 204, 223, 204, 228, 35, 220, 0, 32, 204, 128, 204, 205, 5, 48, 74, 204, 223, 4, 72, 204, 223, 204, 231, 60, 90, 204, 128, 204, 196, 37, 74, 204, 225, 60, 123, 112, 204, 167, 2, 28, 204, 201, 204, 210, 41, 9, 204, 128, 204, 136, 204, 223, 204, 228, 35, 0, 0, 144];
        println!("{:?}", mined_block);
        println!("Mined block to bytes:");
        println!("{:?}", mined_block.to_bytes());
        println!("Mined block bytes from storage:");
        println!("{:?}", &raw_block[..]);
        assert_eq!(InventoryItem::from_bytes(&raw_block).unwrap(), mined_block);
        */
    }

    #[test]
    fn block_storable_fail() {
        use witnet_data_structures::chain::Hash::SHA256;
        use witnet_data_structures::chain::Signature::Secp256k1;
        use witnet_data_structures::chain::*;
        use witnet_storage::storage::Storable;

        let mined_block = InventoryItem::Block(Block {
            block_header: BlockHeader {
                version: 0,
                beacon: CheckpointBeacon {
                    checkpoint: 400,
                    hash_prev_block: SHA256([
                        47, 17, 139, 130, 7, 164, 151, 185, 64, 43, 88, 183, 53, 213, 38, 89, 76,
                        66, 231, 53, 78, 216, 230, 217, 245, 184, 150, 33, 182, 15, 111, 38,
                    ]),
                },
                hash_merkle_root: SHA256([
                    227, 176, 196, 66, 152, 252, 28, 20, 154, 251, 244, 200, 153, 111, 185, 36, 39,
                    174, 65, 228, 100, 155, 147, 76, 164, 149, 153, 27, 120, 82, 184, 85,
                ]),
            },
            proof: LeadershipProof {
                block_sig: Some(Secp256k1(Secp256k1Signature {
                    r: [
                        128, 205, 5, 48, 74, 223, 4, 72, 223, 231, 60, 90, 128, 196, 37, 74, 225,
                        60, 123, 112, 167, 2, 28, 201, 210, 41, 9, 128, 136, 223, 228, 35,
                    ],
                    s: [
                        128, 205, 5, 48, 74, 223, 4, 72, 223, 231, 60, 90, 128, 196, 37, 74, 225,
                        60, 123, 112, 167, 2, 28, 201, 210, 41, 9, 128, 136, 223, 228, 35,
                    ],
                    v: 0,
                })),
                influence: 0,
            },
            txns: vec![],
        });
        let msp = mined_block.to_bytes().unwrap();

        assert_eq!(InventoryItem::from_bytes(&msp).unwrap(), mined_block);
    }

    #[test]
    fn leadership_storable() {
        use witnet_data_structures::chain::*;
        use witnet_storage::storage::Storable;
        let signed_beacon_hash = [4; 32];

        let signature = Signature::Secp256k1(Secp256k1Signature {
            r: signed_beacon_hash,
            s: signed_beacon_hash,
            v: 0,
        });
        let a = LeadershipProof {
            block_sig: Some(signature),
            influence: 0,
        };

        let msp = a.to_bytes().unwrap();

        assert_eq!(LeadershipProof::from_bytes(&msp).unwrap(), a);
    }

    #[test]
    fn signature_storable() {
        use witnet_data_structures::chain::*;
        use witnet_storage::storage::Storable;
        let signed_beacon_hash = [4; 32];

        let a = Some(Signature::Secp256k1(Secp256k1Signature {
            r: signed_beacon_hash,
            s: signed_beacon_hash,
            v: 0,
        }));
        let msp = a.to_bytes().unwrap();

        assert_eq!(Option::<Signature>::from_bytes(&msp).unwrap(), a);
    }

    #[test]
    fn som_de_uno() {
        use witnet_storage::storage::Storable;

        let a = Some(Some(1u8));
        let msp = a.to_bytes().unwrap();
        assert_eq!(Option::<Option<u8>>::from_bytes(&msp).unwrap(), a);
    }

    #[test]
    fn empty_chain_state_to_bytes() {
        use witnet_storage::storage::Storable;

        let chain_state = ChainState::default();

        assert!(chain_state.to_bytes().is_ok());
    }

    #[test]
    fn chain_state_to_bytes() {
        use witnet_data_structures::chain::*;
        use witnet_storage::storage::Storable;

        let chain_state = ChainState {
            chain_info: Some(ChainInfo {
                environment: Environment::Mainnet,
                consensus_constants: ConsensusConstants {
                    checkpoint_zero_timestamp: 0,
                    checkpoints_period: 0,
                    genesis_hash: Hash::default(),
                    reputation_demurrage: 0.0,
                    reputation_punishment: 0.0,
                    max_block_weight: 0,
                },
                highest_block_checkpoint: CheckpointBeacon {
                    checkpoint: 0,
                    hash_prev_block: Hash::default(),
                },
            }),
            unspent_outputs_pool: UnspentOutputsPool::default(),
            data_request_pool: ActiveDataRequestPool::default(),
        };

        assert!(chain_state.to_bytes().is_ok());
    }

    #[test]
    fn chain_state_with_chain_info_to_bytes() {
        use witnet_data_structures::chain::*;
        use witnet_storage::storage::Storable;

        let chain_state = ChainState {
            chain_info: Some(ChainInfo {
                environment: Environment::Testnet1,
                consensus_constants: ConsensusConstants {
                    checkpoint_zero_timestamp: 1546427376,
                    checkpoints_period: 10,
                    genesis_hash: Hash::SHA256([
                        0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
                        0, 0, 0, 0, 0, 0, 0,
                    ]),
                    reputation_demurrage: 0.0,
                    reputation_punishment: 0.0,
                    max_block_weight: 10000,
                },
                highest_block_checkpoint: CheckpointBeacon {
                    checkpoint: 122533,
                    hash_prev_block: Hash::SHA256([
                        239, 173, 3, 247, 9, 44, 43, 68, 13, 51, 67, 110, 79, 191, 165, 135, 157,
                        167, 155, 126, 49, 39, 120, 119, 206, 75, 15, 74, 97, 167, 220, 214,
                    ]),
                },
            }),
            unspent_outputs_pool: UnspentOutputsPool::default(),
            data_request_pool: ActiveDataRequestPool::default(),
        };

        assert!(chain_state.to_bytes().is_ok());
    }

    #[test]
    fn chain_state_with_utxo_to_bytes() {
        use witnet_data_structures::chain::*;
        use witnet_storage::storage::Storable;

        let mut utxo = UnspentOutputsPool::default();
        utxo.insert(
            OutputPointer {
                transaction_id: Hash::SHA256([
                    191, 75, 125, 95, 27, 78, 216, 89, 168, 222, 88, 21, 171, 139, 44, 170, 127,
                    120, 139, 142, 98, 209, 129, 129, 16, 52, 0, 62, 43, 116, 67, 245,
                ]),
                output_index: 0,
            },
            Output::ValueTransfer(ValueTransferOutput {
                pkh: [0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0],
                value: 50000000000,
            }),
        );

        let chain_state = ChainState {
            chain_info: Some(ChainInfo {
                environment: Environment::Testnet1,
                consensus_constants: ConsensusConstants {
                    checkpoint_zero_timestamp: 1546427376,
                    checkpoints_period: 10,
                    genesis_hash: Hash::SHA256([
                        0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
                        0, 0, 0, 0, 0, 0, 0,
                    ]),
                    reputation_demurrage: 0.0,
                    reputation_punishment: 0.0,
                    max_block_weight: 10000,
                },
                highest_block_checkpoint: CheckpointBeacon {
                    checkpoint: 122533,
                    hash_prev_block: Hash::SHA256([
                        239, 173, 3, 247, 9, 44, 43, 68, 13, 51, 67, 110, 79, 191, 165, 135, 157,
                        167, 155, 126, 49, 39, 120, 119, 206, 75, 15, 74, 97, 167, 220, 214,
                    ]),
                },
            }),
            unspent_outputs_pool: utxo,
            data_request_pool: ActiveDataRequestPool::default(),
        };

        assert!(chain_state.to_bytes().is_ok());
    }

    #[test]
    fn utxo_to_bytes() {
        use witnet_data_structures::chain::*;
        use witnet_storage::storage::Storable;

        let mut utxo = UnspentOutputsPool::default();
        utxo.insert(
            OutputPointer {
                transaction_id: Hash::SHA256([
                    191, 75, 125, 95, 27, 78, 216, 89, 168, 222, 88, 21, 171, 139, 44, 170, 127,
                    120, 139, 142, 98, 209, 129, 129, 16, 52, 0, 62, 43, 116, 67, 245,
                ]),
                output_index: 0,
            },
            Output::ValueTransfer(ValueTransferOutput {
                pkh: [0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0],
                value: 50000000000,
            }),
        );

        assert!(utxo.to_bytes().is_ok());
    }

    #[test]
    fn output_pointer_to_bytes() {
        use witnet_data_structures::chain::*;
        use witnet_storage::storage::Storable;

        let output_pointer = OutputPointer {
            transaction_id: Hash::SHA256([
                191, 75, 125, 95, 27, 78, 216, 89, 168, 222, 88, 21, 171, 139, 44, 170, 127, 120,
                139, 142, 98, 209, 129, 129, 16, 52, 0, 62, 43, 116, 67, 245,
            ]),
            output_index: 0,
        };

        assert!(output_pointer.to_bytes().is_ok());
    }

    #[test]
    fn output_to_bytes() {
        use witnet_data_structures::chain::*;
        use witnet_storage::storage::Storable;

        let output = Output::ValueTransfer(ValueTransferOutput {
            pkh: [0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0],
            value: 50000000000,
        });

        assert!(output.to_bytes().is_ok());
    }
}
