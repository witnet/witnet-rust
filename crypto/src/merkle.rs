//! Merkle tree implementation
//!
//! Design details:
//!
//! * When the number of nodes is not a multiple of two, the last element is promoted to the
//! next layer:
//!
//! ```norun
//!        ^
//!    ^       ^
//!  ^   ^   ^  |
//! a b c d e f g
//! ```

use crate::hash::{calculate_sha256, Sha256, EMPTY_SHA256};

/// Calculate merkle tree root from the supplied hashes
pub fn merkle_tree_root(hashes: &[Sha256]) -> Sha256 {
    if hashes.is_empty() {
        // On empty input, return empty SHA256 hash
        EMPTY_SHA256
    } else {
        merkle_tree_root_with_hashing_function(sha256_concat, hashes)
    }
}

/// Calculate `sha256(a || b)` where || means concatenation
pub fn sha256_concat(a: Sha256, b: Sha256) -> Sha256 {
    let mut h = a.0.to_vec();
    h.extend(&b.0);
    calculate_sha256(&h)
}

/// Generic merkle tree root calculation
fn merkle_tree_root_with_hashing_function<T: Copy>(hash_concat: fn(T, T) -> T, hashes: &[T]) -> T {
    match hashes.len() {
        // The public interface guarantees this panic will never happen
        0 => panic!("Input is empty"),
        1 => {
            // 1 node: copy hash to next level
            hashes[0]
        }
        2 => {
            // 2 nodes: concatenate hashes and hash that
            hash_concat(hashes[0], hashes[1])
        }
        n => {
            // n nodes: split into 2 and calculate the root for each half
            // split at the first power of two greater or equal to n / 2
            let (left, right) = hashes.split_at(((n + 1) / 2).next_power_of_two());
            let left_hash = merkle_tree_root_with_hashing_function(hash_concat, left);
            let right_hash = merkle_tree_root_with_hashing_function(hash_concat, right);

            // 2 nodes: concatenate hashes and hash that
            hash_concat(left_hash, right_hash)
        }
    }
}

/// Progressive merkle tree.
///
/// Usage:
///
/// ```
/// use witnet_crypto::hash::Sha256;
/// use witnet_crypto::merkle::ProgressiveMerkleTree;
/// use witnet_crypto::merkle::sha256_concat;
///
/// // Create an empty merkle tree
/// let mut mt = ProgressiveMerkleTree::sha256();
/// // Push hashes sequentially
/// let x0 = Sha256([0xAB; 32]);
/// mt.push(x0);
/// // The root of a one-item merkle tree is that item
/// assert_eq!(mt.root(), x0);
/// // Add another item
/// let x1 = Sha256([0xCD; 32]);
/// mt.push(x1);
/// // The root of a two-item merkle tree is the hash of the concatenation of the items
/// assert_eq!(mt.root(), sha256_concat(x0, x1));
/// ```
///
/// The root can be calculated on demand at any point in time.
///
/// The leaves are stored as a vector: the length of the vector encodes the tree depth.
///
/// Here is an illustration of the leaves vector when pushing 8 nodes in sequence.
/// Each slot `[   ]` contains the concatenation of the hashes of the elements inside it.
/// ```norun
/// [1   ]
/// [12  ][    ]
/// [12  ][3   ]
/// [1234][    ][    ]
/// [1234][    ][5   ]
/// [1234][56  ][    ]
/// [1234][56  ][7   ]
/// [12345678][  ][  ][  ]
/// ```
#[derive(Debug)]
pub struct ProgressiveMerkleTree<T: 'static> {
    leaves: Vec<Option<T>>,
    hash_concat: fn(T, T) -> T,
    empty_hash: &'static T,
}

impl ProgressiveMerkleTree<Sha256> {
    /// Merkle tree using SHA256 function
    pub fn sha256() -> Self {
        Self {
            leaves: vec![],
            hash_concat: sha256_concat,
            empty_hash: &EMPTY_SHA256,
        }
    }
}

impl<T: Copy> ProgressiveMerkleTree<T> {
    /// Push a new node to the end of the tree
    pub fn push(&mut self, x: T) {
        let ProgressiveMerkleTree {
            leaves,
            hash_concat,
            ..
        } = self;

        // Insert the new element in the first empty slot, starting from the end.
        // For each non-empty slot, concatenate and hash the existing
        // element with the new element, going up one level.
        if let Some(h1) = leaves.iter_mut().rev().try_fold(x, |h1, m| {
            match m.take() {
                None => {
                    *m = Some(h1);
                    // Done, exit
                    None
                }
                Some(h0) => {
                    // Concatenate h0 and h1
                    let h01 = hash_concat(h0, h1);
                    // Move the hash one level up
                    Some(h01)
                }
            }
        }) {
            // If we got here it means that the tree depth has increased by one
            // and the leaves vector consists of empty slots
            // So add an empty node to the end of the vector
            self.leaves.push(None);
            // And set the first node to the root of the tree
            self.leaves[0] = Some(h1);
        }
    }

    /// Calculate the current root of the merkle tree
    pub fn root(&self) -> T {
        // Concatenate all the hashes starting from the end
        self.leaves
            .iter()
            .rev()
            .fold(None, |h1, h0| match (*h0, h1) {
                (h0, None) => h0,
                (None, h1) => h1,
                (Some(h0), Some(h1)) => Some((self.hash_concat)(h0, h1)),
            })
            .unwrap_or(*self.empty_hash)
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn dummy_hash() {
        // Example of using a different hashing function
        let x = [80u8; 16];
        let hashes = vec![x, [15; 16]];
        let dummy = |a, _b| a;
        let root = merkle_tree_root_with_hashing_function(dummy, &hashes);
        assert_eq!(root, x);
    }
}
