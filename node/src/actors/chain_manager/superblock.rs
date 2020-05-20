use std::collections::HashSet;
use witnet_data_structures::chain::{
    BlockHeader, Hash, Hashable, PublicKeyHash, SuperBlock, SuperBlockVote,
};
use witnet_validations::validations::hash_merkle_tree_root;

/// Possible result of SuperBlockState::add_vote
pub enum AddSuperBlockVote {
    AlreadySeen,
    InvalidIndex,
    MaybeValid,
    NotInArs,
    ValidButDifferentHash,
    ValidWithSameHash,
}

/// State related to superblocks
#[derive(Debug, Default)]
pub struct SuperBlockState {
    // Set of ARS identities that will be able to send superblock votes in the next superblock epoch
    current_ars_identities: Option<HashSet<PublicKeyHash>>,
    // Current superblock hash created by this node
    current_superblock_hash: Option<Hash>,
    // Current superblock index, used to limit the range of broadcasted votes to
    // [index - 1, index + 1]. So if index is 10, only votes with index 9, 10, 11 will be broadcasted
    current_superblock_index: Option<u32>,
    // Set of ARS identities that can currently send superblock votes
    previous_ars_identities: Option<HashSet<PublicKeyHash>>,
    // Set of received superblock votes
    // This is cleared when we try to create a new superblock
    received_superblocks: HashSet<SuperBlockVote>,
    // Set of votes that agree with current_superblock_hash
    // This is cleared when we try to create a new superblock
    votes_on_local_superlock: HashSet<SuperBlockVote>,
}

impl SuperBlockState {
    /// Add a vote sent by another peer.
    /// This method assumes that the signatures are valid, they must be checked by the caller.
    pub fn add_vote(&mut self, sbv: &SuperBlockVote) -> AddSuperBlockVote {
        if self.received_superblocks.contains(sbv) {
            // Already processed before
            AddSuperBlockVote::AlreadySeen
        } else {
            // Insert to avoid validating again
            self.received_superblocks.insert(sbv.clone());

            let valid = self.is_valid(sbv);

            match valid {
                Some(true) => {
                    // If the superblock vote is valid and agrees with the local superblock hash,
                    // store it
                    if Some(sbv.superblock_hash) == self.current_superblock_hash {
                        self.votes_on_local_superlock.insert(sbv.clone());

                        AddSuperBlockVote::ValidWithSameHash
                    } else {
                        AddSuperBlockVote::ValidButDifferentHash
                    }
                }
                Some(false) => {
                    if Some(sbv.superblock_index) == self.current_superblock_index {
                        AddSuperBlockVote::NotInArs
                    } else {
                        AddSuperBlockVote::InvalidIndex
                    }
                }
                None => AddSuperBlockVote::MaybeValid,
            }
        }
    }

    /// Since we do not check signatures here, a superblock vote is valid if the signing identity
    /// is in the ARS.
    /// Returns true, false, or unknown
    fn is_valid(&self, sbv: &SuperBlockVote) -> Option<bool> {
        match self.current_superblock_index {
            // We do not know the current index, we cannot know if the vote is valid
            None => None,
            // If the index is the same as the current one, the vote is valid if it is signed by a
            // member of the ARS
            Some(x) if x == sbv.superblock_index => self
                .previous_ars_identities
                .as_ref()
                .map(|x| x.contains(&sbv.secp256k1_signature.public_key.pkh())),
            // If the index is not the same as the current one, but it is within an acceptable range
            // of [x-1, x+1], broadcast the vote without checking if it is a member of the ARS, as
            // the ARS may have changed and we do not keep older copies of the ARS in memory
            Some(x) => {
                // Check [x-1, x+1] range with overflow prevention
                if ((x.saturating_sub(1))..=(x.saturating_add(1))).contains(&sbv.superblock_index) {
                    None
                } else {
                    Some(false)
                }
            }
        }
    }

