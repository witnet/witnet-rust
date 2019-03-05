use std::collections::HashMap;

use super::data_request::DataRequestPool;

use witnet_crypto::{hash::Sha256, merkle::merkle_tree_root as crypto_merkle_tree_root};

use witnet_data_structures::chain::{
    Block, DataRequestOutput, Epoch, Hash, Hashable, Input, Output, OutputPointer, Transaction,
    TransactionsPool, UnspentOutputsPool,
};

use crate::actors::chain_manager::BlockInChain;
use log::{debug, warn};
use witnet_data_structures::chain::CheckpointBeacon;

/// Function to validate a transaction
pub fn validate_transaction<S: ::std::hash::BuildHasher>(
    _transaction: &Transaction,
    _utxo_set: &mut HashMap<OutputPointer, Output, S>,
) -> bool {
    //TODO Implement validate transaction properly
    true
}

/// Function to validate transactions in a block and update a utxo_set and a `TransactionsPool`
// TODO: Add verifications related to data requests (e.g. enough commitment transactions for a data request)
pub fn validate_transactions(
    utxo_set: &UnspentOutputsPool,
    _txn_pool: &TransactionsPool,
    data_request_pool: &DataRequestPool,
    block: &Block,
) -> Option<BlockInChain> {
    // TODO: Add validate_mint function

    let mut utxo_set = utxo_set.clone();
    let mut data_request_pool = data_request_pool.clone();

    let mut valid_transactions = true;

    let transactions = block.txns.clone();

    let mut remove_later = vec![];
    for transaction in &transactions {
        if validate_transaction(&transaction, &mut utxo_set) {
            let txn_hash = transaction.hash();

            for input in &transaction.inputs {
                // Obtain the OuputPointer of each input and remove it from the utxo_set
                let output_pointer = input.output_pointer();
                match input {
                    Input::DataRequest(..) => {
                        remove_later.push(output_pointer);
                    }
                    _ => {
                        utxo_set.remove(&output_pointer);
                    }
                }
            }

            for (index, output) in transaction.outputs.iter().enumerate() {
                // Add the new outputs to the utxo_set
                let output_pointer = OutputPointer {
                    transaction_id: txn_hash,
                    output_index: index as u32,
                };

                utxo_set.insert(output_pointer, output.clone());
            }

            // Add DataRequests from the block into the data_request_pool
            data_request_pool.process_transaction(
                transaction,
                block.block_header.beacon.checkpoint,
                &block.hash(),
            );
        } else {
            warn!("Transaction not valid");
            valid_transactions = false;
            break;
        }
    }

    for output_pointer in remove_later {
        utxo_set.remove(&output_pointer);
    }

    if valid_transactions {
        Some(BlockInChain {
            block: block.clone(),
            utxo_set,
            data_request_pool,
        })
    } else {
        None
    }
}

/// Function to validate a block
pub fn validate_block(
    block: &Block,
    current_epoch: Epoch,
    chain_beacon: CheckpointBeacon,
    genesis_block_hash: Hash,
    utxo_set: &UnspentOutputsPool,
    txn_pool: &TransactionsPool,
    data_request_pool: &DataRequestPool,
) -> Option<BlockInChain> {
    let mut response = None;
    let block_epoch = block.block_header.beacon.checkpoint;
    let hash_prev_block = block.block_header.beacon.hash_prev_block;

    if !verify_poe_block() {
        warn!("Invalid PoE");
    } else if !validate_merkle_tree(&block) {
        warn!("Block merkle tree not valid");
    } else if block_epoch > current_epoch {
        warn!(
            "Block epoch from the future: current: {}, block: {}",
            current_epoch, block_epoch
        );
    } else if chain_beacon.checkpoint > block_epoch {
        debug!(
            "Ignoring block from epoch {} (older than highest block checkpoint {})",
            block_epoch, chain_beacon.checkpoint
        );
    } else if hash_prev_block != genesis_block_hash
        && chain_beacon.hash_prev_block != hash_prev_block
    {
        warn!(
            "Ignoring block because previous hash [{:?}]is not known",
            hash_prev_block
        );
    } else {
        response = validate_transactions(&utxo_set, &txn_pool, &data_request_pool, &block)
    }

    response
}

