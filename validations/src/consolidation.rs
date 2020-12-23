use crate::validations::{validate_block, validate_block_transactions, verify_signatures};
use std::collections::HashMap;
use std::convert::TryFrom;
use witnet_config::config::consensus_constants_from_partial;
use witnet_crypto::key::CryptoEngine;
use witnet_data_structures::chain::{
    penalize_factor, reputation_issuance, Alpha, AltKeys, Block, ChainInfo, ChainState,
    CheckpointBeacon, CheckpointVRF, ConsensusConstants, DataRequestInfo, Environment, Epoch,
    EpochConstants, Hashable, NodeStats, PartialConsensusConstants, PublicKeyHash, Reputation,
    ReputationEngine, StateMachine, TransactionsPool,
};
use witnet_data_structures::data_request::DataRequestPool;
use witnet_data_structures::superblock::SuperBlockState;
use witnet_data_structures::transaction::{RevealTransaction, TallyTransaction};
use witnet_data_structures::utxo_pool::{Diff, OwnUnspentOutputsPool, UnspentOutputsPool};
use witnet_data_structures::vrf::VrfCtx;

/// Result of updating the reputation: number of new witnessing acts and statistics about every
/// identity that participated (number of truths, lies, and errors).
#[derive(Debug, Default)]
pub struct ReputationInfo {
    /// Counter of "witnessing acts".
    /// For every data request with a tally in this block, increment alpha_diff
    /// by the number of reveals present in the tally.
    pub alpha_diff: Alpha,

    /// Map used to count the witnesses results in one epoch
    pub result_count: HashMap<PublicKeyHash, RequestResult>,
}

impl ReputationInfo {
    /// Create new empty `ReputationInfo`
    pub fn new() -> Self {
        Self::default()
    }

    /// Update with `TallyTransaction`
    pub fn update(
        &mut self,
        tally_transaction: &TallyTransaction,
        data_request_pool: &DataRequestPool,
        own_pkh: PublicKeyHash,
        node_stats: &mut NodeStats,
    ) {
        let dr_pointer = tally_transaction.dr_pointer;
        let dr_state = &data_request_pool.data_request_pool[&dr_pointer];
        let commits = &dr_state.info.commits;
        // 1 reveal = 1 witnessing act
        let reveals_count = u32::try_from(dr_state.info.reveals.len()).unwrap();
        self.alpha_diff += Alpha(reveals_count);

        // Set of pkhs which were slashed in the tally transaction
        let out_of_consensus = &tally_transaction.out_of_consensus;
        let error_committers = &tally_transaction.error_committers;
        for pkh in commits.keys() {
            let result = self.result_count.entry(*pkh).or_default();
            if error_committers.contains(pkh) {
                result.errors += 1;
            } else if out_of_consensus.contains(pkh) {
                result.lies += 1;
            } else {
                result.truths += 1;
            }
        }

        // Update node stats
        if out_of_consensus.contains(&own_pkh) && !error_committers.contains(&own_pkh) {
            node_stats.slashed_count += 1;
        }
    }
}

/// This struct count the number of truths, lies and errors committed by an identity
#[derive(Debug, Default, Clone, Eq, PartialEq)]
pub struct RequestResult {
    /// Truths
    pub truths: u32,
    /// Lies
    pub lies: u32,
    /// Errors
    pub errors: u32,
}

fn separate_honest_errors_and_liars<K, I>(rep_info: I) -> (Vec<K>, Vec<K>, Vec<(K, u32)>)
where
    I: IntoIterator<Item = (K, RequestResult)>,
{
    let mut honests = vec![];
    let mut liars = vec![];
    let mut errors = vec![];
    for (pkh, result) in rep_info {
        if result.lies > 0 {
            liars.push((pkh, result.lies));
        // TODO: Decide which percentage would be fair enough
        } else if result.truths >= result.errors {
            honests.push(pkh);
        } else {
            errors.push(pkh);
        }
    }

    (honests, errors, liars)
}

