use crate::chain::{
    AltKeys, BlockHeader, Bn256PublicKey, CheckpointBeacon, Hash, Hashable, PublicKeyHash,
    SuperBlock, SuperBlockVote,
};
use std::{
    collections::{HashMap, HashSet},
    convert::{TryFrom, TryInto},
};

use serde::{Deserialize, Serialize};

use witnet_crypto::{hash::Sha256, merkle::merkle_tree_root as crypto_merkle_tree_root};

/// Possible result of SuperBlockState::add_vote
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum AddSuperBlockVote {
    /// vote already counted
    AlreadySeen,
    /// this identity has already voted for a different superblock with this index
    DoubleVote,
    /// invalid superblock index
    InvalidIndex,
    /// unverifiable vote because we do not have the required ARS state
    MaybeValid,
    /// vote from a peer not in the ARS
    NotInSigningCommittee,
    /// valid vote but with different hash
    ValidButDifferentHash,
    /// valid vote with identical hash
    ValidWithSameHash,
}

#[derive(Debug)]
/// Possible result of SuperBlockState::has_consensus
pub enum SuperBlockConsensus {
    /// The local superblock has the majority of votes, everything ok
    SameAsLocal,
    /// A different superblock has the majority of votes
    Different(Hash),
    /// No superblock candidate can achieve majority of votes
    NoConsensus,
    /// There are some missing votes that are needed to determine the consenus
    Unknown,
}

/// ARS identities
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct ARSIdentities {
    // HashSet of the identities in a specific ARS
    identities: HashSet<PublicKeyHash>,
    // Ordered vector of the identities in a specific ARS
    ordered_identities: Vec<PublicKeyHash>,
    // Alternative public Key mapping in a specific ARS
    alt_keys: AltKeys,
}

impl ARSIdentities {
    pub fn len(&self) -> usize {
        self.identities.len()
    }

    pub fn is_empty(&self) -> bool {
        self.identities.is_empty()
    }

    pub fn new(ordered_identities: Vec<PublicKeyHash>, alt_keys: AltKeys) -> Self {
        ARSIdentities {
            identities: ordered_identities.iter().cloned().collect(),
            ordered_identities,
            alt_keys,
        }
    }

    pub fn get_rep_ordered_bn256_list(&self) -> Vec<Bn256PublicKey> {
        self.ordered_identities
            .iter()
            .filter_map(|pkh| self.alt_keys.get_bn256(pkh).cloned())
            .collect()
    }
}

/// Mempool for SuperBlockVotes
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct SuperBlockVotesMempool {
    // Identities that commit an invalid SuperBlockVote and will be penalized
    penalized_identities: HashSet<PublicKeyHash>,
    // Set of received superblock votes
    // This is cleared when we try to create a new superblock
    received_votes: HashSet<SuperBlockVote>,
    // Map each identity to its superblock vote
    votes_of_each_identity: HashMap<PublicKeyHash, SuperBlockVote>,
    // Map of superblock_hash to votes to that superblock
    // This votes are valid according to the ARS check
    // This is cleared when we try to create a new superblock
    votes_on_each_superblock: HashMap<Hash, Vec<SuperBlockVote>>,
}

impl SuperBlockVotesMempool {
    fn contains(&self, sbv: &SuperBlockVote) -> bool {
        self.received_votes.contains(sbv)
    }

    fn insert(&mut self, sbv: &SuperBlockVote) {
        self.received_votes.insert(sbv.clone());
    }

    // Returns false if the identity voted more than once
    fn check_double_vote(&self, pkh: &PublicKeyHash) -> bool {
        self.penalized_identities.contains(pkh) || self.votes_of_each_identity.contains_key(pkh)
    }

    // Remove both votes and reject future votes by this identity
    fn override_vote(&mut self, pkh: PublicKeyHash) {
        if let Some(old_sbv) = self.votes_of_each_identity.remove(&pkh) {
            let v = self
                .votes_on_each_superblock
                .get_mut(&old_sbv.superblock_hash)
                .unwrap();
            let pos = v.iter().position(|x| *x == old_sbv).unwrap();
            v.swap_remove(pos);
        }
        self.penalized_identities.insert(pkh);
    }

    fn insert_vote(&mut self, sbv: SuperBlockVote) {
        let pkh = sbv.secp256k1_signature.public_key.pkh();
        self.votes_of_each_identity.insert(pkh, sbv.clone());

        self.votes_on_each_superblock
            .entry(sbv.superblock_hash)
            .or_default()
            .push(sbv);
    }

    fn get_received_votes(&self) -> HashSet<SuperBlockVote> {
        self.received_votes.clone()
    }

    fn get_valid_votes(&self) -> HashMap<Hash, Vec<SuperBlockVote>> {
        self.votes_on_each_superblock.clone()
    }

    fn clear_and_remove_votes(&mut self) -> Vec<SuperBlockVote> {
        self.votes_of_each_identity.clear();
        self.votes_on_each_superblock.clear();
        self.penalized_identities.clear();

        self.received_votes.drain().collect()
    }

    /// Returns the superblock hash and the number of votes of the most voted superblock.
    /// If the most voted superblock does not have a majority of votes, returns None.
    /// In case of tie, returns one of the superblocks with the most votes.
    /// If there are zero votes, returns None.
    pub fn most_voted_superblock(&self) -> Option<(Hash, usize)> {
        self.votes_on_each_superblock
            .iter()
            .map(|(superblock_hash, votes)| (*superblock_hash, votes.len()))
            .max_by_key(|&(_, num_votes)| num_votes)
    }

    fn valid_votes_counter(&self) -> usize {
        self.votes_of_each_identity.len()
    }

    fn invalid_votes_counter(&self) -> usize {
        self.penalized_identities.len()
    }
}

/// State related to superblocks
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct SuperBlockState {
    // Structure of the current Active Reputation Set identities
    ars_current_identities: ARSIdentities,
    // Structure of the previous Active Reputation Set identities
    ars_previous_identities: ARSIdentities,
    // Current superblock beacon including the superblock hash created by this node
    //and the current superblock index, used to limit the range of broadcasted votes to
    // [index - 1, index + 1]. So if index is 10, only votes with index 9, 10, 11 will be broadcasted
    current_superblock_beacon: CheckpointBeacon,
    // Subset of ARS in charge of signing the next superblock
    current_signing_committee: Option<HashSet<PublicKeyHash>>,
    // SuperBlockMempool
    votes_mempool: SuperBlockVotesMempool,
}

impl SuperBlockState {
    // Initialize the superblock state
    pub fn new(superblock_genesis_hash: Hash, bootstrap_committee: Vec<PublicKeyHash>) -> Self {
        let hs: HashSet<PublicKeyHash> = bootstrap_committee.iter().cloned().collect();
        Self {
            ars_current_identities: ARSIdentities {
                identities: hs,
                ordered_identities: vec![],
                alt_keys: AltKeys::default(),
            },
            current_superblock_beacon: CheckpointBeacon {
                checkpoint: 0,
                hash_prev_block: superblock_genesis_hash,
            },
            ..SuperBlockState::default()
        }
    }

    fn insert_vote(&mut self, sbv: SuperBlockVote) -> AddSuperBlockVote {
        log::debug!("Superblock insert vote {:?}", sbv);
        // If the superblock vote is valid, store it
        let pkh = sbv.secp256k1_signature.public_key.pkh();

        if self.votes_mempool.check_double_vote(&pkh) {
            // This identity has already voted for a different superblock
            self.votes_mempool.override_vote(pkh);

            AddSuperBlockVote::DoubleVote
        } else {
            let is_same_hash =
                sbv.superblock_hash == self.current_superblock_beacon.hash_prev_block;
            self.votes_mempool.insert_vote(sbv);

            if is_same_hash {
                AddSuperBlockVote::ValidWithSameHash
            } else {
                AddSuperBlockVote::ValidButDifferentHash
            }
        }
    }