    /// Produces a `SuperBlock` that includes the blocks in `block_headers` if there is at least one of them.
    /// `sorted_ars_identities` will be used to validate all the superblock votes received for the
    /// next superblock. The votes for the current superblock must be validated using
    /// `previous_ars_identities`.
    pub fn build_superblock(
        &mut self,
        block_headers: &[BlockHeader],
        sorted_ars_identities: &[PublicKeyHash],
        superblock_index: u32,
        last_block_in_previous_superblock: Hash,
    ) -> Option<SuperBlock> {
        self.current_superblock_index = Some(superblock_index);
        self.votes_on_local_superlock.clear();

        match mining_build_superblock(
            block_headers,
            sorted_ars_identities,
            superblock_index,
            last_block_in_previous_superblock,
        ) {
            None => {
                // Clear state when there is no superblock
                // Note that the ARS members list is not updated in this case
                self.current_superblock_hash = None;
                self.received_superblocks.clear();

                None
            }
            Some(superblock) => {
                let superblock_hash = superblock.hash();
                self.current_superblock_hash = Some(superblock_hash);

                // Save ARS identities:
                // previous = current
                // current = sorted_ars_identities
                {
                    std::mem::swap(
                        &mut self.previous_ars_identities,
                        &mut self.current_ars_identities,
                    );
                    // Reuse allocated memory
                    let hs = self.current_ars_identities.get_or_insert(HashSet::new());
                    hs.clear();
                    hs.extend(sorted_ars_identities.iter().cloned());
                }

                let mut old_superblock_votes =
                    std::mem::replace(&mut self.received_superblocks, HashSet::new());
                // Process old superblock votes
                for sbv in old_superblock_votes.drain() {
                    // Validate again, check if they are valid now
                    let valid = self.is_valid(&sbv);

                    // If the superblock vote is valid and agrees with the local superblock hash,
                    // store it
                    if valid == Some(true)
                        && Some(sbv.superblock_hash) == self.current_superblock_hash
                    {
                        self.votes_on_local_superlock.insert(sbv);
                    }
                }
                // old_superblock_votes should be empty, as we have drained it
                // But swap it back to reuse allocated memory
                // TODO: remove asserts after adding tests
                assert!(old_superblock_votes.is_empty());
                assert!(self.received_superblocks.is_empty());
                std::mem::replace(&mut self.received_superblocks, old_superblock_votes);

                Some(superblock)
            }
        }
    }
}