/// Result of updating the `ReputationEngine` when consolidating a `Block`
pub struct UpdateReputationResult {
    /// Number of witnessing acts before updating reputation
    pub old_alpha: Alpha,
    /// alpha_diff and result_count
    pub reputation_info: ReputationInfo,
    /// Amount of reputation slashed to `own_pkh`
    pub own_slashed_rep: Option<Reputation>,
    /// Leftover reputation from the previous epoch
    pub extra_rep_previous_epoch: Reputation,
    /// Reputation that expired
    pub expired_rep: Reputation,
    /// Reputation that was created
    pub issued_rep: Reputation,
    /// Reputation subtracted from dishonest identities
    pub penalized_rep: Reputation,
    /// Total reputation that can be divided amongst all the honest identities
    pub reputation_bounty: Reputation,
    /// Amount of reputation added to `own_pkh`
    pub own_gained_rep: Option<Reputation>,
    /// Reputation gained by each identity
    pub gained_rep: Reputation,
    /// Number of honest identities
    pub num_honest: u32,
    /// Total reputation rewarded to nodes
    pub rep_reward: Reputation,
    /// Leftover reputation for the next epoch
    pub extra_reputation: Reputation,
}

/// Update `ReputationEngine` and `AltKeys` using the provided `ReputationInfo`
// FIXME(#676): Remove clippy skip error
#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cognitive_complexity,
    clippy::too_many_arguments
)]
pub fn update_reputation(
    rep_eng: &mut ReputationEngine,
    secp_bls_mapping: &mut AltKeys,
    consensus_constants: &ConsensusConstants,
    miner_pkh: PublicKeyHash,
    ReputationInfo {
        alpha_diff,
        result_count,
    }: ReputationInfo,
    block_epoch: Epoch,
    own_pkh: PublicKeyHash,
) -> UpdateReputationResult {
    let old_alpha = rep_eng.current_alpha();
    let new_alpha = Alpha(old_alpha.0 + alpha_diff.0);
    let (honests, _errors, liars) = separate_honest_errors_and_liars(result_count.clone());
    let revealers = result_count.keys().copied();
    // Leftover reputation from the previous epoch
    let extra_rep_previous_epoch = rep_eng.extra_reputation;
    // Expire in old_alpha to maximize reputation lost in penalizations.
    // Example: we are in old_alpha 10000, new_alpha 5 and some reputation expires in
    // alpha 10002. This reputation will expire in the next epoch.
    let expired_rep = rep_eng.trs_mut().expire(&old_alpha);
    // There is some reputation issued for every witnessing act
    let issued_rep = reputation_issuance(
        Reputation(consensus_constants.reputation_issuance),
        Alpha(consensus_constants.reputation_issuance_stop),
        old_alpha,
        new_alpha,
    );
    let own_rep = rep_eng.trs().get(&own_pkh);
    let mut own_slashed_rep = None;
    // Penalize liars and accumulate the reputation
    // The penalization depends on the number of lies from the last epoch
    let liars_and_penalize_function = liars.iter().map(|(pkh, num_lies)| {
        if own_pkh == *pkh {
            let after_slashed_rep = f64::from(own_rep.0)
                * consensus_constants
                    .reputation_penalization_factor
                    .powf(f64::from(*num_lies));
            let slashed_rep = Reputation(own_rep.0 - (after_slashed_rep as u32));
            // TODO: I assume that `own_pkh == *pkh` will only be true once
            assert_eq!(own_slashed_rep, None);
            own_slashed_rep = Some(slashed_rep);
        }

        (
            pkh,
            penalize_factor(
                consensus_constants.reputation_penalization_factor,
                *num_lies,
            ),
        )
    });
    let penalized_rep = rep_eng
        .trs_mut()
        .penalize_many(liars_and_penalize_function)
        .unwrap();

    let mut reputation_bounty = extra_rep_previous_epoch;
    reputation_bounty += expired_rep;
    reputation_bounty += issued_rep;
    reputation_bounty += penalized_rep;

    let mut own_gained_rep = None;
    let mut gained_rep = Reputation(0);
    let mut rep_reward = Reputation(0);
    let num_honest = u32::try_from(honests.len()).unwrap();

    // Gain reputation
    if num_honest > 0 {
        rep_reward = Reputation(reputation_bounty.0 / num_honest);
        // Expiration starts counting from new_alpha.
        // All the reputation earned in this block will expire at the same time.
        let expire_alpha = Alpha(new_alpha.0 + consensus_constants.reputation_expire_alpha_diff);
        let honest_gain = honests.into_iter().map(|pkh| {
            if own_pkh == pkh {
                // TODO: I assume that `own_pkh == *pkh` will only be true once
                assert_eq!(own_gained_rep, None);
                own_gained_rep = Some(rep_reward);
            }
            (pkh, rep_reward)
        });
        rep_eng.trs_mut().gain(expire_alpha, honest_gain).unwrap();

        gained_rep = Reputation(rep_reward.0 * num_honest);
        reputation_bounty -= gained_rep;
    }

    let extra_reputation = reputation_bounty;
    rep_eng.extra_reputation = extra_reputation;

    // Update active reputation set
    // Add block miner pkh to active identities
    if let Err(e) = rep_eng
        .ars_mut()
        .update(revealers.chain(vec![miner_pkh]), block_epoch)
    {
        log::error!("Error updating reputation in consolidation: {}", e);
    }

    // Retain identities that exist in the ARS
    secp_bls_mapping.retain(|k| rep_eng.is_ars_member(k));

    rep_eng.set_current_alpha(new_alpha);

    UpdateReputationResult {
        old_alpha,
        reputation_info: ReputationInfo {
            alpha_diff,
            result_count,
        },
        own_slashed_rep,
        extra_rep_previous_epoch,
        expired_rep,
        issued_rep,
        penalized_rep,
        reputation_bounty,
        own_gained_rep,
        gained_rep,
        num_honest,
        rep_reward,
        extra_reputation,
    }
}

