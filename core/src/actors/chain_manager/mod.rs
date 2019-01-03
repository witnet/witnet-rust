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

use crate::actors::{
    chain_manager::messages::InventoryEntriesResult,
    inventory_manager::{messages::AddItem, InventoryManager},
    storage_keys::CHAIN_KEY,
    storage_manager::{messages::Put, StorageManager},
};

use log::{debug, error, info, warn};
use std::collections::{BTreeMap, HashMap, HashSet};
use witnet_data_structures::chain::{
    Block, BlockHeader, ChainInfo, Epoch, Hash, Hashable, InventoryEntry, InventoryItem, Output,
    OutputPointer, TransactionsPool,
};

use crate::actors::chain_manager::messages::BuildBlock;
use crate::actors::reputation_manager::{messages::ValidatePoE, ReputationManager};
use crate::actors::session::messages::AnnounceItems;
use crate::actors::sessions_manager::{messages::Broadcast, SessionsManager};

use crate::validations::{block_reward, merkle_tree_root, validate_coinbase, validate_merkle_tree};

use witnet_storage::error::StorageError;
use witnet_util::error::WitnetError;

mod actor;
mod handlers;

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
    /// Blockchain information data structure
    chain_info: Option<ChainInfo>,
    /// Map that relates an epoch with the hashes of the blocks for that epoch
    // One epoch can have more than one block
    epoch_to_block_hash: BTreeMap<Epoch, HashSet<Hash>>,
    /// Map that stores blocks by their hash
    blocks: HashMap<Hash, Block>,
    /// Current Epoch
    current_epoch: Option<Epoch>,
    /// Transactions Pool
    transactions_pool: TransactionsPool,
    /// Block candidate to update chain_info in the next epoch
    block_candidate: Option<Block>,
    /// Maximum weight each block can have
    max_block_weight: u32,
    /// Unspent Outputs Pool
    _unspent_outputs_pool: HashMap<OutputPointer, Output>,
}

/// Required trait for being able to retrieve ChainManager address from registry
impl Supervised for ChainManager {}

/// Required trait for being able to retrieve ChainManager address from registry
impl SystemService for ChainManager {}