    /// Add a vote sent by another peer.
    /// This method assumes that the signatures are valid, they must be checked by the caller.
    pub fn add_vote(&mut self, sbv: &SuperBlockVote) -> AddSuperBlockVote {
        let r = if self.votes_mempool.contains(sbv) {
            // Already processed before
            AddSuperBlockVote::AlreadySeen
        } else {
            // Insert to avoid validating again
            self.votes_mempool.insert(sbv);

            let valid = self.is_valid(sbv);

            match valid {
                Some(true) => self.insert_vote(sbv.clone()),
                Some(false) => {
                    if sbv.superblock_index == self.current_superblock_beacon.checkpoint
                        || self.ars_previous_identities.is_empty()
                    {
                        AddSuperBlockVote::NotInSigningCommittee
                    } else {
                        AddSuperBlockVote::InvalidIndex
                    }
                }
                None => AddSuperBlockVote::MaybeValid,
            }
        };
        // TODO: delete this log after testing
        log::debug!("Add vote: {:?}", r);

        r
    }

    /// Since we do not check signatures here, a superblock vote is valid if the signing identity
    /// is in the superblock signing committee.
    /// Returns true, false, or unknown
    fn is_valid(&self, sbv: &SuperBlockVote) -> Option<bool> {
        let superblock_index = self.current_superblock_beacon.checkpoint;
        if superblock_index == sbv.superblock_index {
            // If the index is the same as the current one, the vote is valid if it is signed by a
            // member of the current signing committee
            self.current_signing_committee.as_ref().map_or(Some(false), |x| {
                Some(x.contains(&sbv.secp256k1_signature.public_key.pkh()))
            })
        } else if ((superblock_index.saturating_sub(1))..=superblock_index.saturating_add(1))
            .contains(&sbv.superblock_index)
        {
            // If the index is not the same as the current one, but it is within an acceptable range
            // of [x-1, x+1], broadcast the vote without checking if it is a member of the ARS, as
            // the ARS may have changed and we do not keep older copies of the ARS in memory
            None
        } else {
            // If the index is outside the [x-1, x+1] range, it is considered not valid
            Some(false)
        }
    }

    /// Return true if the local superblock has the majority of votes
    pub fn has_consensus(&self) -> SuperBlockConsensus {
        log::debug!(
            "Superblock votes: {:?}",
            self.votes_mempool.get_valid_votes()
        );
        log::debug!("Previous ars: {:?}", self.current_signing_committee);
        // If current_signing_committee is None, this is the first superblock. The first superblock
        // is the one with index 0 and genesis hash. These are consensus constants and we do not
        // need any votes to determine that that is the most voted superblock.
        if self.current_signing_committee.is_none() {
            return SuperBlockConsensus::SameAsLocal;
        }
        let identities_that_can_vote = self.current_signing_committee.as_ref().unwrap().len();
        let (most_voted_superblock, most_voted_num_votes) =
            match self.votes_mempool.most_voted_superblock() {
                Some(x) => x,
                None => {
                    // 0 votes, no consensus
                    return SuperBlockConsensus::Unknown;
                }
            };

        if two_thirds_consensus(most_voted_num_votes, identities_that_can_vote) {
            if most_voted_superblock == self.current_superblock_beacon.hash_prev_block {
                SuperBlockConsensus::SameAsLocal
            } else {
                SuperBlockConsensus::Different(most_voted_superblock)
            }
        } else {
            let num_total_votes = self.votes_mempool.valid_votes_counter();
            let invalid_votes = self.votes_mempool.invalid_votes_counter();
            let num_missing_votes = identities_that_can_vote - num_total_votes - invalid_votes;
            if two_thirds_consensus(
                most_voted_num_votes + num_missing_votes,
                identities_that_can_vote,
            ) {
                // There is no consensus, but if the missing votes vote the same as the
                // majority, there can be consensus
                SuperBlockConsensus::Unknown
            } else {
                // There is no consensus, regardless of the missing votes
                SuperBlockConsensus::NoConsensus
            }
        }
    }

    fn update_ars_identities(&mut self, new_identities: ARSIdentities) {
        self.ars_previous_identities = std::mem::take(&mut self.ars_current_identities);
        self.ars_current_identities = new_identities;
    }

    /// Produces a `SuperBlock` that includes the blocks in `block_headers` if there is at least one of them.
    /// `ars_pkh_keys` will be used to validate all the superblock votes received for the
    /// next superblock. The votes for the current superblock must be validated using
    /// `ars_pkh_keys_ordered` will be used to calculate the superblock_signing_committee
    /// `previous_ars_identities`. The ordered bn256 keys will be merkelized and appended to the superblock
    pub fn build_superblock(
        &mut self,
        block_headers: &[BlockHeader],
        ars_ordered_identities: ARSIdentities,
        signing_committee_size: u32,
        superblock_index: u32,
        last_block_in_previous_superblock: Hash,
    ) -> SuperBlock {
        let key_leaves = hash_key_leaves(&ars_ordered_identities.get_rep_ordered_bn256_list());

        let superblock = mining_build_superblock(
            block_headers,
            &key_leaves,
            superblock_index,
            last_block_in_previous_superblock,
        );

        self.update_ars_identities(ars_ordered_identities);

        // Before updating the superblock_beacon, calculate the signing committee
        self.current_signing_committee = calculate_superblock_signing_committee(
            self.ars_previous_identities.clone(),
            signing_committee_size,
            self.current_superblock_beacon.hash_prev_block,
        );

        // update the superblock_beacon
        self.current_superblock_beacon = CheckpointBeacon {
            checkpoint: superblock_index,
            hash_prev_block: superblock.hash(),
        };

        let old_votes = self.votes_mempool.clear_and_remove_votes();
        for sbv in old_votes {
            let valid = self.is_valid(&sbv);

            // If the superblock vote is valid, store it
            if valid == Some(true) {
                self.insert_vote(sbv);
            }
        }

        superblock
    }

    /// Returns the last superblock hash and index.
    pub fn get_beacon(&self) -> CheckpointBeacon {
        self.current_superblock_beacon
    }

    /// Returns the current superblock votes.
    pub fn get_current_superblock_votes(&self) -> HashSet<SuperBlockVote> {
        self.votes_mempool.get_received_votes()
    }

    /// Check if we had already received this superblock vote
    pub fn contains(&self, sbv: &SuperBlockVote) -> bool {
        self.votes_mempool.contains(sbv)
    }
}

/// Calculates the superblock signing committee for a given superblock hash and ars
pub fn calculate_superblock_signing_committee(
    ars_identities: ARSIdentities,
    signing_committee_size: u32,
    superblock_hash: Hash,
) -> Option<HashSet<PublicKeyHash>> {
    // If the number of identities is lower than committee_size all the members of the ARS sign the superblock
    if ars_identities.len() < usize::try_from(signing_committee_size).unwrap() {
        Some(ars_identities.identities)
    } else {
        // Start counting the members of the subset from the superblock_hash
        let mut first = u32::from(*superblock_hash.as_ref().get(0).unwrap());
        first %= signing_committee_size;
        // Get the subset
        let subset = magic_partition(
            &ars_identities.ordered_identities.to_vec(),
            first.try_into().unwrap(),
            signing_committee_size.try_into().unwrap(),
        );
        let hs: HashSet<PublicKeyHash> = subset.iter().cloned().collect();
        Some(hs)
    }
}