/// Update `UnspentOutputsPool`, `DataRequestPool`, and `TransactionsPool` with a new consolidated
/// `Block` and its `Diff`.
#[allow(clippy::too_many_arguments)]
pub fn update_pools(
    block: &Block,
    unspent_outputs_pool: &mut UnspentOutputsPool,
    data_request_pool: &mut DataRequestPool,
    transactions_pool: &mut TransactionsPool,
    utxo_diff: Diff,
    own_pkh: PublicKeyHash,
    own_utxos: &mut OwnUnspentOutputsPool,
    epoch_constants: EpochConstants,
    node_stats: &mut NodeStats,
    state_machine: StateMachine,
) -> ReputationInfo {
    let mut rep_info = ReputationInfo::new();

    for ta_tx in &block.txns.tally_txns {
        // Process tally transactions: used to update reputation engine
        rep_info.update(&ta_tx, data_request_pool, own_pkh, node_stats);

        // IMPORTANT: Update the data request pool after updating reputation info
        if let Err(e) = data_request_pool.process_tally(&ta_tx, &block.hash()) {
            log::error!("Error processing tally transaction:\n{}", e);
        }
    }

    for vt_tx in &block.txns.value_transfer_txns {
        transactions_pool.vt_remove(&vt_tx);
    }

    for dr_tx in &block.txns.data_request_txns {
        if let Err(e) = data_request_pool.process_data_request(
            &dr_tx,
            block.block_header.beacon.checkpoint,
            epoch_constants,
            &block.hash(),
        ) {
            log::error!("Error processing data request transaction:\n{}", e);
        } else {
            transactions_pool.dr_remove(&dr_tx);
        }
    }

    for co_tx in &block.txns.commit_txns {
        if let Err(e) = data_request_pool.process_commit(&co_tx, &block.hash()) {
            log::error!("Error processing commit transaction:\n{}", e);
        } else {
            if co_tx.body.proof.proof.pkh() == own_pkh {
                node_stats.commits_count += 1;
                if state_machine != StateMachine::Synced {
                    // During synchronization, we assume that every consolidated commit had,
                    // at least, one data requests valid proof and one commit proposed
                    node_stats.dr_eligibility_count += 1;
                    node_stats.commits_proposed_count += 1;
                }
            }
            transactions_pool.remove_inputs(&co_tx.body.collateral);
        }
    }

    for re_tx in &block.txns.reveal_txns {
        if let Err(e) = data_request_pool.process_reveal(&re_tx, &block.hash()) {
            log::error!("Error processing reveal transaction:\n{}", e);
        }
    }

    // Remove reveals because they expire every consolidated block
    transactions_pool.clear_reveals();

    // Update own_utxos
    utxo_diff.visit(
        own_utxos,
        |own_utxos, output_pointer, output| {
            // Insert new outputs
            if output.pkh == own_pkh {
                own_utxos.insert(output_pointer.clone(), 0);
            }
        },
        |own_utxos, output_pointer| {
            // Remove spent inputs
            own_utxos.remove(&output_pointer);
        },
    );

    utxo_diff.apply(unspent_outputs_pool);

    rep_info
}

