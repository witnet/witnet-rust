use witnet_crypto::hash::Sha256;
use witnet_crypto::merkle::merkle_tree_root as crypto_merkle_tree_root;
use witnet_data_structures::chain::{Block, Hash, Hashable, Transaction};

/// Function to validate block's coinbase
pub fn validate_coinbase(_block: &Block) -> bool {
    // TODO Implement validate coinbase algorithm
    true
}

/// Function to calculate a merkle tree from a transaction vector
pub fn merkle_tree_root(transactions: &[Transaction]) -> Hash {
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
