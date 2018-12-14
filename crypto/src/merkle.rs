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

use crate::hash::{calculate_sha256, Sha256};

/// Calculate merkle tree root from the supplied hashes
pub fn merkle_tree_root(hashes: &[Sha256]) -> Sha256 {
    if hashes.is_empty() {
        // On empty input, return empty SHA256 hash
        calculate_sha256(b"")
    } else {
        merkle_tree_root_with_hashing_function(sha256_concat, hashes)
    }
}

/// Calculate `sha256(a || b)` where || means concatenation
fn sha256_concat(a: Sha256, b: Sha256) -> Sha256 {
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