/// Result of consolidating a `Block`. This includes a list of resolved data requests and a list of
/// reveals that should be broadcasted to the network.
pub struct ConsolidateBlockResult {
    /// List of resolved data requests that should be persisted into storage
    pub to_be_stored: Vec<DataRequestInfo>,
    /// List of reveal transactions that should be broadcasted to the network
    pub reveals: Vec<RevealTransaction>,
    /// Info about reputation update
    pub reputation_update: Option<UpdateReputationResult>,
    /// True if this block was mined by `own_pkh`
    pub congratulations: bool,
}

/// Simplified version of ChainManager used for testing
pub struct ChainStateTest {
    chain_state: ChainState,
    transactions_pool: TransactionsPool,
    sm_state: StateMachine,
    own_pkh: PublicKeyHash,
    epoch_constants: EpochConstants,
    consensus_constants: ConsensusConstants,
}

impl ChainStateTest {
    /// Create new empty `ChainStateTest`
    // TODO: initialize using consensus constants
    pub fn new() -> Self {
        // TODO: set consensus constants using parameter?
        let consensus_constants = consensus_constants_from_partial(
            &PartialConsensusConstants::default(),
            &witnet_config::defaults::Mainnet,
        );
        let environment = Environment::Mainnet;

        let chain_state = {
            // Create a new ChainInfo
            let bootstrap_hash = consensus_constants.bootstrap_hash;
            let reputation_engine =
                ReputationEngine::new(consensus_constants.activity_period as usize);
            let hash_prev_block = bootstrap_hash;

            let chain_info = ChainInfo {
                environment,
                consensus_constants: consensus_constants.clone(),
                highest_block_checkpoint: CheckpointBeacon {
                    checkpoint: 0,
                    hash_prev_block,
                },
                highest_superblock_checkpoint: CheckpointBeacon {
                    checkpoint: 0,
                    hash_prev_block,
                },
                highest_vrf_output: CheckpointVRF {
                    checkpoint: 0,
                    hash_prev_vrf: hash_prev_block,
                },
            };

            let bootstrap_committee = chain_info
                .consensus_constants
                .bootstrapping_committee
                .iter()
                .map(|add| add.parse().expect("Malformed bootstrapping committee"))
                .collect();
            let superblock_state = SuperBlockState::new(bootstrap_hash, bootstrap_committee);

            ChainState {
                chain_info: Some(chain_info),
                reputation_engine: Some(reputation_engine),
                own_utxos: OwnUnspentOutputsPool::new(),
                data_request_pool: DataRequestPool::new(consensus_constants.extra_rounds),
                superblock_state,
                ..ChainState::default()
            }
        };

        Self {
            chain_state,
            transactions_pool: Default::default(),
            sm_state: StateMachine::Synced,
            own_pkh: Default::default(),
            epoch_constants: EpochConstants {
                checkpoint_zero_timestamp: consensus_constants.checkpoint_zero_timestamp,
                checkpoints_period: consensus_constants.checkpoints_period,
            },
            consensus_constants,
        }
    }