/// Produces a `SuperBlock` that includes the blocks in `block_headers` if there is at least one of them.
pub fn mining_build_superblock(
    block_headers: &[BlockHeader],
    sorted_ars_identities: &[PublicKeyHash],
    index: u32,
    last_block_in_previous_superblock: Hash,
) -> Option<SuperBlock> {
    let last_block = block_headers.last()?.hash();
    let merkle_drs: Vec<Hash> = block_headers
        .iter()
        .map(|b| b.merkle_roots.dr_hash_merkle_root)
        .collect();
    let merkle_tallies: Vec<Hash> = block_headers
        .iter()
        .map(|b| b.merkle_roots.tally_hash_merkle_root)
        .collect();

    let pkh_hashes: Vec<Hash> = sorted_ars_identities.iter().map(|pkh| pkh.hash()).collect();

    Some(SuperBlock {
        data_request_root: hash_merkle_tree_root(&merkle_drs),
        tally_root: hash_merkle_tree_root(&merkle_tallies),
        ars_root: hash_merkle_tree_root(&pkh_hashes),
        index,
        last_block,
        last_block_in_previous_superblock,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use witnet_data_structures::chain::{BlockMerkleRoots, CheckpointBeacon};
    use witnet_data_structures::vrf::BlockEligibilityClaim;

    #[test]
    fn test_superblock_creation_no_blocks() {
        let default_hash = Hash::default();
        let superblock = mining_build_superblock(&[], &[], 0, default_hash);
        assert_eq!(superblock, None);
    }

    static DR_MERKLE_ROOT_1: &str =
        "0000000000000000000000000000000000000000000000000000000000000000";
    static TALLY_MERKLE_ROOT_1: &str =
        "1111111111111111111111111111111111111111111111111111111111111111";
    static DR_MERKLE_ROOT_2: &str =
        "ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff";
    static TALLY_MERKLE_ROOT_2: &str =
        "dddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddd";
    #[test]
    fn test_superblock_creation_one_block() {
        let default_hash = Hash::default();
        let default_proof = BlockEligibilityClaim::default();
        let default_beacon = CheckpointBeacon::default();
        let dr_merkle_root_1 = DR_MERKLE_ROOT_1.parse().unwrap();
        let tally_merkle_root_1 = TALLY_MERKLE_ROOT_1.parse().unwrap();

        let block = BlockHeader {
            version: 1,
            beacon: default_beacon,
            merkle_roots: BlockMerkleRoots {
                mint_hash: default_hash,
                vt_hash_merkle_root: default_hash,
                dr_hash_merkle_root: dr_merkle_root_1,
                commit_hash_merkle_root: default_hash,
                reveal_hash_merkle_root: default_hash,
                tally_hash_merkle_root: tally_merkle_root_1,
            },
            proof: default_proof,
            bn256_public_key: None,
        };

        let expected_superblock = SuperBlock {
            data_request_root: dr_merkle_root_1,
            tally_root: tally_merkle_root_1,
            ars_root: PublicKeyHash::default().hash(),
            index: 0,
            last_block: block.hash(),
            last_block_in_previous_superblock: default_hash,
        };

        let superblock =
            mining_build_superblock(&[block], &[PublicKeyHash::default()], 0, default_hash)
                .unwrap();
        assert_eq!(superblock, expected_superblock);
    }

    #[test]
    fn test_superblock_creation_two_blocks() {
        let default_hash = Hash::default();
        let default_proof = BlockEligibilityClaim::default();
        let default_beacon = CheckpointBeacon::default();
        let dr_merkle_root_1 = DR_MERKLE_ROOT_1.parse().unwrap();
        let tally_merkle_root_1 = TALLY_MERKLE_ROOT_1.parse().unwrap();
        let dr_merkle_root_2 = DR_MERKLE_ROOT_2.parse().unwrap();
        let tally_merkle_root_2 = TALLY_MERKLE_ROOT_2.parse().unwrap();
        // Sha256(dr_merkle_root_1 || dr_merkle_root_2)
        let expected_superblock_dr_root =
            "bba91ca85dc914b2ec3efb9e16e7267bf9193b14350d20fba8a8b406730ae30a"
                .parse()
                .unwrap();
        // Sha256(tally_merkle_root_1 || tally_merkle_root_2)
        let expected_superblock_tally_root =
            "83a70a79e9bef7bd811df52736eb61373095d7a8936aed05d0dc96d959b30b50"
                .parse()
                .unwrap();

        let block_1 = BlockHeader {
            version: 1,
            beacon: default_beacon,
            merkle_roots: BlockMerkleRoots {
                mint_hash: default_hash,
                vt_hash_merkle_root: default_hash,
                dr_hash_merkle_root: dr_merkle_root_1,
                commit_hash_merkle_root: default_hash,
                reveal_hash_merkle_root: default_hash,
                tally_hash_merkle_root: tally_merkle_root_1,
            },
            proof: default_proof.clone(),
            bn256_public_key: None,
        };

        let block_2 = BlockHeader {
            version: 1,
            beacon: default_beacon,
            merkle_roots: BlockMerkleRoots {
                mint_hash: default_hash,
                vt_hash_merkle_root: default_hash,
                dr_hash_merkle_root: dr_merkle_root_2,
                commit_hash_merkle_root: default_hash,
                reveal_hash_merkle_root: default_hash,
                tally_hash_merkle_root: tally_merkle_root_2,
            },
            proof: default_proof,
            bn256_public_key: None,
        };

        let expected_superblock = SuperBlock {
            data_request_root: expected_superblock_dr_root,
            tally_root: expected_superblock_tally_root,
            ars_root: PublicKeyHash::default().hash(),
            index: 0,
            last_block: block_2.hash(),
            last_block_in_previous_superblock: default_hash,
        };

        let superblock = mining_build_superblock(
            &[block_1, block_2],
            &[PublicKeyHash::default()],
            0,
            default_hash,
        )
        .unwrap();
        assert_eq!(superblock, expected_superblock);
    }
}
