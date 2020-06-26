use crate::chain::{
    BlockHeader, Bn256PublicKey, Hash, Hashable, PublicKeyHash, SuperBlock, SuperBlockVote,
};
use std::collections::HashSet;

use serde::{Deserialize, Serialize};

use witnet_crypto::{hash::Sha256, merkle::merkle_tree_root as crypto_merkle_tree_root};

/// Possible result of SuperBlockState::add_vote
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum AddSuperBlockVote {
    /// vote already counted
    AlreadySeen,
    /// invalid superblock index
    InvalidIndex,
    /// unverifiable vote because we do not have the required ARS state
    MaybeValid,
    /// vote from a peer not in the ARS
    NotInArs,
    /// valid vote but with different hash
    ValidButDifferentHash,
    /// valid vote with identical hash
    ValidWithSameHash,
}

/// State related to superblocks
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
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
    // The last ARS ordered keys
    previous_ars_ordered_keys: Vec<Bn256PublicKey>,
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
    /// `ars_pkh_keys` will be used to validate all the superblock votes received for the
    /// next superblock. The votes for the current superblock must be validated using
    /// `previous_ars_identities`. The ordered bn256 keys will be merkelized and appended to the superblock
    pub fn build_superblock(
        &mut self,
        block_headers: &[BlockHeader],
        ars_pkh_keys: &[PublicKeyHash],
        ars_ordered_bn256_keys: &[Bn256PublicKey],
        superblock_index: u32,
        last_block_in_previous_superblock: Hash,
    ) -> Option<SuperBlock> {
        self.current_superblock_index = Some(superblock_index);
        self.votes_on_local_superlock.clear();
        let key_leaves = hash_key_leaves(ars_ordered_bn256_keys);

        match mining_build_superblock(
            block_headers,
            &key_leaves,
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
                // current = ars_pkh_keys
                {
                    std::mem::swap(
                        &mut self.previous_ars_identities,
                        &mut self.current_ars_identities,
                    );
                    // Reuse allocated memory
                    let hs = self.current_ars_identities.get_or_insert(HashSet::new());
                    hs.clear();
                    hs.extend(ars_pkh_keys.iter().cloned());
                    self.previous_ars_ordered_keys = ars_ordered_bn256_keys.to_vec();
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
                std::mem::replace(&mut self.received_superblocks, old_superblock_votes);

                Some(superblock)
            }
        }
    }
}

/// Produces a `SuperBlock` that includes the blocks in `block_headers` if there is at least one of them.
pub fn mining_build_superblock(
    block_headers: &[BlockHeader],
    ars_ordered_hash_leaves: &[Hash],
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

    Some(SuperBlock {
        ars_length: ars_ordered_hash_leaves.len() as u64,
        data_request_root: hash_merkle_tree_root(&merkle_drs),
        tally_root: hash_merkle_tree_root(&merkle_tallies),
        ars_root: hash_merkle_tree_root(ars_ordered_hash_leaves),
        index,
        last_block,
        last_block_in_previous_superblock,
    })
}

/// Takes a set of keys and calculates their hashes roots to be used as leaves.
pub fn hash_key_leaves(ars_ordered_keys: &[Bn256PublicKey]) -> Vec<Hash> {
    ars_ordered_keys.iter().map(|bn256| bn256.hash()).collect()
}