    /// Validate block
    pub fn validate_block(
        &self,
        block: &Block,
        vrf_ctx: &mut VrfCtx,
        secp_ctx: &CryptoEngine,
    ) -> Result<Diff, failure::Error> {
        // current_epoch is only used to check for blocks from the future, so set it to the max
        // value to avoid that error
        let current_epoch = Epoch::MAX;
        let block_number = self.chain_state.block_number();
        let chain_info = self.chain_state.chain_info.as_ref().unwrap();
        let mut vrf_input = chain_info.highest_vrf_output;
        vrf_input.checkpoint = block.block_header.beacon.checkpoint;
        let chain_beacon = chain_info.highest_block_checkpoint;
        let rep_eng = self.chain_state.reputation_engine.as_ref().unwrap();
        let epoch_constants = self.epoch_constants;
        let consensus_constants = &self.consensus_constants;
        let utxo_set = &self.chain_state.unspent_outputs_pool;
        let dr_pool = &self.chain_state.data_request_pool;

        let mut signatures_to_verify = vec![];
        validate_block(
            block,
            current_epoch,
            vrf_input,
            chain_beacon,
            &mut signatures_to_verify,
            rep_eng,
            consensus_constants,
        )?;
        verify_signatures(signatures_to_verify, vrf_ctx, secp_ctx)?;

        let mut signatures_to_verify = vec![];
        let utxo_dif = validate_block_transactions(
            utxo_set,
            dr_pool,
            block,
            vrf_input,
            &mut signatures_to_verify,
            rep_eng,
            epoch_constants,
            block_number,
            consensus_constants,
        )?;
        verify_signatures(signatures_to_verify, vrf_ctx, secp_ctx)?;

        Ok(utxo_dif)
    }

    /// Consolidates block assuming that it is valid.
    pub fn consolidate_block(
        &mut self,
        block: &Block,
        utxo_diff: Diff,
        vrf_ctx: &mut VrfCtx,
    ) -> ConsolidateBlockResult {
        consolidate_block(
            &mut self.chain_state,
            &mut self.transactions_pool,
            vrf_ctx,
            block,
            utxo_diff,
            self.sm_state,
            self.own_pkh,
            self.epoch_constants,
        )
    }
}

