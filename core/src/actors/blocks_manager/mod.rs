//! # BlocksManager actor
//!
//! This module contains the BlocksManager actor which is in charge
//! of managing the blocks of the Witnet blockchain received through
//! the protocol. Among its responsabilities are the following:
//!
//! * Initializing the chain info upon running the node for the first time and persisting it into storage [StorageManager](actors::storage_manager::StorageManager)
//! * Recovering the chain info from storage and keeping it in its state.
//! * Validating block candidates as they come from a session.
//! * Consolidating multiple block candidates for the same checkpoint into a single valid block.
//! * Putting valid blocks into storage by sending them to the storage manager actor.
//! * Having a method for letting other components get blocks by *hash* or *checkpoint*.
//! * Having a method for letting other components get the epoch of the current tip of the
//! blockchain (e.g. the last epoch field required for the handshake in the Witnet network
//! protocol).
use actix::{
    ActorFuture, Context, ContextFutureSpawner, Supervised, System, SystemService, WrapFuture,
};

use witnet_data_structures::chain::ChainInfo;

use crate::actors::{
    storage_keys::CHAIN_KEY,
    storage_manager::{messages::Put, StorageManager},
};

use log::{debug, error, info};
use std::collections::HashMap;
use std::collections::HashSet;
use witnet_data_structures::chain::{Block, Epoch, Hash};

use witnet_storage::storage::Storable;

use witnet_crypto::hash::calculate_sha256;

mod actor;
mod handlers;

/// Messages for BlocksManager
pub mod messages;

/// Possible errors when getting the current epoch
#[derive(Copy, Clone, Debug, PartialEq)]
pub enum BlocksManagerError {
    /// The new block is not new anymore
    BlockAlreadyExists,
}

////////////////////////////////////////////////////////////////////////////////////////
// ACTOR BASIC STRUCTURE
////////////////////////////////////////////////////////////////////////////////////////
/// BlocksManager actor
#[derive(Default)]
pub struct BlocksManager {
    /// Blockchain information data structure
    chain_info: Option<ChainInfo>,
    /// Map that relates an epoch with the hashes of the blocks for that epoch
    // One epoch can have more than one block
    epoch_to_block_hash: HashMap<Epoch, HashSet<Hash>>,
    /// Map that stores blocks by hash
    blocks: HashMap<Hash, Block>,
}

/// Required trait for being able to retrieve BlocksManager address from registry
impl Supervised for BlocksManager {}

/// Required trait for being able to retrieve BlocksManager address from registry
impl SystemService for BlocksManager {}

/// Auxiliary methods for BlocksManager actor
impl BlocksManager {
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
                        info!("BlocksManager successfully persisted chain_info into storage")
                    }
                    _ => {
                        error!("BlocksManager failed to persist chain_info into storage");
                        // FIXME(#72): handle errors
                    }
                }
                actix::fut::ok(())
            })
            .wait(ctx);
    }

    fn process_new_block(&mut self, block: Block) -> Result<Hash, BlocksManagerError> {
        // Calculate the hash of the block
        let hash = calculate_sha256(&block.to_bytes().unwrap());

        // Check if we already have a block with that hash
        if let Some(_block) = self.blocks.get(&hash) {
            return Err(BlocksManagerError::BlockAlreadyExists);
        }

        // This is a new block, insert it into the internal maps
        {
            // Insert the new block into the map that relates epochs to block hashes
            let beacon = &block.header.block_header.beacon;
            let hash_set = &mut self
                .epoch_to_block_hash
                .entry(beacon.checkpoint)
                .or_insert_with(HashSet::new);
            hash_set.insert(hash);

            debug!(
                "Checkpoint {} has {} blocks",
                beacon.checkpoint,
                hash_set.len()
            );
        }

        // Insert the new block into the map of known blocks
        self.blocks.insert(hash, block);

        Ok(hash)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn add_block() {
        let mut bm = BlocksManager::default();

        use witnet_data_structures::chain::*;
        let checkpoint = 2;
        let block_a = Block {
            header: BlockHeaderWithProof {
                block_header: BlockHeader {
                    version: 1,
                    beacon: CheckpointBeacon {
                        checkpoint,
                        hash_prev_block: Hash::SHA256([4; 32]),
                    },
                    hash_merkle_root: Hash::SHA256([3; 32]),
                },
                proof: LeadershipProof {
                    block_sig: None,
                    influence: 99999,
                },
            },
            txn_count: 1,
            txns: vec![Transaction],
        };

        let hash_a = bm.process_new_block(block_a.clone()).unwrap();

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
        let mut bm = BlocksManager::default();

        use witnet_data_structures::chain::*;
        let block = Block {
            header: BlockHeaderWithProof {
                block_header: BlockHeader {
                    version: 1,
                    beacon: CheckpointBeacon {
                        checkpoint: 2,
                        hash_prev_block: Hash::SHA256([4; 32]),
                    },
                    hash_merkle_root: Hash::SHA256([3; 32]),
                },
                proof: LeadershipProof {
                    block_sig: None,
                    influence: 99999,
                },
            },
            txn_count: 1,
            txns: vec![Transaction],
        };

        // Only the first block will be inserted
        assert!(bm.process_new_block(block.clone()).is_ok());
        assert!(bm.process_new_block(block).is_err());
        assert_eq!(bm.blocks.len(), 1);
    }

    #[test]
    fn add_blocks_same_epoch() {
        let mut bm = BlocksManager::default();

        use witnet_data_structures::chain::*;
        let checkpoint = 2;
        let block_a = Block {
            header: BlockHeaderWithProof {
                block_header: BlockHeader {
                    version: 1,
                    beacon: CheckpointBeacon {
                        checkpoint: 2,
                        hash_prev_block: Hash::SHA256([4; 32]),
                    },
                    hash_merkle_root: Hash::SHA256([3; 32]),
                },
                proof: LeadershipProof {
                    block_sig: None,
                    influence: 99999,
                },
            },
            txn_count: 1,
            txns: vec![Transaction],
        };

        let mut block_b = block_a.clone();
        // Change a value to change the block_b hash
        block_b.header.proof.influence = 12345;

        let hash_a = bm.process_new_block(block_a).unwrap();
        let hash_b = bm.process_new_block(block_b).unwrap();

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
}