/// Function to validate a block candidate
pub fn validate_candidate(block: &Block, current_epoch: Epoch) -> bool {
    let mut valid_block = false;
    let block_epoch = block.block_header.beacon.checkpoint;

    if !verify_poe_block() {
        warn!("Invalid PoE");
    } else if block_epoch != current_epoch {
        warn!(
            "Block epoch from different epoch: current: {}, block: {}",
            current_epoch, block_epoch
        );
    } else {
        valid_block = true;
    }

    valid_block
}

/// Function to calculate a merkle tree from a transaction vector
pub fn merkle_tree_root<T>(transactions: &[T]) -> Hash
where
    T: std::convert::AsRef<Transaction> + Hashable,
{
    let transactions_hashes: Vec<Sha256> = transactions
        .iter()
        .map(|x| match x.hash() {
            Hash::SHA256(x) => Sha256(x),
        })
        .collect();

    Hash::from(crypto_merkle_tree_root(&transactions_hashes))
}

/// Function to validate block's merkle tree
pub fn validate_merkle_tree(block: &Block) -> bool {
    let merkle_tree = block.block_header.hash_merkle_root;
    let transactions = &block.txns;

    merkle_tree == merkle_tree_root(transactions)
}

/// 1 satowit is the minimal unit of value
/// 1 wit = 100_000_000 satowits
pub const SATOWITS_PER_WIT: u64 = 100_000_000;

/// Calculate the block mining reward.
/// Returns "satowits", where 1 wit = 100_000_000 satowits.
pub fn block_reward(epoch: Epoch) -> u64 {
    let initial_reward: u64 = 500 * SATOWITS_PER_WIT;
    let halvings = epoch / 1_750_000;
    if halvings < 64 {
        initial_reward >> halvings
    } else {
        0
    }
}

/// Function to check poe validation for blocks
// TODO: Implement logic for this function
pub fn verify_poe_block() -> bool {
    true
}

/// Function to check poe validation for data requests
// TODO: Implement logic for this function
pub fn verify_poe_data_request() -> bool {
    true
}

/// Function to calculate the commit reward
pub fn calculate_commit_reward(dr_output: &DataRequestOutput) -> u64 {
    dr_output.value / u64::from(dr_output.witnesses) - dr_output.commit_fee
}

/// Function to calculate the reveal reward
pub fn calculate_reveal_reward(dr_output: &DataRequestOutput) -> u64 {
    calculate_commit_reward(dr_output) - dr_output.reveal_fee
}

/// Function to calculate the value transfer reward
pub fn calculate_dr_vt_reward(dr_output: &DataRequestOutput) -> u64 {
    calculate_reveal_reward(dr_output) - dr_output.tally_fee
}

/// Function to calculate the tally change
pub fn calculate_tally_change(dr_output: &DataRequestOutput, n_reveals: u64) -> u64 {
    calculate_reveal_reward(dr_output) * (u64::from(dr_output.witnesses) - n_reveals)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_block_reward() {
        // Satowits per wit
        let spw = 100_000_000;

        assert_eq!(block_reward(0), 500 * spw);
        assert_eq!(block_reward(1), 500 * spw);
        assert_eq!(block_reward(1_749_999), 500 * spw);
        assert_eq!(block_reward(1_750_000), 250 * spw);
        assert_eq!(block_reward(3_499_999), 250 * spw);
        assert_eq!(block_reward(3_500_000), 125 * spw);
        assert_eq!(block_reward(1_750_000 * 35), 1);
        assert_eq!(block_reward(1_750_000 * 36), 0);
        assert_eq!(block_reward(1_750_000 * 63), 0);
        assert_eq!(block_reward(1_750_000 * 64), 0);
        assert_eq!(block_reward(1_750_000 * 100), 0);
    }
}