/// Consolidates block assuming that it is valid.
pub fn consolidate_block(
    chain_state: &mut ChainState,
    transactions_pool: &mut TransactionsPool,
    vrf_ctx: &mut VrfCtx,
    block: &Block,
    utxo_diff: Diff,
    sm_state: StateMachine,
    own_pkh: PublicKeyHash,
    epoch_constants: EpochConstants,
) -> ConsolidateBlockResult {
    let block_hash = block.hash();
    let block_epoch = block.block_header.beacon.checkpoint;

    // Update `highest_block_checkpoint`
    let beacon = CheckpointBeacon {
        checkpoint: block_epoch,
        hash_prev_block: block_hash,
    };

    // Decide the input message for the VRF of this block candidate:
    // If the candidate builds right on top of the genesis block, use candidate's own checkpoint and the genesis block hash.
    // Else, use use candidate's own checkpoint and the hash of the VRF proof from the block it builds on.
    let vrf_input = match block_epoch {
        0 => CheckpointVRF {
            checkpoint: block_epoch,
            hash_prev_vrf: block_hash,
        },
        _ => {
            let proof_hash = block.block_header.proof.proof_to_hash(vrf_ctx).unwrap();
            CheckpointVRF {
                checkpoint: block_epoch,
                hash_prev_vrf: proof_hash,
            }
        }
    };

    let chain_info = chain_state.chain_info.as_mut().unwrap();
    let reputation_engine = chain_state.reputation_engine.as_mut().unwrap();
    // Update beacon and vrf output
    chain_info.highest_block_checkpoint = beacon;
    chain_info.highest_vrf_output = vrf_input;

    let rep_info = update_pools(
        &block,
        &mut chain_state.unspent_outputs_pool,
        &mut chain_state.data_request_pool,
        transactions_pool,
        utxo_diff,
        own_pkh,
        &mut chain_state.own_utxos,
        epoch_constants,
        &mut chain_state.node_stats,
        sm_state,
    );

    let miner_pkh = block.block_header.proof.proof.pkh();

    let mut reputation_update = None;
    // Do not update reputation when consolidating genesis block
    if block_hash != chain_info.consensus_constants.genesis_hash {
        reputation_update = Some(update_reputation(
            reputation_engine,
            &mut chain_state.alt_keys,
            &chain_info.consensus_constants,
            miner_pkh,
            rep_info,
            block_epoch,
            own_pkh,
        ));
    }

    // Update bn256 public keys with block information
    chain_state.alt_keys.insert_keys_from_block(&block);

    // Insert candidate block into `block_chain` state
    chain_state.block_chain.insert(block_epoch, block_hash);

    let to_be_stored = chain_state.data_request_pool.finished_data_requests();

    let reveals = chain_state.data_request_pool.update_data_request_stages();

    let mut congratulations = false;
    if miner_pkh == own_pkh {
        chain_state.node_stats.block_mined_count += 1;
        if sm_state == StateMachine::Synced {
            congratulations = true;
        } else {
            // During synchronization, we assume that every consolidated block has, at least, one proposed block.
            chain_state.node_stats.block_proposed_count += 1;
        }
    }

    ConsolidateBlockResult {
        to_be_stored,
        reveals,
        reputation_update,
        congratulations,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use witnet_data_structures::chain::{
        Hash, Hashable, KeyedSignature, PublicKey, ValueTransferOutput,
    };
    use witnet_data_structures::transaction::{
        CommitTransaction, DRTransaction, RevealTransaction,
    };

    #[test]
    fn test_rep_info_update() {
        let mut rep_info = ReputationInfo::default();
        let mut dr_pool = DataRequestPool::default();

        let pk1 = PublicKey {
            compressed: 0,
            bytes: [1; 32],
        };
        let pk2 = PublicKey {
            compressed: 0,
            bytes: [2; 32],
        };
        let pk3 = PublicKey {
            compressed: 0,
            bytes: [3; 32],
        };

        let mut dr_tx = DRTransaction::default();
        dr_tx.signatures.push(KeyedSignature {
            public_key: pk1.clone(),
            ..KeyedSignature::default()
        });
        let dr_pointer = dr_tx.hash();

        let mut co_tx = CommitTransaction::default();
        co_tx.body.dr_pointer = dr_pointer;
        co_tx.signatures.push(KeyedSignature {
            public_key: pk1.clone(),
            ..KeyedSignature::default()
        });
        let mut co_tx2 = CommitTransaction::default();
        co_tx2.body.dr_pointer = dr_pointer;
        co_tx2.signatures.push(KeyedSignature {
            public_key: pk2.clone(),
            ..KeyedSignature::default()
        });
        let mut co_tx3 = CommitTransaction::default();
        co_tx3.body.dr_pointer = dr_pointer;
        co_tx3.signatures.push(KeyedSignature {
            public_key: pk3.clone(),
            ..KeyedSignature::default()
        });
        let mut re_tx1 = RevealTransaction::default();
        re_tx1.body.dr_pointer = dr_pointer;
        re_tx1.signatures.push(KeyedSignature {
            public_key: pk1.clone(),
            ..KeyedSignature::default()
        });
        let mut re_tx2 = RevealTransaction::default();
        re_tx2.body.dr_pointer = dr_pointer;
        re_tx2.signatures.push(KeyedSignature {
            public_key: pk2.clone(),
            ..KeyedSignature::default()
        });

        let mut ta_tx = TallyTransaction::default();
        ta_tx.dr_pointer = dr_pointer;
        ta_tx.outputs = vec![ValueTransferOutput {
            pkh: pk1.pkh(),
            ..Default::default()
        }];
        ta_tx.out_of_consensus = vec![pk3.pkh()];
        ta_tx.error_committers = vec![pk2.pkh()];

        dr_pool
            .add_data_request(1, dr_tx, &Hash::default())
            .unwrap();
        dr_pool.process_commit(&co_tx, &Hash::default()).unwrap();
        dr_pool.process_commit(&co_tx2, &Hash::default()).unwrap();
        dr_pool.process_commit(&co_tx3, &Hash::default()).unwrap();
        dr_pool.update_data_request_stages();
        dr_pool.process_reveal(&re_tx1, &Hash::default()).unwrap();
        dr_pool.process_reveal(&re_tx2, &Hash::default()).unwrap();

        rep_info.update(
            &ta_tx,
            &dr_pool,
            PublicKeyHash::default(),
            &mut NodeStats::default(),
        );

        assert_eq!(
            rep_info.result_count[&pk1.pkh()],
            RequestResult {
                truths: 1,
                lies: 0,
                errors: 0,
            }
        );
        assert_eq!(
            rep_info.result_count[&pk2.pkh()],
            RequestResult {
                truths: 0,
                lies: 0,
                errors: 1,
            }
        );
        assert_eq!(
            rep_info.result_count[&pk3.pkh()],
            RequestResult {
                truths: 0,
                lies: 1,
                errors: 0,
            }
        );
    }
}