// Take size element out of v.len() starting with element at index first:
// magic_partition(v, 3, 3), v=[0, 1, 2, 3, 4, 5].
// Will return elements at index 3, 5, 1.
fn magic_partition<T: Clone>(v: &[T], first: usize, size: usize) -> Vec<T> {
    if first >= v.len() {
        return vec![];
    }
    let each = v.len() / size;

    let mut v_subset = Vec::new();
    v_subset.push(v[first].clone());

    let mut a = (first + each) % v.len();
    while v_subset.len() < size {
        v_subset.push(v[a].clone());
        a = (a + each) % v.len();
    }

    v_subset
}

/// Returns true if the number of votes is enough to achieve 2/3 consensus.
///
/// The number of votes needed must be strictly greater than 2/3 of the number of identities.
pub fn two_thirds_consensus(votes: usize, identities: usize) -> bool {
    let required_votes = identities * 2 / 3;

    votes > required_votes
}

/// Produces a `SuperBlock` that includes the blocks in `block_headers` if there is at least one of them.
pub fn mining_build_superblock(
    block_headers: &[BlockHeader],
    ars_ordered_hash_leaves: &[Hash],
    index: u32,
    last_block_in_previous_superblock: Hash,
) -> SuperBlock {
    let last_block = block_headers.last();
    match last_block {
        None =>
        // Build "empty" Superblock (there were no blocks during super-epoch)
        {
            let ars_root = hash_merkle_tree_root(ars_ordered_hash_leaves);
            log::debug!(
                "Created superblock #{} with hash_prev_block {}, ARS {}, blocks []",
                index,
                last_block_in_previous_superblock,
                ars_root,
            );
            SuperBlock {
                ars_length: ars_ordered_hash_leaves.len() as u64,
                ars_root,
                index,
                last_block: last_block_in_previous_superblock,
                last_block_in_previous_superblock,
                ..Default::default()
            }
        }
        Some(last_block_header) => {
            let last_block_hash = last_block_header.hash();
            let merkle_drs: Vec<Hash> = block_headers
                .iter()
                .map(|b| b.merkle_roots.dr_hash_merkle_root)
                .collect();
            let merkle_tallies: Vec<Hash> = block_headers
                .iter()
                .map(|b| b.merkle_roots.tally_hash_merkle_root)
                .collect();

            let ars_root = hash_merkle_tree_root(ars_ordered_hash_leaves);
            let blocks: Vec<_> = block_headers
                .iter()
                .map(|b| format!("#{}: {}", b.beacon.checkpoint, b.hash()))
                .collect();
            log::debug!(
                "Created superblock #{} with hash_prev_block {}, ARS {}, blocks {:?}",
                index,
                last_block_in_previous_superblock,
                ars_root,
                blocks
            );

            SuperBlock {
                ars_length: ars_ordered_hash_leaves.len() as u64,
                data_request_root: hash_merkle_tree_root(&merkle_drs),
                tally_root: hash_merkle_tree_root(&merkle_tallies),
                ars_root,
                index,
                last_block: last_block_hash,
                last_block_in_previous_superblock,
            }
        }
    }
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
    use witnet_crypto::hash::{calculate_sha256, EMPTY_SHA256};

    #[test]
    fn test_superblock_creation_no_blocks() {
        let default_hash = Hash::default();
        let superblock = mining_build_superblock(&[], &[], 0, default_hash);

        let expected = SuperBlock {
            ars_length: 0,
            ars_root: Hash::from(EMPTY_SHA256),
            data_request_root: default_hash,
            index: 0,
            last_block: default_hash,
            last_block_in_previous_superblock: default_hash,
            tally_root: default_hash,
        };
        assert_eq!(superblock, expected);
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
            ars_root: default_hash,
            data_request_root: dr_merkle_root_1,
            index: 0,
            last_block: block.hash(),
            last_block_in_previous_superblock: default_hash,
            tally_root: tally_merkle_root_1,
        };

        let superblock = mining_build_superblock(&[block], &[default_hash], 0, default_hash);
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
            ars_root: default_hash,
            data_request_root: expected_superblock_dr_root,
            index: 0,
            last_block: block_2.hash(),
            last_block_in_previous_superblock: default_hash,
            tally_root: expected_superblock_tally_root,
        };

        let superblock =
            mining_build_superblock(&[block_1, block_2], &[default_hash], 0, default_hash);
        assert_eq!(superblock, expected_superblock);
    }

    #[test]
    fn superblock_state_default_add_votes() {
        // When the superblock state is initialized to default (for example when starting the node),
        // all the received superblock votes are marked as `NotInSigningCommittee`
        let mut sbs = SuperBlockState::default();

        let v1 = SuperBlockVote::new_unsigned(Hash::SHA256([1; 32]), 0);
        assert_eq!(sbs.add_vote(&v1), AddSuperBlockVote::NotInSigningCommittee);

        let v2 = SuperBlockVote::new_unsigned(Hash::SHA256([2; 32]), 0);
        assert_eq!(sbs.add_vote(&v2), AddSuperBlockVote::NotInSigningCommittee);

        // Before building the first superblock locally we do not know the current superblock_index,
        // so all the superblock votes will be "NotInSigningCommittee"
        let v3 = SuperBlockVote::new_unsigned(Hash::SHA256([3; 32]), 33);
        assert_eq!(sbs.add_vote(&v3), AddSuperBlockVote::NotInSigningCommittee);
    }

    #[test]
    fn superblock_state_initial_non_empty() {
        // When the superblock state is initialized to an initial state,
        // only the bootstrap committe votes are marked as valid
        let p1 = PublicKey::from_bytes([1; 33]);
        let p2 = PublicKey::from_bytes([2; 33]);

        let block_headers = vec![BlockHeader::default()];

        let ars1 = vec![p1.pkh()];
        let ars2 = vec![p2.pkh()];
        let mut sbs = SuperBlockState::new(Hash::default(), ars1);

        let sb1 = sbs.build_superblock(
            &block_headers,
            ARSIdentities::new(ars2, AltKeys::default()),
            100,
            0,
            Hash::default(),
        );
        let mut v0 = SuperBlockVote::new_unsigned(sb1.hash(), 0);

        v0.secp256k1_signature.public_key = p1;

        assert_eq!(sbs.add_vote(&v0), AddSuperBlockVote::ValidWithSameHash);

        let mut v1 = SuperBlockVote::new_unsigned(Hash::default(), 0);
        v1.secp256k1_signature.public_key = p2;

        assert_eq!(sbs.add_vote(&v1), AddSuperBlockVote::NotInSigningCommittee);
    }

    fn create_pkh(n: u8) -> PublicKeyHash {
        PublicKeyHash::from_bytes(&[n; 20]).unwrap()
    }

    fn create_bn256(n: u8) -> Bn256PublicKey {
        Bn256PublicKey::from_secret_key(&Bn256SecretKey::from_slice(&[n; 32]).unwrap()).unwrap()
    }

    fn create_alt_keys(pkhs: Vec<PublicKeyHash>, keys: Vec<Bn256PublicKey>) -> AltKeys {
        let mut alt_keys = AltKeys::default();
        for (pkh, bn256) in pkhs.iter().zip(keys.iter()) {
            alt_keys.insert_bn256(*pkh, bn256.clone());
        }

        alt_keys
    }

    #[test]
    fn superblock_state_first_superblock_cannot_be_validated() {
        // The first superblock built after starting the node cannot be validated because we need
        // the list of ARS members from the previous superblock
        let mut sbs = SuperBlockState::default();

        let block_headers = vec![BlockHeader::default()];
        let pkhs = vec![create_pkh(1)];
        let keys = vec![create_bn256(1)];
        let ars_identities = ARSIdentities::new(pkhs.clone(), create_alt_keys(pkhs, keys));
        let genesis_hash = Hash::default();
        let sb1 = sbs.build_superblock(&block_headers, ars_identities, 100, 0, genesis_hash);
        let v1 = SuperBlockVote::new_unsigned(sb1.hash(), 0);
        assert_eq!(sbs.add_vote(&v1), AddSuperBlockVote::NotInSigningCommittee);
    }

    #[test]
    fn superblock_state_first_superblock_empty() {
        // If there were no blocks, there will be a second empty superblock. The state is not
        // updated except for the superblock_index
        let mut sbs = SuperBlockState::default();

        let block_headers = vec![];
        let genesis_hash = Hash::default();
        let pkhs = vec![create_pkh(1)];
        let keys = vec![create_bn256(1)];
        let ars_identities = ARSIdentities::new(pkhs.clone(), create_alt_keys(pkhs, keys));

        let first_superblock =
            sbs.build_superblock(&block_headers, ars_identities.clone(), 100, 0, genesis_hash);

        let expected_superblock = SuperBlock {
            ars_length: 1,
            ars_root: hash_merkle_tree_root(&hash_key_leaves(
                &ars_identities.get_rep_ordered_bn256_list(),
            )),
            data_request_root: Hash::default(),
            index: 0,
            last_block: genesis_hash,
            last_block_in_previous_superblock: genesis_hash,
            tally_root: Hash::default(),
        };
        assert_eq!(first_superblock, expected_superblock);

        let expected_superblock_hash =
            "bcbead467194d639bc8162725db72495056b65c4ff8caf033f86f76c118874c9"
                .parse()
                .unwrap();

        let expected_sbs = SuperBlockState {
            ars_current_identities: ars_identities,
            current_signing_committee: Some(HashSet::new()),
            current_superblock_beacon: CheckpointBeacon {
                checkpoint: 0,
                hash_prev_block: expected_superblock_hash,
            },
            ars_previous_identities: ARSIdentities::default(),
            ..Default::default()
        };
        assert_eq!(sbs, expected_sbs);
    }

    #[test]
    fn superblock_state_second_superblock_empty() {
        let mut sbs = SuperBlockState::default();

        let block_headers = vec![BlockHeader::default()];
        let pkhs = vec![create_pkh(1)];
        let keys = vec![create_bn256(1)];
        let ars_identities = ARSIdentities::new(pkhs.clone(), create_alt_keys(pkhs, keys));

        let genesis_hash = Hash::default();

        sbs.build_superblock(&block_headers, ars_identities.clone(), 100, 0, genesis_hash);

        let expected_second_superblock = SuperBlock {
            ars_length: 1,
            ars_root: hash_merkle_tree_root(&hash_key_leaves(
                &ars_identities.get_rep_ordered_bn256_list(),
            )),
            data_request_root: Hash::default(),
            index: 1,
            last_block: genesis_hash,
            last_block_in_previous_superblock: genesis_hash,
            tally_root: Hash::default(),
        };
        let mut expected_sbs = sbs.clone();
        assert_eq!(
            sbs.build_superblock(&[], ars_identities.clone(), 100, 1, genesis_hash),
            expected_second_superblock
        );

        // The only think that should change is the superblock_index
        // And the superblock_hash, which will be set to the previous superblock
        expected_sbs.current_superblock_beacon = CheckpointBeacon {
            checkpoint: 1,
            hash_prev_block: expected_second_superblock.hash(),
        };
        expected_sbs.current_signing_committee = Some(ars_identities.identities.iter().cloned().collect());
        expected_sbs.ars_previous_identities = ars_identities;
        assert_eq!(sbs, expected_sbs);
    }

    #[test]
    fn superblock_state_already_seen() {
        // Check that no matter the internal state, the second time a vote is added, it will return
        // `AlreadySeen`
        let mut sbs = SuperBlockState::default();

        let v0 = SuperBlockVote::new_unsigned(Hash::SHA256([1; 32]), 0);
        assert_eq!(sbs.add_vote(&v0), AddSuperBlockVote::NotInSigningCommittee);
        assert_eq!(sbs.add_vote(&v0), AddSuperBlockVote::AlreadySeen);

        let block_headers = vec![BlockHeader::default()];
        let pkhs = vec![create_pkh(1)];
        let keys = vec![create_bn256(1)];
        let ars_identities = ARSIdentities::new(pkhs.clone(), create_alt_keys(pkhs, keys));

        let genesis_hash = Hash::default();
        let _sb1 =
            sbs.build_superblock(&block_headers, ars_identities.clone(), 100, 0, genesis_hash);
        // After building a new superblock the cache is invalidated but the previous ARS is still empty
        assert_eq!(sbs.add_vote(&v0), AddSuperBlockVote::NotInSigningCommittee);
        assert_eq!(sbs.add_vote(&v0), AddSuperBlockVote::AlreadySeen);

        let _sb2 = sbs.build_superblock(&block_headers, ars_identities, 100, 1, genesis_hash);
        let v1 = SuperBlockVote::new_unsigned(Hash::SHA256([2; 32]), 1);
        assert_eq!(sbs.add_vote(&v1), AddSuperBlockVote::NotInSigningCommittee);
        assert_eq!(sbs.add_vote(&v1), AddSuperBlockVote::AlreadySeen);

        let v2 = SuperBlockVote::new_unsigned(Hash::SHA256([3; 32]), 2);
        assert_eq!(sbs.add_vote(&v2), AddSuperBlockVote::MaybeValid);
        assert_eq!(sbs.add_vote(&v2), AddSuperBlockVote::AlreadySeen);

        let v3 = SuperBlockVote::new_unsigned(Hash::SHA256([4; 32]), 3);
        assert_eq!(sbs.add_vote(&v3), AddSuperBlockVote::InvalidIndex);
        assert_eq!(sbs.add_vote(&v3), AddSuperBlockVote::AlreadySeen);
    }

    #[test]
    fn superblock_state_double_vote() {
        // Check that an identity cannot vote for more than one superblock per index
        let mut sbs = SuperBlockState::default();
        let block_headers = vec![BlockHeader::default()];
        let genesis_hash = Hash::default();

        let p1 = PublicKey::from_bytes([1; 33]);
        let pkhs = vec![p1.pkh()];
        let keys = vec![create_bn256(1)];
        let ars0 = ARSIdentities::new(vec![], create_alt_keys(vec![], vec![]));
        let ars1 = ARSIdentities::new(pkhs.clone(), create_alt_keys(pkhs.clone(), keys.clone()));
        let ars2 = ARSIdentities::new(pkhs.clone(), create_alt_keys(pkhs, keys));

        // Superblock votes for index 0 cannot be validated because we do not know the ARS for index -1
        // (because it does not exist)
        let _sb0 = sbs.build_superblock(&block_headers, ars0, 100, 0, genesis_hash);

        // The ARS included in superblock 0 is empty, so none of the superblock votes for index 1
        // can be valid, they all return `NotInSigningCommittee`
        let _sb1 = sbs.build_superblock(&block_headers, ars1, 100, 1, genesis_hash);

        // The ARS included in superblock 1 contains only identity p1, so only its vote will be
        // valid in superblock votes for index 2
        let sb2 = sbs.build_superblock(&block_headers, ars2, 100, 2, genesis_hash);
        let mut v1 = SuperBlockVote::new_unsigned(sb2.hash(), 2);
        v1.secp256k1_signature.public_key = p1.clone();
        assert_eq!(sbs.add_vote(&v1), AddSuperBlockVote::ValidWithSameHash);
        let mut v2 = SuperBlockVote::new_unsigned(Hash::SHA256([2; 32]), 2);
        v2.secp256k1_signature.public_key = p1;
        assert_eq!(sbs.add_vote(&v2), AddSuperBlockVote::DoubleVote);
    }

    #[test]
    fn superblock_state_double_vote_on_different_epoch() {
        // Check that an identity cannot vote for more than one superblock per index, even if one
        // vote is received before we build the corresponding superblock
        let mut sbs = SuperBlockState::default();
        let block_headers = vec![BlockHeader::default()];
        let genesis_hash = Hash::default();

        let p1 = PublicKey::from_bytes([1; 33]);
        let pkhs = vec![p1.pkh()];
        let keys = vec![create_bn256(1)];
        let ars0 = ARSIdentities::new(vec![], create_alt_keys(vec![], vec![]));
        let ars1 = ARSIdentities::new(pkhs.clone(), create_alt_keys(pkhs.clone(), keys.clone()));
        let ars2 = ARSIdentities::new(pkhs.clone(), create_alt_keys(pkhs, keys));

        // Superblock votes for index 0 cannot be validated because we do not know the ARS for index -1
        // (because it does not exist)
        let _sb0 = sbs.build_superblock(&block_headers, ars0, 100, 0, genesis_hash);

        // The ARS included in superblock 0 is empty, so none of the superblock votes for index 1
        // can be valid, they all return `NotInSigningCommittee`
        let _sb1 = sbs.build_superblock(&block_headers, ars1, 100, 1, genesis_hash);

        let mut v2 = SuperBlockVote::new_unsigned(Hash::SHA256([2; 32]), 2);
        v2.secp256k1_signature.public_key = p1.clone();
        assert_eq!(sbs.add_vote(&v2), AddSuperBlockVote::MaybeValid);

        // The ARS included in superblock 1 contains only identity p1, so only its vote will be
        // valid in superblock votes for index 2
        let sb2 = sbs.build_superblock(&block_headers, ars2, 100, 2, genesis_hash);
        let mut v1 = SuperBlockVote::new_unsigned(sb2.hash(), 2);
        v1.secp256k1_signature.public_key = p1;
        assert_eq!(sbs.add_vote(&v1), AddSuperBlockVote::DoubleVote);
    }

    #[test]
    fn superblock_state_no_double_vote_if_index_is_different() {
        // Check that an identity can vote for one superblock with index i and for a different
        // superblock with index i+1 without any penalty
        let mut sbs = SuperBlockState::default();
        let block_headers = vec![BlockHeader::default()];
        let genesis_hash = Hash::default();

        let p1 = PublicKey::from_bytes([1; 33]);
        let pkhs = vec![p1.pkh()];
        let keys = vec![create_bn256(1)];
        let ars0 = ARSIdentities::new(vec![], create_alt_keys(vec![], vec![]));
        let ars1 = ARSIdentities::new(pkhs.clone(), create_alt_keys(pkhs.clone(), keys.clone()));
        let ars2 = ARSIdentities::new(pkhs.clone(), create_alt_keys(pkhs, keys));

        // Superblock votes for index 0 cannot be validated because we do not know the ARS for index -1
        // (because it does not exist)
        let _sb0 = sbs.build_superblock(&block_headers, ars0, 100, 0, genesis_hash);

        // The ARS included in superblock 0 is empty, so none of the superblock votes for index 1
        // can be valid, they all return `NotInSigningCommittee`
        let _sb1 = sbs.build_superblock(&block_headers, ars1, 100, 1, genesis_hash);

        // The ARS included in superblock 1 contains only identity p1, so only its vote will be
        // valid in superblock votes for index 2
        let sb2 = sbs.build_superblock(&block_headers, ars2, 100, 2, genesis_hash);
        let mut v1 = SuperBlockVote::new_unsigned(sb2.hash(), 2);
        v1.secp256k1_signature.public_key = p1.clone();
        assert_eq!(sbs.add_vote(&v1), AddSuperBlockVote::ValidWithSameHash);
        // This is a vote for index 3
        let mut v2 = SuperBlockVote::new_unsigned(Hash::SHA256([2; 32]), 3);
        v2.secp256k1_signature.public_key = p1;
        assert_eq!(sbs.add_vote(&v2), AddSuperBlockVote::MaybeValid);
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

        let ars0 = ARSIdentities::new(vec![], create_alt_keys(vec![], vec![]));
        let ars1 = ARSIdentities::new(
            vec![p1.pkh()],
            create_alt_keys(vec![p1.pkh()], vec![create_bn256(1)]),
        );
        let ars2 = ARSIdentities::new(
            vec![p2.pkh()],
            create_alt_keys(vec![p2.pkh()], vec![create_bn256(1)]),
        );
        let ars3 = ARSIdentities::new(
            vec![p3.pkh()],
            create_alt_keys(vec![p3.pkh()], vec![create_bn256(1)]),
        );
        let ars4 = ARSIdentities::new(vec![], create_alt_keys(vec![], vec![]));

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
        let sb0 = sbs.build_superblock(&block_headers, ars0, 100, 0, genesis_hash);
        let (v1, v2, v3) = create_votes(sb0.hash(), 0);
        assert_eq!(sbs.add_vote(&v1), AddSuperBlockVote::NotInSigningCommittee);
        assert_eq!(sbs.add_vote(&v2), AddSuperBlockVote::NotInSigningCommittee);
        assert_eq!(sbs.add_vote(&v3), AddSuperBlockVote::NotInSigningCommittee);

        // The ARS included in superblock 0 is empty, so none of the superblock votes for index 1
        // can be valid, they all return `NotInSigningCommittee`
        let sb1 = sbs.build_superblock(&block_headers, ars1, 100, 1, genesis_hash);
        let (v1, v2, v3) = create_votes(sb1.hash(), 1);
        assert_eq!(sbs.add_vote(&v1), AddSuperBlockVote::NotInSigningCommittee);
        assert_eq!(sbs.add_vote(&v2), AddSuperBlockVote::NotInSigningCommittee);
        assert_eq!(sbs.add_vote(&v3), AddSuperBlockVote::NotInSigningCommittee);

        // The ARS included in superblock 1 contains only identity p1, so only the vote v1 will be
        // valid in superblock votes for index 2
        let sb2 = sbs.build_superblock(&block_headers, ars2, 100, 2, genesis_hash);
        let (v1, v2, v3) = create_votes(sb2.hash(), 2);
        assert_eq!(sbs.add_vote(&v1), AddSuperBlockVote::ValidWithSameHash);
        assert_eq!(sbs.add_vote(&v2), AddSuperBlockVote::NotInSigningCommittee);
        assert_eq!(sbs.add_vote(&v3), AddSuperBlockVote::NotInSigningCommittee);

        // The ARS included in superblock 2 contains only identity p2, so only the vote v2 will be
        // valid in superblock votes for index 3
        let sb3 = sbs.build_superblock(&block_headers, ars3, 100, 3, genesis_hash);
        let (v1, v2, v3) = create_votes(sb3.hash(), 3);
        assert_eq!(sbs.add_vote(&v1), AddSuperBlockVote::NotInSigningCommittee);
        assert_eq!(sbs.add_vote(&v2), AddSuperBlockVote::ValidWithSameHash);
        assert_eq!(sbs.add_vote(&v3), AddSuperBlockVote::NotInSigningCommittee);

        // The ARS included in superblock 3 contains only identity p3, so only the vote v3 will be
        // valid in superblock votes for index 4
        let sb4 = sbs.build_superblock(&block_headers, ars4, 100, 4, genesis_hash);
        let (v1, v2, v3) = create_votes(sb4.hash(), 4);
        assert_eq!(sbs.add_vote(&v1), AddSuperBlockVote::NotInSigningCommittee);
        assert_eq!(sbs.add_vote(&v2), AddSuperBlockVote::NotInSigningCommittee);
        assert_eq!(sbs.add_vote(&v3), AddSuperBlockVote::ValidWithSameHash);
    }

    #[test]
    fn first_vote_validation() {
        // Tests the function first_vote_validation
        let mut sbs = SuperBlockState::default();
        let block_headers = vec![BlockHeader::default()];
        let genesis_hash = Hash::default();

        let p1 = PublicKey::from_bytes([1; 33]);
        let p2 = PublicKey::from_bytes([2; 33]);
        let p3 = PublicKey::from_bytes([3; 33]);

        let pkhs = vec![p1.pkh(), p2.pkh(), p3.pkh()];
        let keys = vec![create_bn256(1), create_bn256(2), create_bn256(3)];
        let ars_identities = ARSIdentities::new(pkhs.clone(), create_alt_keys(pkhs, keys));

        let create_votes = |superblock_hash, superblock_index| {
            let mut v1 = SuperBlockVote::new_unsigned(superblock_hash, superblock_index);
            v1.secp256k1_signature.public_key = p1.clone();
            let mut v2 = SuperBlockVote::new_unsigned(superblock_hash, superblock_index);
            v2.secp256k1_signature.public_key = p2.clone();
            let mut v3 = SuperBlockVote::new_unsigned(superblock_hash, superblock_index);
            v3.secp256k1_signature.public_key = p3.clone();
            let mut v4 = SuperBlockVote::new_unsigned(
                "0f0e2e43e928c8916ddad65c489dc9de196fef5b04438ea7af86499530ec28d5"
                    .parse()
                    .unwrap(),
                superblock_index,
            );
            v4.secp256k1_signature.public_key = p3.clone();

            (v1, v2, v3, v4)
        };

        // Superblock votes for index 0 cannot be validated because we do not know the ARS for index -1
        // (because it does not exist). When adding a vote it will return NotInSigningCommittee
        let sb0 =
            sbs.build_superblock(&block_headers, ars_identities.clone(), 100, 0, genesis_hash);
        let (v1, v2, v3, v4) = create_votes(sb0.hash(), 0);
        assert_eq!(sbs.add_vote(&v1), AddSuperBlockVote::NotInSigningCommittee);
        assert_eq!(sbs.add_vote(&v2), AddSuperBlockVote::NotInSigningCommittee);
        assert_eq!(sbs.add_vote(&v3), AddSuperBlockVote::NotInSigningCommittee);
        // If voting again, the vote should be set as AlreadySeen
        assert_eq!(sbs.add_vote(&v3), AddSuperBlockVote::AlreadySeen);
        assert_eq!(sbs.add_vote(&v4), AddSuperBlockVote::NotInSigningCommittee);

        // Create a superblock with the ars_identities
        let sb1 =
            sbs.build_superblock(&block_headers, ars_identities.clone(), 100, 1, genesis_hash);
        let (v1, _v2, v3, v4) = create_votes(sb1.hash(), 1);
        let mut v2 = SuperBlockVote::new_unsigned(
            "0f0e2e43e928c8916ddad65c489dc9de196fef5b04438ea7af86499530ec28d5"
                .parse()
                .unwrap(),
            1,
        );
        v2.secp256k1_signature.public_key = p2.clone();

        assert_eq!(sbs.add_vote(&v1), AddSuperBlockVote::ValidWithSameHash);
        // The vote v2 has different Hash than the current superblock hash
        assert_eq!(sbs.add_vote(&v2), AddSuperBlockVote::ValidButDifferentHash);
        assert_eq!(sbs.add_vote(&v3), AddSuperBlockVote::ValidWithSameHash);
        // If p3 votes a different superblock it should detect it as DoubleVote
        assert_eq!(sbs.add_vote(&v4), AddSuperBlockVote::DoubleVote);

        let sb2 = sbs.build_superblock(&block_headers, ars_identities, 100, 2, genesis_hash);

        let mut v1 = SuperBlockVote::new_unsigned(sb2.hash(), 5);
        v1.secp256k1_signature.public_key = p1;
        let mut v2 = SuperBlockVote::new_unsigned(sb2.hash(), 3);
        v2.secp256k1_signature.public_key = p2.clone();

        // let x be the current superblock index, when voting for an index grater than x + 1,
        // or smaller than x -1 the funciton should detected as InvalidIndex
        assert_eq!(sbs.add_vote(&v1), AddSuperBlockVote::InvalidIndex);
        // when voting for a different index but in [x-1, x+1], it should set the vote as MaybeValid
        assert_eq!(sbs.add_vote(&v2), AddSuperBlockVote::MaybeValid);
    }

    #[test]
    fn superblock_state_most_voted() {
        // Create 3 superblocks, where each one of them has an ARS with only one identity
        // This checks that the ARS is correctly set
        let mut sbs = SuperBlockState::default();
        let block_headers = vec![BlockHeader::default()];
        let genesis_hash = Hash::default();

        let p1 = PublicKey::from_bytes([1; 33]);
        let p2 = PublicKey::from_bytes([2; 33]);
        let p3 = PublicKey::from_bytes([3; 33]);
        let p4 = PublicKey::from_bytes([4; 33]);

        let pkhs0 = vec![p1.pkh(), p2.pkh(), p3.pkh()];
        let pkhs1 = vec![p1.pkh(), p2.pkh(), p3.pkh(), p4.pkh()];
        let pkhs2 = vec![p1.pkh(), p2.pkh(), p3.pkh(), p4.pkh()];

        let keys0 = vec![create_bn256(1), create_bn256(2), create_bn256(3)];
        let keys1 = vec![
            create_bn256(1),
            create_bn256(2),
            create_bn256(3),
            create_bn256(4),
        ];
        let keys2 = vec![
            create_bn256(1),
            create_bn256(2),
            create_bn256(3),
            create_bn256(4),
        ];

        let ars0 = ARSIdentities::new(pkhs0.clone(), create_alt_keys(pkhs0, keys0));
        let ars1 = ARSIdentities::new(pkhs1.clone(), create_alt_keys(pkhs1, keys1));
        let ars2 = ARSIdentities::new(pkhs2.clone(), create_alt_keys(pkhs2, keys2));

        let create_votes = |superblock_hash, superblock_index| {
            let mut v1 = SuperBlockVote::new_unsigned(superblock_hash, superblock_index);
            v1.secp256k1_signature.public_key = p1.clone();
            let mut v2 = SuperBlockVote::new_unsigned(superblock_hash, superblock_index);
            v2.secp256k1_signature.public_key = p2.clone();
            let mut v3 = SuperBlockVote::new_unsigned(superblock_hash, superblock_index);
            v3.secp256k1_signature.public_key = p3.clone();
            let mut v4 = SuperBlockVote::new_unsigned(superblock_hash, superblock_index);
            v4.secp256k1_signature.public_key = p4.clone();

            (v1, v2, v3, v4)
        };

        // Superblock votes for index 0 cannot be validated because we do not know the ARS for index -1
        // (because it does not exist)
        let sb0 = sbs.build_superblock(&block_headers, ars0, 100, 0, genesis_hash);
        let (v1, v2, v3, v4) = create_votes(sb0.hash(), 0);
        assert_eq!(sbs.add_vote(&v1), AddSuperBlockVote::NotInSigningCommittee);
        assert_eq!(sbs.add_vote(&v2), AddSuperBlockVote::NotInSigningCommittee);
        assert_eq!(sbs.add_vote(&v3), AddSuperBlockVote::NotInSigningCommittee);
        assert_eq!(sbs.add_vote(&v4), AddSuperBlockVote::NotInSigningCommittee);

        // The ARS included in superblock 0 contains identities p1, p2, p3
        let sb1 = sbs.build_superblock(&block_headers, ars1, 100, 1, genesis_hash);
        let (v1, v2, v3, v4) = create_votes(sb1.hash(), 1);
        assert_eq!(sbs.add_vote(&v1), AddSuperBlockVote::ValidWithSameHash);
        assert_eq!(
            sbs.votes_mempool.most_voted_superblock(),
            Some((sb1.hash(), 1))
        );
        assert_eq!(sbs.add_vote(&v2), AddSuperBlockVote::ValidWithSameHash);
        assert_eq!(
            sbs.votes_mempool.most_voted_superblock(),
            Some((sb1.hash(), 2))
        );
        assert_eq!(sbs.add_vote(&v3), AddSuperBlockVote::ValidWithSameHash);
        assert_eq!(
            sbs.votes_mempool.most_voted_superblock(),
            Some((sb1.hash(), 3))
        );
        assert_eq!(sbs.add_vote(&v4), AddSuperBlockVote::NotInSigningCommittee);
        assert_eq!(
            sbs.votes_mempool.most_voted_superblock(),
            Some((sb1.hash(), 3))
        );

        // The ARS included in superblock 1 contains identities p1, p2, p3, p4
        let sb2 = sbs.build_superblock(&block_headers, ars2, 100, 2, genesis_hash);
        let (v1, v2, v3, v4) = create_votes(sb2.hash(), 2);
        assert_eq!(sbs.add_vote(&v1), AddSuperBlockVote::ValidWithSameHash);
        assert_eq!(
            sbs.votes_mempool.most_voted_superblock(),
            Some((sb2.hash(), 1))
        );
        assert_eq!(sbs.add_vote(&v2), AddSuperBlockVote::ValidWithSameHash);
        assert_eq!(
            sbs.votes_mempool.most_voted_superblock(),
            Some((sb2.hash(), 2))
        );
        assert_eq!(sbs.add_vote(&v3), AddSuperBlockVote::ValidWithSameHash);
        assert_eq!(
            sbs.votes_mempool.most_voted_superblock(),
            Some((sb2.hash(), 3))
        );
        assert_eq!(sbs.add_vote(&v4), AddSuperBlockVote::ValidWithSameHash);
        assert_eq!(
            sbs.votes_mempool.most_voted_superblock(),
            Some((sb2.hash(), 4))
        );
    }

    #[test]
    fn superblock_state_check_on_build() {
        // When calling build_superblock, all the old superblock votes will be evaluated again, and
        // inserted into votes_on_each_superblock
        let mut sbs = SuperBlockState::default();

        let p1 = PublicKey::from_bytes([1; 33]);
        let p2 = PublicKey::from_bytes([2; 33]);
        let p3 = PublicKey::from_bytes([3; 33]);

        let pkhs = vec![p1.pkh(), p2.pkh(), p3.pkh()];
        let keys = vec![create_bn256(1), create_bn256(2), create_bn256(3)];
        let ars_identities = ARSIdentities::new(pkhs.clone(), create_alt_keys(pkhs, keys));

        let block_headers = vec![BlockHeader::default()];
        let genesis_hash = Hash::default();
        let _sb1 =
            sbs.build_superblock(&block_headers, ars_identities.clone(), 100, 0, genesis_hash);

        let expected_sb2 = mining_build_superblock(
            &block_headers,
            &hash_key_leaves(&ars_identities.get_rep_ordered_bn256_list()),
            1,
            genesis_hash,
        );
        let sb2_hash = expected_sb2.hash();

        // Receive a superblock vote for index 1 when we are in index 0
        let mut v1 = SuperBlockVote::new_unsigned(sb2_hash, 1);
        v1.secp256k1_signature.public_key = p1;
        assert_eq!(sbs.add_vote(&v1), AddSuperBlockVote::MaybeValid);
        // The vote is not inserted into votes_on_each_superblock because the local superblock is
        // still the one with index 0, while the vote has index 1
        assert_eq!(sbs.votes_mempool.get_valid_votes(), HashMap::new());
        // Create the second superblock afterwards
        let sb2 =
            sbs.build_superblock(&block_headers, ars_identities.clone(), 100, 1, genesis_hash);
        assert_eq!(sb2, expected_sb2);
        let mut hh: HashMap<_, Vec<_>> = HashMap::new();
        hh.entry(sb2_hash).or_default().push(v1);
        assert_eq!(sbs.votes_mempool.get_valid_votes(), hh);

        // Votes received during the next "superblock epoch" are also included
        // Receive a superblock vote for index 1 when we are in index 1
        let mut v2 = SuperBlockVote::new_unsigned(sb2_hash, 1);
        v2.secp256k1_signature.public_key = p2;
        assert_eq!(sbs.add_vote(&v2), AddSuperBlockVote::ValidWithSameHash);
        hh.entry(sb2_hash).or_default().push(v2);
        assert_eq!(sbs.votes_mempool.get_valid_votes(), hh);

        // But if we are in index 2 and receive a vote for index 1, the votes are simply marked as
        // "MaybeValid", they are not included in votes_on_local_superlock
        let _sb3 = sbs.build_superblock(&block_headers, ars_identities, 100, 2, genesis_hash);
        // votes_on_each_superblock are cleared when the local superblock changes
        assert_eq!(sbs.votes_mempool.get_valid_votes(), HashMap::new());
        let mut v3 = SuperBlockVote::new_unsigned(sb2_hash, 1);
        v3.secp256k1_signature.public_key = p3;
        assert_eq!(sbs.add_vote(&v3), AddSuperBlockVote::MaybeValid);
        assert_eq!(sbs.votes_mempool.get_valid_votes(), HashMap::new());
    }

    #[test]
    fn superblock_voted_by_signing_committee() {
        // When adding a superblock vote, it should be valid only by members of the
        // superblock signing committee.
        let mut sbs = SuperBlockState::default();

        let p1 = PublicKey::from_bytes([1; 33]);
        let p2 = PublicKey::from_bytes([2; 33]);
        let p3 = PublicKey::from_bytes([3; 33]);

        let pkhs = vec![p1.pkh(), p2.pkh(), p3.pkh()];
        let keys = vec![create_bn256(1), create_bn256(2), create_bn256(3)];
        let ars_identities = ARSIdentities::new(pkhs.clone(), create_alt_keys(pkhs, keys));

        let block_headers = vec![BlockHeader::default()];
        let genesis_hash = Hash::default();
        // superblock with index 0.
        let _sb1 = sbs.build_superblock(&block_headers, ars_identities.clone(), 2, 0, genesis_hash);
        // superblock with index 1
        let _sb2 = sbs.build_superblock(&block_headers, ars_identities.clone(), 2, 1, genesis_hash);

        let expected_sb2 = mining_build_superblock(
            &block_headers,
            &hash_key_leaves(&ars_identities.get_rep_ordered_bn256_list()),
            1,
            genesis_hash,
        );
        let sb2_hash = expected_sb2.hash();

        // Receive three superblock votes for index 1
        // Since the signing_committee_size is 2, one of the votes will not be valid
        let mut v1 = SuperBlockVote::new_unsigned(sb2_hash, 1);
        v1.secp256k1_signature.public_key = p1;
        assert_eq!(sbs.add_vote(&v1), AddSuperBlockVote::ValidWithSameHash);
        assert_eq!(
            sbs.votes_mempool.most_voted_superblock(),
            Some((sb2_hash, 1))
        );
        let mut v2 = SuperBlockVote::new_unsigned(sb2_hash, 1);
        v2.secp256k1_signature.public_key = p2;
        assert_eq!(sbs.add_vote(&v2), AddSuperBlockVote::ValidWithSameHash);
        assert_eq!(
            sbs.votes_mempool.most_voted_superblock(),
            Some((sb2_hash, 2))
        );
        let mut v3 = SuperBlockVote::new_unsigned(sb2_hash, 1);
        v3.secp256k1_signature.public_key = p3;
        assert_eq!(sbs.add_vote(&v3), AddSuperBlockVote::NotInSigningCommittee);
        assert_eq!(
            sbs.votes_mempool.most_voted_superblock(),
            Some((sb2_hash, 2))
        );
    }

    #[test]
    fn test_calculate_superblock_signing_committee() {
        // When the ARS has less members than the committee size it should
        // return the entire ARS as superblock signing committee.
        let mut sbs = SuperBlockState::default();

        let p1 = PublicKey::from_bytes([1; 33]);
        let p2 = PublicKey::from_bytes([2; 33]);
        let p3 = PublicKey::from_bytes([3; 33]);

        let pkhs = vec![p1.pkh(), p2.pkh(), p3.pkh()];
        let keys = vec![create_bn256(1), create_bn256(2), create_bn256(3)];
        let ars_identities = ARSIdentities::new(pkhs.clone(), create_alt_keys(pkhs, keys));

        let block_headers = vec![BlockHeader::default()];
        let genesis_hash = Hash::default();
        let _sb1 =
            sbs.build_superblock(&block_headers, ars_identities.clone(), 100, 0, genesis_hash);
        sbs.ars_previous_identities = ars_identities.clone();
        let committee_size = 4;
        let subset = calculate_superblock_signing_committee(
            sbs.ars_previous_identities,
            committee_size,
            sbs.current_superblock_beacon.hash_prev_block,
        );
        assert_eq!(ars_identities.len(), subset.unwrap().len());
    }

    #[test]
    fn test_calculate_superblock_signing_committee_2() {
        // It shpuld return a subset of 4 members from an ARS having size 8
        let mut sbs = SuperBlockState::default();

        let p1 = PublicKey::from_bytes([1; 33]);
        let p2 = PublicKey::from_bytes([2; 33]);
        let p3 = PublicKey::from_bytes([3; 33]);
        let p4 = PublicKey::from_bytes([4; 33]);
        let p5 = PublicKey::from_bytes([5; 33]);
        let p6 = PublicKey::from_bytes([6; 33]);
        let p7 = PublicKey::from_bytes([7; 33]);
        let p8 = PublicKey::from_bytes([8; 33]);

        let pkhs = vec![
            p1.pkh(),
            p2.pkh(),
            p3.pkh(),
            p4.pkh(),
            p5.pkh(),
            p6.pkh(),
            p7.pkh(),
            p8.pkh(),
        ];
        let ars_identities = ARSIdentities::new(pkhs.clone(), create_alt_keys(pkhs, vec![]));

        let block_headers = vec![BlockHeader::default()];
        let genesis_hash = Hash::default();
        let _sb1 = sbs.build_superblock(&block_headers, ars_identities.clone(), 4, 0, genesis_hash);
        sbs.ars_previous_identities = ars_identities;
        let committee_size = 4;
        let subset = calculate_superblock_signing_committee(
            sbs.ars_previous_identities,
            committee_size,
            sbs.current_superblock_beacon.hash_prev_block,
        );

        // The members of the signing_committee should be p3, p5, p7 and p1
        assert_eq!(
            subset
                .as_ref()
                .map_or(Some(false), |x| { Some(x.contains(&p3.pkh())) }),
            Some(true)
        );

        assert_eq!(
            subset
                .as_ref()
                .map_or(Some(false), |x| { Some(x.contains(&p5.pkh())) }),
            Some(true)
        );

        assert_eq!(
            subset
                .as_ref()
                .map_or(Some(false), |x| { Some(x.contains(&p7.pkh())) }),
            Some(true)
        );

        assert_eq!(
            subset
                .as_ref()
                .map_or(Some(false), |x| { Some(x.contains(&p1.pkh())) }),
            Some(true)
        );

        assert_eq!(
            usize::try_from(committee_size).unwrap(),
            subset.unwrap().len()
        );
    }

    #[test]
    fn test_magic_particion() {
        // Tests the magic partition function
        let empty_vec: Vec<i32> = vec![];

        assert_eq!(magic_partition(&empty_vec, 0, 5), empty_vec);
        assert_eq!(
            magic_partition(&[0, 1, 2, 3, 4, 5, 6], 0, 5),
            vec![0, 1, 2, 3, 4]
        );
        assert_eq!(
            magic_partition(&[0, 1, 2, 3, 4, 5, 6], 4, 5),
            vec![4, 5, 6, 0, 1]
        );
        assert_eq!(
            magic_partition(&[0, 1, 2, 3, 4, 5, 6], 2, 5),
            vec![2, 3, 4, 5, 6]
        );
        assert_eq!(
            magic_partition(&[0, 1, 2, 3, 4, 5, 6], 3, 5),
            vec![3, 4, 5, 6, 0]
        );
        assert_eq!(magic_partition(&[0, 1, 2, 3, 4, 5, 6], 4, 2), vec![4, 0]);
        assert_eq!(
            magic_partition(&[0, 1, 2, 3, 4, 5, 6], 5, 6),
            vec![5, 6, 0, 1, 2, 3]
        );
        assert_eq!(magic_partition(&[0, 1, 2, 3, 4, 5, 6], 6, 3), vec![6, 1, 3]);
        assert_eq!(
            magic_partition(&[0, 1, 2, 3, 4, 5, 6], 1, 5),
            vec![1, 2, 3, 4, 5]
        );
    }

    #[test]
    fn test_hash_uncompressed_bn256key_leaves() {
        let bls_pk1 = create_bn256(1);
        let bls_pk2 = create_bn256(2);
        let bls_pk3 = create_bn256(3);
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

    #[test]
    fn test_get_beacon_1() {
        let superblock_state = SuperBlockState::default();
        let beacon = superblock_state.get_beacon();

        assert_eq!(
            beacon,
            CheckpointBeacon {
                checkpoint: 0,
                hash_prev_block: Hash::default(),
            }
        );
    }

    #[test]
    fn test_get_beacon_2() {
        let superblock_state = SuperBlockState {
            ars_current_identities: ARSIdentities::default(),
            current_superblock_beacon: CheckpointBeacon {
                checkpoint: 0,
                hash_prev_block: Hash::SHA256([1; 32]),
            },
            ars_previous_identities: ARSIdentities::default(),
            ..Default::default()
        };
        let beacon = superblock_state.get_beacon();

        assert_eq!(
            beacon,
            CheckpointBeacon {
                checkpoint: 0,
                hash_prev_block: Hash::SHA256([1; 32]),
            }
        );
    }

    #[test]
    fn test_get_beacon_3() {
        let superblock_state = SuperBlockState {
            ars_current_identities: ARSIdentities::default(),
            current_superblock_beacon: CheckpointBeacon {
                checkpoint: 1,
                hash_prev_block: Hash::default(),
            },
            ars_previous_identities: ARSIdentities::default(),
            ..Default::default()
        };
        let beacon = superblock_state.get_beacon();

        assert_eq!(
            beacon,
            CheckpointBeacon {
                checkpoint: 1,
                hash_prev_block: Hash::default(),
            }
        );
    }

    #[test]
    fn test_two_thirds_consensus() {
        assert_eq!(two_thirds_consensus(2, 3), false);
        assert_eq!(two_thirds_consensus(3, 3), true);
        assert_eq!(two_thirds_consensus(2, 4), false);
        assert_eq!(two_thirds_consensus(3, 4), true);
        assert_eq!(two_thirds_consensus(21, 32), false);
        assert_eq!(two_thirds_consensus(22, 32), true);
        assert_eq!(two_thirds_consensus(22, 33), false);
        assert_eq!(two_thirds_consensus(23, 33), true);
        assert_eq!(two_thirds_consensus(22, 34), false);
        assert_eq!(two_thirds_consensus(23, 34), true);
    }
}
