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

use crate::hash::calculate_sha256;
use witnet_data_structures::chain::Hash;

/// Calculate merkle tree root from the supplied hashes
pub fn merkle_tree_root(hashes: &[Hash]) -> Hash {
    if hashes.is_empty() {
        // On empty input, return empty SHA256 hash
        calculate_sha256(b"")
    } else {
        let first = hashes[0];
        match first {
            Hash::SHA256(..) => {
                // Check that all the hashes are SHA256, and return SHA256
                let hashes_as_vec_of_u8_32 = hashes
                    .iter()
                    .map(|x| match x {
                        Hash::SHA256(y) => y,
                    })
                    .collect::<Vec<_>>();
                Hash::SHA256(merkle_tree_root_with_hashing_function(
                    sha256_concat,
                    &hashes_as_vec_of_u8_32,
                ))
            }
        }
    }
}

/// Calculate `sha256(a || b)` where || means concatenation
fn sha256_concat(a: [u8; 32], b: [u8; 32]) -> [u8; 32] {
    let mut h = a.to_vec();
    h.extend(&b);
    match calculate_sha256(&h) {
        Hash::SHA256(x) => x,
    }
}

/// Generic merkle tree root calculation
fn merkle_tree_root_with_hashing_function<T: Copy>(hash_concat: fn(T, T) -> T, hashes: &[&T]) -> T {
    match hashes.len() {
        0 => panic!("Input is empty"),
        1 => {
            // 1 node: copy hash to next level
            *hashes[0]
        }
        2 => {
            // 2 nodes: concatenate hashes and hash that
            hash_concat(*hashes[0], *hashes[1])
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

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn dummy_hash() {
        // Example of using a different hashing function
        let x = [80u8; 16];
        let hashes = vec![&x, &[15; 16]];
        let dummy = |a, _b| a;
        let root = merkle_tree_root_with_hashing_function(dummy, &hashes);
        assert_eq!(root, x);
    }
}
