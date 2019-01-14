use crate::utils::get_output_pointer_from_input;
use std::collections::HashMap;
use witnet_crypto::hash::Sha256;
use witnet_crypto::merkle::merkle_tree_root as crypto_merkle_tree_root;
use witnet_data_structures::chain::{
    Block, Epoch, Hash, Hashable, Output, OutputPointer, Transaction, TransactionsPool,
};

/// Function to validate block's mint
pub fn validate_mint(_block: &Block) -> bool {
    // TODO Implement validate mint algorithm
    true
}

/// Function to validate a transaction
pub fn validate_transaction<S: ::std::hash::BuildHasher>(
    _transaction: &Transaction,
    _utxo_set: &mut HashMap<OutputPointer, Output, S>,
) -> bool {
    //TODO Implement validate transaction properly
    true
}

/// Function to validate transactions in a block and update a utxo_set and a `TransactionsPool`
pub fn validate_transactions<S: ::std::hash::BuildHasher>(
    utxo_set: &mut HashMap<OutputPointer, Output, S>,
    txn_pool: &mut TransactionsPool,
    block: &Block,
) -> bool {
    let mut valid_transactions = true;
    let transactions = block.txns.clone();

    for transaction in transactions {
        if validate_transaction(&transaction, utxo_set) {
            let txn_hash = transaction.hash();

            for input in transaction.inputs {
                // Obtain the OuputPointer of each input and remove it from the utxo_set
                let output_pointer = get_output_pointer_from_input(&input);

                utxo_set.remove(&output_pointer);
            }

            for (index, output) in transaction.outputs.iter().enumerate() {
                // Add the new outputs to the utxo_set
                let output_pointer = OutputPointer {
                    transaction_id: txn_hash,
                    output_index: index as u32,
                };

                utxo_set.insert(output_pointer, output.clone());
            }

            txn_pool.remove(&txn_hash);
        } else {
            valid_transactions = false;
            break;
        }
    }

    valid_transactions
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