/// Function to calculate a merkle tree from a transaction vector
pub fn hash_merkle_tree_root(hashes: &[Hash]) -> Hash {
    let hashes: Vec<Sha256> = hashes
        .iter()
        .map(|x| match x {
            Hash::SHA256(x) => Sha256(*x),
        })
        .collect();

    Hash::from(crypto_merkle_tree_root(&hashes))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        chain::{BlockMerkleRoots, Bn256SecretKey, CheckpointBeacon, PublicKey},
        vrf::BlockEligibilityClaim,
    };
    use witnet_crypto::hash::calculate_sha256;

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
            ars_length: 1,
            data_request_root: dr_merkle_root_1,
            tally_root: tally_merkle_root_1,
            ars_root: default_hash,
            index: 0,
            last_block: block.hash(),
            last_block_in_previous_superblock: default_hash,
        };

        let superblock =
            mining_build_superblock(&[block], &[default_hash], 0, default_hash).unwrap();
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
            ars_length: 1,
            data_request_root: expected_superblock_dr_root,
            tally_root: expected_superblock_tally_root,
            ars_root: default_hash,
            index: 0,
            last_block: block_2.hash(),
            last_block_in_previous_superblock: default_hash,
        };

        let superblock =
            mining_build_superblock(&[block_1, block_2], &[default_hash], 0, default_hash).unwrap();
        assert_eq!(superblock, expected_superblock);
    }

    #[test]
    fn superblock_state_default_add_votes() {
        // When the superblock state is initialized to default (for example when starting the node),
        // all the received superblock votes are marked as `MaybeValid` or `AlreadySeen`
        let mut sbs = SuperBlockState::default();

        let v1 = SuperBlockVote::new_unsigned(Hash::SHA256([1; 32]), 0);
        assert_eq!(sbs.add_vote(&v1), AddSuperBlockVote::MaybeValid);

        let v2 = SuperBlockVote::new_unsigned(Hash::SHA256([2; 32]), 0);
        assert_eq!(sbs.add_vote(&v2), AddSuperBlockVote::MaybeValid);

        // Before building the first superblock locally we do not know the current superblock_index,
        // so all the superblock votes will be "MaybeValid"
        let v3 = SuperBlockVote::new_unsigned(Hash::SHA256([3; 32]), 33);
        assert_eq!(sbs.add_vote(&v3), AddSuperBlockVote::MaybeValid);
    }

    #[test]
    fn superblock_state_first_superblock_cannot_be_validated() {
        // The first superblock built after starting the node cannot be validated because we need
        // the list of ARS members from the previous superblock
        let mut sbs = SuperBlockState::default();

        let block_headers = vec![BlockHeader::default()];
        let ars_identities = vec![PublicKeyHash::from_bytes(&[1; 20]).unwrap()];
        let genesis_hash = Hash::default();
        let bls_pk =
            Bn256PublicKey::from_secret_key(&Bn256SecretKey::from_slice(&[1; 32]).unwrap())
                .unwrap();
        let sb1 = sbs
            .build_superblock(&block_headers, &ars_identities, &[bls_pk], 0, genesis_hash)
            .unwrap();
        let v1 = SuperBlockVote::new_unsigned(sb1.hash(), 0);
        assert_eq!(sbs.add_vote(&v1), AddSuperBlockVote::MaybeValid);
    }

    #[test]
    fn superblock_state_first_superblock_none() {
        // If the first superblock is None, the state is not updated except for the superblock_index
        let mut sbs = SuperBlockState::default();

        // If there were no blocks, there will be no superblock
        let block_headers = vec![];
        let ars_identities = vec![PublicKeyHash::from_bytes(&[1; 20]).unwrap()];
        let bls_pk =
            Bn256PublicKey::from_secret_key(&Bn256SecretKey::from_slice(&[1; 32]).unwrap())
                .unwrap();

        let genesis_hash = Hash::default();
        assert_eq!(
            sbs.build_superblock(&block_headers, &ars_identities, &[bls_pk], 0, genesis_hash),
            None
        );

        let mut expected_sbs = SuperBlockState::default();
        expected_sbs.current_superblock_index = Some(0);
        assert_eq!(sbs, expected_sbs);
    }

    #[test]
    fn superblock_state_second_superblock_none() {
        let mut sbs = SuperBlockState::default();

        let block_headers = vec![BlockHeader::default()];
        let ars_identities = vec![PublicKeyHash::from_bytes(&[1; 20]).unwrap()];
        let bls_pk =
            Bn256PublicKey::from_secret_key(&Bn256SecretKey::from_slice(&[1; 32]).unwrap())
                .unwrap();
        let genesis_hash = Hash::default();
        let _sb1 = sbs
            .build_superblock(
                &block_headers,
                &ars_identities,
                &[bls_pk.clone()],
                0,
                genesis_hash,
            )
            .unwrap();

        let mut expected_sbs = sbs.clone();
        assert_eq!(
            sbs.build_superblock(&[], &ars_identities, &[bls_pk], 1, genesis_hash),
            None
        );

        // The only think that should change is the superblock_index
        expected_sbs.current_superblock_index = Some(1);
        // And the superblock_hash, which will be set to None
        expected_sbs.current_superblock_hash = None;
        assert_eq!(sbs, expected_sbs);
    }

    #[test]
    fn superblock_state_already_seen() {
        // Check that no matter the internal state, the second time a vote is added, it will return
        // `AlreadySeen`
        let mut sbs = SuperBlockState::default();

        let v0 = SuperBlockVote::new_unsigned(Hash::SHA256([1; 32]), 0);
        assert_eq!(sbs.add_vote(&v0), AddSuperBlockVote::MaybeValid);
        assert_eq!(sbs.add_vote(&v0), AddSuperBlockVote::AlreadySeen);

        let block_headers = vec![BlockHeader::default()];
        let ars_identities = vec![PublicKeyHash::from_bytes(&[1; 20]).unwrap()];

        let bls_pk =
            Bn256PublicKey::from_secret_key(&Bn256SecretKey::from_slice(&[1; 32]).unwrap())
                .unwrap();
        let genesis_hash = Hash::default();
        let _sb1 = sbs
            .build_superblock(
                &block_headers,
                &ars_identities,
                &[bls_pk.clone()],
                0,
                genesis_hash,
            )
            .unwrap();
        // After building a new superblock the cache is invalidated
        assert_eq!(sbs.add_vote(&v0), AddSuperBlockVote::MaybeValid);
        assert_eq!(sbs.add_vote(&v0), AddSuperBlockVote::AlreadySeen);

        let _sb2 = sbs
            .build_superblock(&block_headers, &ars_identities, &[bls_pk], 1, genesis_hash)
            .unwrap();
        let v1 = SuperBlockVote::new_unsigned(Hash::SHA256([2; 32]), 1);
        assert_eq!(sbs.add_vote(&v1), AddSuperBlockVote::NotInArs);
        assert_eq!(sbs.add_vote(&v1), AddSuperBlockVote::AlreadySeen);

        let v2 = SuperBlockVote::new_unsigned(Hash::SHA256([3; 32]), 2);
        assert_eq!(sbs.add_vote(&v2), AddSuperBlockVote::MaybeValid);
        assert_eq!(sbs.add_vote(&v2), AddSuperBlockVote::AlreadySeen);

        let v3 = SuperBlockVote::new_unsigned(Hash::SHA256([4; 32]), 3);
        assert_eq!(sbs.add_vote(&v3), AddSuperBlockVote::InvalidIndex);
        assert_eq!(sbs.add_vote(&v3), AddSuperBlockVote::AlreadySeen);
    }

    #[test]
    fn superblock_state_ars_identities() {
        // Create 3 superblocks, where each one of them has an ARS with only one identity
        // This checks that the ARS is correctly set
        let mut sbs = SuperBlockState::default();
        let block_headers = vec![BlockHeader::default()];
        let genesis_hash = Hash::default();

        let p1 = PublicKey::from_bytes([1; 33]);
        let p2 = PublicKey::from_bytes([2; 33]);
        let p3 = PublicKey::from_bytes([3; 33]);

        let bls_pk1 =
            Bn256PublicKey::from_secret_key(&Bn256SecretKey::from_slice(&[1; 32]).unwrap())
                .unwrap();
        let bls_pk2 =
            Bn256PublicKey::from_secret_key(&Bn256SecretKey::from_slice(&[2; 32]).unwrap())
                .unwrap();
        let bls_pk3 =
            Bn256PublicKey::from_secret_key(&Bn256SecretKey::from_slice(&[3; 32]).unwrap())
                .unwrap();

        let ars0 = vec![];
        let ars1 = vec![p1.pkh()];
        let ars2 = vec![p2.pkh()];
        let ars3 = vec![p3.pkh()];
        let ars4 = vec![];

        let ars0_ordered = vec![];
        let ars1_ordered = vec![bls_pk1];
        let ars2_ordered = vec![bls_pk2];
        let ars3_ordered = vec![bls_pk3];
        let ars4_ordered = vec![];

        let create_votes = |superblock_hash, superblock_index| {
            let mut v1 = SuperBlockVote::new_unsigned(superblock_hash, superblock_index);
            v1.secp256k1_signature.public_key = p1.clone();
            let mut v2 = SuperBlockVote::new_unsigned(superblock_hash, superblock_index);
            v2.secp256k1_signature.public_key = p2.clone();
            let mut v3 = SuperBlockVote::new_unsigned(superblock_hash, superblock_index);
            v3.secp256k1_signature.public_key = p3.clone();

            (v1, v2, v3)
        };

        // Superblock votes for index 0 cannot be validated because we do not know the ARS for index -1
        // (because it does not exist)
        let sb0 = sbs
            .build_superblock(&block_headers, &ars0, &ars0_ordered, 0, genesis_hash)
            .unwrap();
        let (v1, v2, v3) = create_votes(sb0.hash(), 0);
        assert_eq!(sbs.add_vote(&v1), AddSuperBlockVote::MaybeValid);
        assert_eq!(sbs.add_vote(&v2), AddSuperBlockVote::MaybeValid);
        assert_eq!(sbs.add_vote(&v3), AddSuperBlockVote::MaybeValid);

        // The ARS included in superblock 0 is empty, so none of the superblock votes for index 1
        // can be valid, they all return `NotInArs`
        let sb1 = sbs
            .build_superblock(&block_headers, &ars1, &ars1_ordered, 1, genesis_hash)
            .unwrap();
        let (v1, v2, v3) = create_votes(sb1.hash(), 1);
        assert_eq!(sbs.add_vote(&v1), AddSuperBlockVote::NotInArs);
        assert_eq!(sbs.add_vote(&v2), AddSuperBlockVote::NotInArs);
        assert_eq!(sbs.add_vote(&v3), AddSuperBlockVote::NotInArs);

        // The ARS included in superblock 1 contains only identity p1, so only the vote v1 will be
        // valid in superblock votes for index 2
        let sb2 = sbs
            .build_superblock(&block_headers, &ars2, &ars2_ordered, 2, genesis_hash)
            .unwrap();
        let (v1, v2, v3) = create_votes(sb2.hash(), 2);
        assert_eq!(sbs.add_vote(&v1), AddSuperBlockVote::ValidWithSameHash);
        assert_eq!(sbs.add_vote(&v2), AddSuperBlockVote::NotInArs);
        assert_eq!(sbs.add_vote(&v3), AddSuperBlockVote::NotInArs);

        // The ARS included in superblock 2 contains only identity p2, so only the vote v2 will be
        // valid in superblock votes for index 3
        let sb3 = sbs
            .build_superblock(&block_headers, &ars3, &ars3_ordered, 3, genesis_hash)
            .unwrap();
        let (v1, v2, v3) = create_votes(sb3.hash(), 3);
        assert_eq!(sbs.add_vote(&v1), AddSuperBlockVote::NotInArs);
        assert_eq!(sbs.add_vote(&v2), AddSuperBlockVote::ValidWithSameHash);
        assert_eq!(sbs.add_vote(&v3), AddSuperBlockVote::NotInArs);

        // The ARS included in superblock 3 contains only identity p3, so only the vote v3 will be
        // valid in superblock votes for index 4
        let sb4 = sbs
            .build_superblock(&block_headers, &ars4, &ars4_ordered, 4, genesis_hash)
            .unwrap();
        let (v1, v2, v3) = create_votes(sb4.hash(), 4);
        assert_eq!(sbs.add_vote(&v1), AddSuperBlockVote::NotInArs);
        assert_eq!(sbs.add_vote(&v2), AddSuperBlockVote::NotInArs);
        assert_eq!(sbs.add_vote(&v3), AddSuperBlockVote::ValidWithSameHash);
    }

    #[test]
    fn superblock_state_check_on_build() {
        // When calling build_superblock, all the old superblock votes will be evaluated again, and
        // inserted into votes_on_local_superblock if they agree
        let mut sbs = SuperBlockState::default();

        let p1 = PublicKey::from_bytes([1; 33]);
        let p2 = PublicKey::from_bytes([2; 33]);
        let p3 = PublicKey::from_bytes([3; 33]);

        let bls_pk1 =
            Bn256PublicKey::from_secret_key(&Bn256SecretKey::from_slice(&[1; 32]).unwrap())
                .unwrap();
        let bls_pk2 =
            Bn256PublicKey::from_secret_key(&Bn256SecretKey::from_slice(&[2; 32]).unwrap())
                .unwrap();
        let bls_pk3 =
            Bn256PublicKey::from_secret_key(&Bn256SecretKey::from_slice(&[3; 32]).unwrap())
                .unwrap();

        let block_headers = vec![BlockHeader::default()];
        let ars_identities = vec![p1.pkh(), p2.pkh(), p3.pkh()];
        let ordered_ars = vec![bls_pk1, bls_pk2, bls_pk3];
        let genesis_hash = Hash::default();
        let _sb1 = sbs
            .build_superblock(
                &block_headers,
                &ars_identities,
                &ordered_ars,
                0,
                genesis_hash,
            )
            .unwrap();

        let expected_sb2 = mining_build_superblock(
            &block_headers,
            &hash_key_leaves(&ordered_ars),
            1,
            genesis_hash,
        )
        .unwrap();
        let sb2_hash = expected_sb2.hash();

        // Receive a superblock vote for index 1 when we are in index 0
        let mut v1 = SuperBlockVote::new_unsigned(sb2_hash, 1);
        v1.secp256k1_signature.public_key = p1;
        assert_eq!(sbs.add_vote(&v1), AddSuperBlockVote::MaybeValid);
        // The vote is not inserted into votes_on_local_superlock because the local superblock is
        // still the one with index 0, while the vote has index 1
        assert_eq!(sbs.votes_on_local_superlock, HashSet::new());
        // Create the second superblock afterwards
        let sb2 = sbs
            .build_superblock(
                &block_headers,
                &ars_identities,
                &ordered_ars,
                1,
                genesis_hash,
            )
            .unwrap();
        assert_eq!(sb2, expected_sb2);
        let mut hh = HashSet::new();
        hh.insert(v1);
        assert_eq!(sbs.votes_on_local_superlock, hh);

        // Votes received during the next "superblock epoch" are also included
        // Receive a superblock vote for index 1 when we are in index 1
        let mut v2 = SuperBlockVote::new_unsigned(sb2_hash, 1);
        v2.secp256k1_signature.public_key = p2;
        assert_eq!(sbs.add_vote(&v2), AddSuperBlockVote::ValidWithSameHash);
        hh.insert(v2);
        assert_eq!(sbs.votes_on_local_superlock, hh);

        // But if we are in index 2 and receive a vote for index 1, the votes are simply marked as
        // "MaybeValid", they are not included in votes_on_local_superlock
        let _sb3 = sbs
            .build_superblock(
                &block_headers,
                &ars_identities,
                &ordered_ars,
                2,
                genesis_hash,
            )
            .unwrap();
        // Votes_on_local_superlock are cleared when the local superblock changes
        assert_eq!(sbs.votes_on_local_superlock, HashSet::new());
        let mut v3 = SuperBlockVote::new_unsigned(sb2_hash, 1);
        v3.secp256k1_signature.public_key = p3;
        assert_eq!(sbs.add_vote(&v3), AddSuperBlockVote::MaybeValid);
        assert_eq!(sbs.votes_on_local_superlock, HashSet::new());
    }

    #[test]
    fn test_hash_uncompressed_bn256key_leaves() {
        let bls_pk1 =
            Bn256PublicKey::from_secret_key(&Bn256SecretKey::from_slice(&[1; 32]).unwrap())
                .unwrap();
        let bls_pk2 =
            Bn256PublicKey::from_secret_key(&Bn256SecretKey::from_slice(&[2; 32]).unwrap())
                .unwrap();
        let bls_pk3 =
            Bn256PublicKey::from_secret_key(&Bn256SecretKey::from_slice(&[3; 32]).unwrap())
                .unwrap();
        let ordered_ars = vec![bls_pk1.clone(), bls_pk2.clone(), bls_pk3.clone()];

        let hashes = hash_key_leaves(&ordered_ars);

        let expected_hashes = [bls_pk1.hash(), bls_pk2.hash(), bls_pk3.hash()];

        let compressed_hashes = [
            Hash::SHA256(calculate_sha256(&bls_pk1.public_key).0),
            Hash::SHA256(calculate_sha256(&bls_pk2.public_key).0),
            Hash::SHA256(calculate_sha256(&bls_pk3.public_key).0),
        ];

        assert_ne!(hashes, compressed_hashes);
        assert_eq!(hashes, expected_hashes);
    }
}
