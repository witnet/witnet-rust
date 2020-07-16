//! Merkle tree implementation
//!
//! Design details:
//!
//! * When the number of nodes is not a multiple of two, the last element is promoted to the
//! next layer:
//!
//! ```ignore
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
/// ```ignore
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

/// Full merkle tree with all the intermediate nodes, used to generate
/// inclusion proofs
#[derive(Debug)]
pub struct FullMerkleTree<T: 'static> {
    nodes: Vec<Vec<T>>,
    hash_concat: fn(T, T) -> T,
    empty_hash: &'static T,
}

impl FullMerkleTree<Sha256> {
    /// Merkle tree using SHA256 function
    pub fn sha256(leaves: Vec<Sha256>) -> Self {
        Self::new(leaves, sha256_concat, &EMPTY_SHA256)
    }
}

impl<T: Copy + PartialEq> FullMerkleTree<T> {
    /// New full merkle tree
    pub fn new(leaves: Vec<T>, hash_concat: fn(T, T) -> T, empty_hash: &'static T) -> Self {
        let nodes = FullMerkleTree::build(leaves, hash_concat);
        Self {
            nodes,
            hash_concat,
            empty_hash,
        }
    }
    // Build all the nodes from the leaves
    fn build(leaves: Vec<T>, hash_concat: fn(T, T) -> T) -> Vec<Vec<T>> {
        if leaves.is_empty() {
            return vec![];
        }
        let next_layer = |leaves: &[T]| {
            let v: Vec<T> = leaves
                .chunks(2)
                .map(|c| match c {
                    &[left, right] => hash_concat(left, right),
                    &[hash] => hash,
                    x => panic!(
                        "Chunks iterator returned invalid slice with len {}",
                        x.len()
                    ),
                })
                .collect();

            v
        };

        let mut nodes = vec![leaves];
        while nodes.last().unwrap().len() > 1 {
            nodes.push(next_layer(nodes.last().unwrap()));
        }

        nodes
    }

    /// Get the merkle tree root
    pub fn root(&self) -> T {
        self.nodes.last().map(|x| x[0]).unwrap_or(*self.empty_hash)
    }

    /// Get the nodes
    pub fn nodes(&self) -> &[Vec<T>] {
        &self.nodes
    }

    /// Create inclusion proof for element at index.
    ///
    /// Returns `None` if the index is out of bounds.
    pub fn inclusion_proof(&self, index: usize) -> Option<InclusionProof<T>> {
        // If the merkle tree is empty or the index is out of bounds, there is nothing to prove
        if self.nodes.is_empty() || index >= self.nodes[0].len() {
            return None;
        }

        // i is the index of the leaf we want to prove, which changes as
        // we go up the tree
        let mut i = index;
        // v is the list of nodes required to prove inclusion, ordered from
        // bottom to top
        let mut v = vec![];
        // proof_index is the index of the leaf in the proof: in a merkle tree
        // with 5 leaves, the element with index 4 will have a proof_index of 1,
        // because the proof will contain 1 node which needs to be appended from the left.
        let mut proof_index = 0;

        for layer in &self.nodes {
            // By using get(i ^ 1) we obtain the node from the left or from the right
            // depending on the index i.
            // This condition will skip incomplete layers where we can just move the node
            // one layer up without creating any proof
            // And it will also skip the last layer which consists of only the root.
            if let Some(x) = layer.get(i ^ 1) {
                // Save information about whether to append the proof from the left or from
                // the right as proof_index. A bit set to 0 means (node || proof), and a
                // bit set to 1 means (proof || node)
                proof_index |= (i & 1) << v.len();
                // Insert node into proof
                v.push(*x);
            }

            // Shift index one bit to the right: this is a division by 2, which is needed
            // to transform the index from layer n to layer n+1
            i >>= 1;
        }

        Some(InclusionProof::new(proof_index, v, self.hash_concat))
    }
}

/// Inclusion proof of an element in a merkle tree
#[derive(Debug)]
pub struct InclusionProof<T> {
    // This is not always the index of the element in the tree:
    // In a 5-element merkle tree: [0, 1, 2, 3, 4]
    // The indexes of the proofs would be [0, 1, 2, 3, 1],
    // but the length of the lemma would be [3, 3, 3, 3, 1].
    index: usize,
    lemma: Vec<T>,
    hash_concat: fn(T, T) -> T,
}

impl InclusionProof<Sha256> {
    /// Sha256 inclusion proof
    pub fn sha256(index: usize, lemma: Vec<Sha256>) -> Self {
        Self::new(index, lemma, sha256_concat)
    }
}

impl<T: Copy + PartialEq> InclusionProof<T> {
    /// New inclusion proof
    pub fn new(index: usize, lemma: Vec<T>, hash_concat: fn(T, T) -> T) -> Self {
        Self {
            index,
            lemma,
            hash_concat,
        }
    }

    /// Calculate the root of the merkle tree given the element hash, using the
    /// merkle path stored in lemma
    pub fn root(&self, element: T) -> T {
        let mut elem = element;
        let hash_concat = self.hash_concat;
        for (level, h) in self.lemma.iter().enumerate() {
            let h_on_the_left = ((self.index >> level) & 1) == 1;
            elem = if h_on_the_left {
                hash_concat(*h, elem)
            } else {
                hash_concat(elem, *h)
            };
        }

        elem
    }

    /// Verify a proof of inclusion, given the element hash and the root hash
    pub fn verify(&self, element: T, root: T) -> bool {
        self.root(element) == root
    }

    /// Lemma: merkle path
    pub fn lemma(&self) -> &[T] {
        &self.lemma
    }

    /// Proof index: used to indicate when to concatenate left and when to
    /// concatenate right
    pub fn proof_index(&self) -> usize {
        self.index
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