/// Auxiliary methods for ChainManager actor
impl ChainManager {
    /// Method to persist chain_info into storage
    fn persist_chain_info(&self, ctx: &mut Context<Self>) {
        // Get StorageManager address
        let storage_manager_addr = System::current().registry().get::<StorageManager>();

        let chain_info = match self.chain_info.as_ref() {
            Some(x) => x,
            None => {
                error!("Trying to persist a None value");
                return;
            }
        };

        // Persist chain_info into storage. `AsyncContext::wait` registers
        // future within context, but context waits until this future resolves
        // before processing any other events.
        let msg = Put::from_value(CHAIN_KEY, chain_info).unwrap();
        storage_manager_addr
            .send(msg)
            .into_actor(self)
            .then(|res, _act, _ctx| {
                match res {
                    Ok(Ok(_)) => {
                        debug!("ChainManager successfully persisted chain_info into storage")
                    }
                    _ => {
                        error!("ChainManager failed to persist chain_info into storage");
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
                    .epoch_to_block_hash
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

    fn broadcast_block(&mut self, hash: Hash) {
        // Get SessionsManager's address
        let sessions_manager_addr = System::current().registry().get::<SessionsManager>();

        // Tell SessionsManager to announce the new block through every consolidated Session
        let items = vec![InventoryEntry::Block(hash)];
        sessions_manager_addr.do_send(Broadcast {
            command: AnnounceItems { items },
        });
    }

    fn try_to_get_block(&mut self, hash: Hash) -> Result<Block, ChainManagerError> {
        // Check if we have a block with that hash
        self.blocks.get(&hash).map_or_else(
            || Err(ChainManagerError::BlockDoesNotExist),
            |block| Ok(block.clone()),
        )
    }

    fn build_block(&self, msg: &BuildBlock) -> Block {
        // Get all the unspent transactions and calculate the sum of their fees
        let mut transaction_fees = 0;

        let max_block_weight = self.max_block_weight;
        let mut block_weight = 0;
        let mut transactions = Vec::new();

        for transaction in self.transactions_pool.iter() {
            // Currently, 1 weight unit is equivalent to 1 byte
            let transaction_weight = transaction.size();
            let transaction_fee = transaction.fee();
            let new_block_weight = block_weight + transaction_weight;

            if new_block_weight <= max_block_weight {
                transactions.push(transaction.clone());
                transaction_fees += transaction_fee;
                block_weight += transaction_weight;

                if new_block_weight == max_block_weight {
                    break;
                }
            }
        }

        let epoch = msg.beacon.checkpoint;
        let _reward = block_reward(epoch) + transaction_fees;
        // TODO: push coinbase transaction
        let beacon = msg.beacon;
        let hash_merkle_root = merkle_tree_root(&transactions);
        let block_header = BlockHeader {
            version: 0,
            beacon,
            hash_merkle_root,
        };
        let proof = msg.leadership_proof;

        Block {
            block_header,
            proof,
            txns: transactions,
        }
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
        // Block verify process
        let reputation_manager_addr = System::current().registry().get::<ReputationManager>();

        let block_epoch = block.block_header.beacon.checkpoint;

        let our_candidate_is_better = Some(block_epoch) == self.current_epoch
            && match self.block_candidate.as_ref() {
                Some(candidate) => candidate.hash() < block.hash(),
                None => false,
            };

        self.current_epoch
            .map(|current_epoch| {
                if !validate_coinbase(&block) {
                    warn!("Block coinbase not valid");
                } else if !validate_merkle_tree(&block) {
                    warn!("Block merkle tree not valid");
                } else if block_epoch > current_epoch {
                    warn!(
                        "Block epoch from the future: current: {}, block: {}",
                        current_epoch, block_epoch
                    );
                } else if our_candidate_is_better {
                    if let Some(candidate) = self.block_candidate.as_ref() {
                        debug!(
                            "We already had a better candidate ({:?} overpowers {:?})",
                            candidate.hash(),
                            block.hash()
                        );
                    }
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
                            proof: block.proof,
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
                // Insert in blocks mempool
                let res = self.accept_block(block.clone());
                match res {
                    Ok(hash) => {
                        self.broadcast_block(hash);

                        // Update block candidate
                        if Some(block.block_header.beacon.checkpoint) == self.current_epoch {
                            self.block_candidate = Some(block.clone());
                        }

                        // Save block to storage
                        // TODO: dont save the current candidate into storage
                        // Because it may not be the chosen block
                        // Add in Session a method to retrieve the block candidate
                        // before checking for blocks in storage
                        self.persist_item(ctx, InventoryItem::Block(block));
                    }
                    Err(ChainManagerError::BlockAlreadyExists) => {
                        warn!("Block already exists");
                    }
                    Err(_) => {
                        error!("Unexpected error");
                    }
                };
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
        let block_a = build_hardcoded_block(checkpoint, 99999);

        // Add block to ChainManager
        let hash_a = bm.accept_block(block_a.clone()).unwrap();

        // Check the block is added into the blocks map
        assert_eq!(bm.blocks.len(), 1);
        assert_eq!(bm.blocks.get(&hash_a).unwrap(), &block_a);

        // Check the block is added into the epoch-to-hash map
        assert_eq!(bm.epoch_to_block_hash.get(&checkpoint).unwrap().len(), 1);
        assert_eq!(
            bm.epoch_to_block_hash
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
        let block = build_hardcoded_block(2, 99999);

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
        let block_a = build_hardcoded_block(checkpoint, 99999);
        let block_b = build_hardcoded_block(checkpoint, 12345);

        // Add blocks to the ChainManager
        let hash_a = bm.accept_block(block_a).unwrap();
        let hash_b = bm.accept_block(block_b).unwrap();
        assert_ne!(hash_a, hash_b);

        // Check that both blocks are stored in the same epoch
        assert_eq!(bm.epoch_to_block_hash.get(&checkpoint).unwrap().len(), 2);
        assert!(bm
            .epoch_to_block_hash
            .get(&checkpoint)
            .unwrap()
            .contains(&hash_a));
        assert!(bm
            .epoch_to_block_hash
            .get(&checkpoint)
            .unwrap()
            .contains(&hash_b));
    }

    #[test]
    fn get_existing_block() {
        // Create empty ChainManager
        let mut bm = ChainManager::default();

        // Create a hardcoded block
        let block_a = build_hardcoded_block(2, 99999);

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
    fn discard_all() {
        // Create empty ChainManager
        let mut bm = ChainManager::default();

        // Build blocks
        let block_a = build_hardcoded_block(2, 99999);
        let block_b = build_hardcoded_block(1, 10000);
        let block_c = build_hardcoded_block(3, 72138);

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
        let block_a = build_hardcoded_block(2, 99999);
        let block_b = build_hardcoded_block(1, 10000);
        let block_c = build_hardcoded_block(3, 72138);

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
        let block_a = build_hardcoded_block(2, 99999);
        let block_b = build_hardcoded_block(1, 10000);
        let block_c = build_hardcoded_block(3, 72138);

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
    fn build_hardcoded_block(checkpoint: u32, influence: u64) -> Block {
        use witnet_data_structures::chain::*;
        let signature = Signature::Secp256k1(Secp256k1Signature {
            r: [0; 32],
            s: [0; 32],
            v: 0,
        });
        let keyed_signature = vec![KeyedSignature {
            public_key: [0; 32],
            signature,
        }];

        let value_transfer_input = Input::ValueTransfer(ValueTransferInput {
            output_index: 0,
            transaction_id: [0; 32],
        });

        let reveal_input = Input::Reveal(RevealInput {
            nonce: 0,
            output_index: 0,
            reveal: [0; 32],
            transaction_id: [0; 32],
        });
        let tally_input = Input::Tally(TallyInput {
            output_index: 0,
            transaction_id: [0; 32],
        });

        let commit_input = Input::Commit(CommitInput {
            output_index: 0,
            poe: [0; 32],
            transaction_id: [0; 32],
        });

        let value_transfer_output = Output::ValueTransfer(ValueTransferOutput {
            pkh: Hash::SHA256([0; 32]),
            value: 0,
        });

        let data_request_output = Output::DataRequest(DataRequestOutput {
            backup_witnesses: 0,
            commit_fee: 0,
            data_request: [0; 32],
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
            pkh: Hash::SHA256([0; 32]),
            reveal: [0; 32],
            value: 0,
        });
        let consensus_output = Output::Consensus(ConsensusOutput {
            pkh: Hash::SHA256([0; 32]),
            result: [0; 32],
            value: 0,
        });

        let inputs = vec![
            value_transfer_input,
            reveal_input,
            tally_input,
            commit_input,
        ];
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
                    hash_prev_block: Hash::SHA256([111; 32]),
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

        let b = InventoryItem::Block(build_hardcoded_block(0, 0));
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
}
