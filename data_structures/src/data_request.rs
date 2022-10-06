use std::{
    collections::{BTreeMap, HashMap, HashSet},
    convert::TryFrom,
};

use serde::{Deserialize, Serialize};

use crate::{
    chain::{
        tapi::ActiveWips, DataRequestInfo, DataRequestOutput, DataRequestStage, DataRequestState,
        Epoch, Hash, Hashable, PublicKeyHash, ValueTransferOutput,
    },
    error::{DataRequestError, TransactionError},
    radon_report::{RadonReport, Stage, TypeLike},
    transaction::{CommitTransaction, DRTransaction, RevealTransaction, TallyTransaction},
};
use witnet_crypto::hash::calculate_sha256;

/// Pool of active data requests
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct DataRequestPool {
    /// Current active data request, in which this node has announced commitments.
    /// Key: DRTransaction hash, Value: Reveal Transaction
    pub waiting_for_reveal: HashMap<Hash, RevealTransaction>,
    /// List of active data request output pointers ordered by epoch (for mining purposes)
    pub data_requests_by_epoch: BTreeMap<Epoch, HashSet<Hash>>,
    /// List of active data requests indexed by output pointer
    pub data_request_pool: HashMap<Hash, DataRequestState>,
    /// List of data requests that should be persisted into storage
    pub to_be_stored: Vec<DataRequestInfo>,
    /// Extra rounds for commitments and reveals
    pub extra_rounds: u16,
}

impl DataRequestPool {
    /// Create a new 'DataRequestPool' initialized with a number of extra rounds
    pub fn new(extra_rounds: u16) -> Self {
        Self {
            extra_rounds,
            ..Default::default()
        }
    }

    /// Get all available data requests output pointers for an epoch
    pub fn get_dr_output_pointers_by_epoch(&self, epoch: Epoch) -> Vec<Hash> {
        let range = 0..=epoch;
        self.data_requests_by_epoch
            .range(range)
            .flat_map(|(_epoch, hashset)| hashset.iter().cloned())
            .collect()
    }

    /// Get a `DataRequestOuput` for a DRTransaction `Hash`
    pub fn get_dr_output(&self, dr_pointer: &Hash) -> Option<DataRequestOutput> {
        self.data_request_pool
            .get(dr_pointer)
            .map(|dr_state| dr_state.data_request.clone())
    }

    /// Get all reveals related to a `DataRequestOuput`
    pub fn get_reveals(
        &self,
        dr_pointer: &Hash,
        active_wips: &ActiveWips,
    ) -> Option<Vec<&RevealTransaction>> {
        self.data_request_pool.get(dr_pointer).map(|dr_state| {
            let mut reveals: Vec<&RevealTransaction> = dr_state.info.reveals.values().collect();
            if active_wips.wip0019() {
                // As specified in (7) in WIP-0019
                reveals.sort_unstable_by_key(|reveal| {
                    concatenate_and_hash(&reveal.body.pkh.hash, dr_pointer.as_ref())
                });
            } else {
                reveals.sort_unstable_by_key(|reveal| reveal.body.pkh);
            }

            reveals
        })
    }

    /// Insert a reveal transaction into the pool
    pub fn insert_reveal(&mut self, dr_pointer: Hash, reveal: RevealTransaction) {
        self.waiting_for_reveal.insert(dr_pointer, reveal);
    }

    /// Get all the reveals
    pub fn get_all_reveals(
        &self,
        active_wips: &ActiveWips,
    ) -> HashMap<Hash, Vec<RevealTransaction>> {
        self.data_request_pool
            .iter()
            .filter_map(|(dr_pointer, dr_state)| {
                if let DataRequestStage::TALLY = dr_state.stage {
                    let mut reveals: Vec<RevealTransaction> =
                        dr_state.info.reveals.values().cloned().collect();

                    if active_wips.wip0019() {
                        // As specified in (7) in WIP-0019
                        reveals.sort_unstable_by_key(|reveal| {
                            concatenate_and_hash(&reveal.body.pkh.hash, dr_pointer.as_ref())
                        });
                    } else {
                        reveals.sort_unstable_by_key(|reveal| reveal.body.pkh);
                    }

                    Some((*dr_pointer, reveals))
                } else {
                    None
                }
            })
            .collect()
    }

    /// Get data request pointers ready for tally
    pub fn get_tally_ready_drs(&self) -> HashSet<Hash> {
        self.data_request_pool
            .iter()
            .filter_map(|(dr_pointer, dr_state)| {
                if let DataRequestStage::TALLY = dr_state.stage {
                    Some(*dr_pointer)
                } else {
                    None
                }
            })
            .collect()
    }

    /// Add a data request to the data request pool
    pub fn add_data_request(
        &mut self,
        epoch: Epoch,
        data_request: DRTransaction,
        block_hash: &Hash,
    ) -> Result<(), failure::Error> {
        let dr_hash = data_request.hash();
        if data_request.signatures.is_empty() {
            return Err(TransactionError::SignatureNotFound.into());
        }

        let pkh = data_request.signatures[0].public_key.pkh();
        let dr_state = DataRequestState::new(data_request.body.dr_output, pkh, epoch, block_hash);

        self.data_requests_by_epoch
            .entry(epoch)
            .or_insert_with(HashSet::new)
            .insert(dr_hash);
        self.data_request_pool.insert(dr_hash, dr_state);

        Ok(())
    }

    /// Add a commit to the corresponding data request
    fn add_commit(
        &mut self,
        pkh: PublicKeyHash,
        commit: CommitTransaction,
        block_hash: &Hash,
    ) -> Result<(), failure::Error> {
        let tx_hash = commit.hash();
        // For a commit output, we need to get the corresponding data request input
        let dr_pointer = commit.body.dr_pointer;

        // The data request must be from a previous block, and must not be timelocked.
        // This is not checked here, as it should have made the block invalid.
        if let Some(dr) = self.data_request_pool.get_mut(&dr_pointer) {
            dr.add_commit(pkh, commit)
        } else {
            Err(DataRequestError::AddCommitFail {
                block_hash: *block_hash,
                tx_hash,
                dr_pointer,
            }
            .into())
        }
    }

    /// Add a reveal transaction
    fn add_reveal(
        &mut self,
        pkh: PublicKeyHash,
        reveal: RevealTransaction,
        block_hash: &Hash,
    ) -> Result<(), failure::Error> {
        let tx_hash = reveal.hash();
        // For a commit output, we need to get the corresponding data request input
        let dr_pointer = reveal.body.dr_pointer;
        // The data request must be from a previous block, and must not be timelocked.
        // This is not checked here, as it should have made the block invalid.
        if let Some(dr) = self.data_request_pool.get_mut(&dr_pointer) {
            dr.add_reveal(pkh, reveal)
        } else {
            Err(DataRequestError::AddRevealFail {
                block_hash: *block_hash,
                tx_hash,
                dr_pointer,
            }
            .into())
        }
    }

    /// Add a tally transaction
    #[allow(clippy::needless_pass_by_value)]
    fn add_tally(
        &mut self,
        tally: TallyTransaction,
        block_hash: &Hash,
    ) -> Result<(), failure::Error> {
        let dr_info = Self::resolve_data_request(&mut self.data_request_pool, tally, block_hash)?;

        // Since this method does not have access to the storage, we save the
        // "to be stored" inside a vector and provide another method to store them
        self.to_be_stored.push(dr_info);

        Ok(())
    }

    /// Removes a resolved data request from the data request pool, returning the `DataRequestOutput`
    /// and a `DataRequestInfoStorage` which should be persisted into storage.
    fn resolve_data_request(
        data_request_pool: &mut HashMap<Hash, DataRequestState>,
        tally_tx: TallyTransaction,
        block_hash: &Hash,
    ) -> Result<DataRequestInfo, failure::Error> {
        let dr_pointer = tally_tx.dr_pointer;

        let dr_state: Result<DataRequestState, failure::Error> =
            data_request_pool.remove(&dr_pointer).ok_or_else(|| {
                DataRequestError::AddTallyFail {
                    block_hash: *block_hash,
                    tx_hash: tally_tx.hash(),
                    dr_pointer,
                }
                .into()
            });
        let dr_state = dr_state?;

        dr_state.add_tally(tally_tx, block_hash)
    }

    /// Return the list of data requests in which this node has participated and are ready
    /// for reveal (the node should send a reveal transaction).
    /// This function must be called after `add_data_requests_from_block`, in order to update
    /// the stage of all the data requests.
    pub fn update_data_request_stages(&mut self) -> Vec<RevealTransaction> {
        let waiting_for_reveal = &mut self.waiting_for_reveal;
        let data_requests_by_epoch = &mut self.data_requests_by_epoch;
        let extra_rounds = self.extra_rounds;
        // Update the stage of the active data requests
        self.data_request_pool
            .iter_mut()
            .filter_map(|(dr_pointer, dr_state)| {
                // We can notify the user that a data request from "my_claims" is available
                // for reveal.
                dr_state.update_stage(extra_rounds);
                match dr_state.stage {
                    DataRequestStage::REVEAL => {
                        // When a data request changes from commit stage to reveal stage, it should
                        // be removed from the "data_requests_by_epoch" map, which stores the data
                        // requests potentially available for commitment
                        if dr_state.info.current_reveal_round == 1 {
                            if let Some(hs) = data_requests_by_epoch.get_mut(&dr_state.epoch) {
                                let present = hs.remove(dr_pointer);
                                if hs.is_empty() {
                                    data_requests_by_epoch.remove(&dr_state.epoch);
                                }
                                if !present {
                                    log::error!(
                                        "Data request {:?} was not present in the \
                                         data_requests_by_epoch map (epoch #{})",
                                        dr_pointer,
                                        dr_state.epoch
                                    );
                                }
                            }
                        }

                        if let Some(transaction) = waiting_for_reveal.get(dr_pointer) {
                            // We submitted a commit for this data request!
                            // But has it been included into the block?
                            let pkh = PublicKeyHash::from_public_key(
                                &transaction.signatures[0].public_key,
                            );
                            if dr_state.info.commits.contains_key(&pkh)
                                && !dr_state.info.reveals.contains_key(&pkh)
                            {
                                // We found our commit, return the reveal transaction to be sent
                                // until it would be included in a block.
                                return Some(transaction.clone());
                            } else if !dr_state.info.commits.contains_key(&pkh)
                                && dr_state.info.current_reveal_round == 1
                            {
                                log::info!(
                                    "The sent commit transaction has not been \
                                     selected to be part of the data request {:?}",
                                    dr_pointer
                                );
                            }
                        }
                    }

                    DataRequestStage::TALLY => {
                        // When a data request changes from commit stage to tally stage
                        // (not enough commits to go into reveal stage)
                        // it should be removed from the "data_requests_by_epoch" map
                        if let Some(hs) = data_requests_by_epoch.get_mut(&dr_state.epoch) {
                            hs.remove(dr_pointer);
                            if hs.is_empty() {
                                data_requests_by_epoch.remove(&dr_state.epoch);
                            }
                        }
                        // Remove pending reveals in Tally stage
                        waiting_for_reveal.remove(dr_pointer);
                    }
                    _ => {}
                }

                None
            })
            .collect()
    }

    /// New commitments are added to their respective data requests, updating the stage to reveal
    pub fn process_commit(
        &mut self,
        commit_transaction: &CommitTransaction,
        block_hash: &Hash,
    ) -> Result<(), failure::Error> {
        let pkh = PublicKeyHash::from_public_key(&commit_transaction.signatures[0].public_key);
        self.add_commit(pkh, commit_transaction.clone(), block_hash)
    }

    /// New reveals are added to their respective data requests, updating the stage to tally
    pub fn process_reveal(
        &mut self,
        reveal_transaction: &RevealTransaction,
        block_hash: &Hash,
    ) -> Result<(), failure::Error> {
        let pkh = PublicKeyHash::from_public_key(&reveal_transaction.signatures[0].public_key);
        self.add_reveal(pkh, reveal_transaction.clone(), block_hash)
    }

    /// New data requests are inserted and wait for commitments
    /// The epoch is needed as the key to the available data requests map
    pub fn process_data_request(
        &mut self,
        dr_transaction: &DRTransaction,
        epoch: Epoch,
        block_hash: &Hash,
    ) -> Result<(), failure::Error> {
        // A data request output should have a valid value transfer input
        // Which we assume valid as it should have been already verified

        self.add_data_request(epoch, dr_transaction.clone(), block_hash)
    }

    /// New tallies are added to their respective data requests and finish them
    pub fn process_tally(
        &mut self,
        tally_transaction: &TallyTransaction,
        block_hash: &Hash,
    ) -> Result<(), failure::Error> {
        self.add_tally(tally_transaction.clone(), block_hash)
    }

    /// Get the detailed state of a data request.
    pub fn data_request_state(&self, dr_pointer: &Hash) -> Option<&DataRequestState> {
        self.data_request_pool.get(dr_pointer)
    }

    /// Get the data request info of the finished data requests, to be persisted to the storage
    pub fn finished_data_requests(&mut self) -> Vec<DataRequestInfo> {
        std::mem::take(&mut self.to_be_stored)
    }

    /// Return the sum of all the wits that is currently being used to resolve data requests
    pub fn locked_wits_by_requests(&self, collateral_minimum: u64) -> u64 {
        let mut total = 0;
        for dr_state in self.data_request_pool.values() {
            let dr_collateral = dr_state.data_request.collateral;
            let dr_collateral = if dr_collateral == 0 {
                collateral_minimum
            } else {
                dr_collateral
            };

            let locked_wits = match dr_state.stage {
                DataRequestStage::COMMIT => {
                    // In commit stage, there are not collateralized wits yet,
                    // but the requester has locked wits to pay fees and rewards
                    dr_state.data_request.checked_total_value().unwrap()
                }
                DataRequestStage::REVEAL | DataRequestStage::TALLY => {
                    // In reveal and tally stage, there are some wits collateralized by the witnesses,
                    // and there are some fees that were already paid
                    let dr_total_value = dr_state.data_request.checked_total_value().unwrap();
                    let n_commits = u64::try_from(dr_state.info.commits.len()).unwrap();
                    let n_reveals = u64::try_from(dr_state.info.reveals.len()).unwrap();

                    let collateralized_wits = n_commits.saturating_mul(dr_collateral);
                    let commit_fees =
                        n_commits.saturating_mul(dr_state.data_request.commit_and_reveal_fee);
                    let reveal_fees =
                        n_reveals.saturating_mul(dr_state.data_request.commit_and_reveal_fee);

                    dr_total_value
                        .saturating_add(collateralized_wits)
                        .saturating_sub(commit_fees)
                        .saturating_sub(reveal_fees)
                }
            };

            total += locked_wits;
        }

        total
    }
}

/// Concatenate 2 bytes sequences and hash
fn concatenate_and_hash(a: &[u8], b: &[u8]) -> [u8; 32] {
    let mut bytes_to_hash = vec![];
    bytes_to_hash.extend(a);
    bytes_to_hash.extend(b);

    calculate_sha256(bytes_to_hash.as_ref()).0
}

/// Return the change that should be returned to the creator of the data request if
/// some witness fails to commit, fails to reveal, or reports a value out of consensus.
/// If the change is 0, the change `ValueTransferOutput` should not be created
pub fn calculate_tally_change(
    commits_count: usize,
    reveals_count: usize,
    honests_count: usize,
    dr_output: &DataRequestOutput,
) -> u64 {
    let commits_count = commits_count as u64;
    let reveals_count = reveals_count as u64;
    let honests_count = honests_count as u64;
    let witnesses = u64::from(dr_output.witnesses);

    dr_output.witness_reward * (witnesses - honests_count)
        + dr_output.commit_and_reveal_fee * (witnesses - reveals_count)
        + dr_output.commit_and_reveal_fee * (witnesses - commits_count)
}

pub fn calculate_witness_reward_before_second_hard_fork(
    commits_count: usize,
    reveals_count: usize,
    // Number of values that are out of consensus plus non-revealers
    liars_count: usize,
    // To calculate the reward, we consider errors_count as the number of errors that are
    // out of consensus, it means, that they do not deserve any reward
    errors_count: usize,
    reward: u64,
    collateral: u64,
) -> (u64, u64) {
    if commits_count == 0 {
        (0, 0)
    } else if reveals_count == 0 {
        (collateral, 0)
    } else {
        let honests_count = (commits_count - liars_count - errors_count) as u64;
        let liars_count = liars_count as u64;
        let slashed_collateral_reward = collateral * liars_count / honests_count;
        let slashed_collateral_remainder = (collateral * liars_count) % honests_count;

        (
            reward + collateral + slashed_collateral_reward,
            slashed_collateral_remainder,
        )
    }
}

pub fn calculate_witness_reward(
    commits_count: usize,
    // Number of values that are out of consensus plus non-revealers
    liars_count: usize,
    // To calculate the reward, we consider errors_count as the number of errors that are
    // out of consensus, it means, that they do not deserve any reward
    errors_count: usize,
    reward: u64,
    collateral: u64,
    wip0023_active: bool,
) -> (u64, u64) {
    let honests_count = (commits_count - liars_count - errors_count) as u64;

    if commits_count == 0 {
        (0, 0)
    } else if honests_count == 0 {
        (collateral, 0)
    } else {
        let liars_count = liars_count as u64;
        let slashed_collateral_reward = collateral * liars_count / honests_count;
        let slashed_collateral_remainder = (collateral * liars_count) % honests_count;

        if wip0023_active {
            (reward + collateral, 0)
        } else {
            (
                reward + collateral + slashed_collateral_reward,
                slashed_collateral_remainder,
            )
        }
    }
}

/// Function to calculate the data request reward to collateral ratio
///
/// The ratio is rounded up to the next integer. This is because the validation checks for ratios
/// greater than a maximum, so a ratio of 125.0001 should be greater than 125. Therefore, 125.0001
/// will be rounded up to 126.
pub fn calculate_reward_collateral_ratio(
    collateral: u64,
    collateral_minimum: u64,
    witness_reward: u64,
) -> u64 {
    let dr_collateral = if collateral == 0 {
        collateral_minimum
    } else {
        collateral
    };

    saturating_div_ceil(dr_collateral, witness_reward)
}

/// Saturating version of `u64::div_ceil`.
///
/// Calculates the quotient of `lhs` and `rhs`, rounding the result towards positive infinity.
///
/// Returns `u64::MAX` if `rhs` is zero.
fn saturating_div_ceil(lhs: u64, rhs: u64) -> u64 {
    if rhs == 0 {
        return u64::MAX;
    }

    let d = lhs / rhs;
    let r = lhs % rhs;

    if r > 0 {
        d + 1
    } else {
        d
    }
}

/// Count how many commitments will be considered "errors" and how many will
/// be considered "lies". This tells apart the case in which the committed value was
/// an error from any other type of out-of-consensus situation, like non-reveals.
fn calculate_errors_and_liars_count(errors: &[bool], liars: &[bool]) -> (usize, usize) {
    liars
        .iter()
        .zip(errors.iter())
        .fold((0, 0), |(l_count, e_count), x| match x {
            // If it is out of consensus and it is an error we consider as a error
            (true, true) => (l_count, e_count + 1),
            // If it is out of consensus and it is not an error we consider as a lie
            (true, false) => (l_count + 1, e_count),
            // Rest of cases is an honest node
            _ => (l_count, e_count),
        })
}

/// Create tally transaction to reward honest witnesses and penalize liars.
///
/// # Panics
///
/// This function panics if the `RadonReport` is not in tally stage.
///
/// And also if `tally_metadata.liars` and `tally_metadata.errors` do not have the same length as
/// `revealers`
#[allow(clippy::too_many_arguments)]
pub fn create_tally<RT, S: ::std::hash::BuildHasher>(
    dr_pointer: Hash,
    dr_output: &DataRequestOutput,
    pkh: PublicKeyHash,
    report: &RadonReport<RT>,
    revealers: Vec<PublicKeyHash>,
    committers: HashSet<PublicKeyHash, S>,
    collateral_minimum: u64,
    tally_bytes_on_encode_error: Vec<u8>,
    active_wips: &ActiveWips,
) -> TallyTransaction
where
    RT: TypeLike,
{
    if let Stage::Tally(tally_metadata) = &report.context.stage {
        let commits_count = committers.len();
        let reveals_count = revealers.len();
        let mut out_of_consensus = committers;
        let mut error_committers = vec![];

        let liars = &tally_metadata.liars;
        let errors = &tally_metadata.errors;

        assert_eq!(reveals_count, liars.len(), "Length of liars vector collected from tally ({}) does not match actual count of reveals ({})", reveals_count, liars.len());
        assert_eq!(reveals_count, errors.len(), "Length of errors vector collected from tally ({}) does not match actual count of reveals ({})", reveals_count, errors.len());

        let (liars_count, errors_count) = calculate_errors_and_liars_count(errors, liars);

        let collateral = if dr_output.collateral == 0 {
            collateral_minimum
        } else {
            dr_output.collateral
        };

        // Collateral division rest goes for the miner
        let non_reveals_count = commits_count - reveals_count;
        let is_after_second_hard_fork = active_wips.wips_0009_0011_0012();
        let (reward, _rest) = if is_after_second_hard_fork {
            calculate_witness_reward(
                commits_count,
                liars_count + non_reveals_count,
                errors_count,
                dr_output.witness_reward,
                collateral,
                active_wips.wip0023(),
            )
        } else {
            calculate_witness_reward_before_second_hard_fork(
                commits_count,
                reveals_count,
                liars_count + non_reveals_count,
                errors_count,
                dr_output.witness_reward,
                collateral,
            )
        };
        // Check if we need to reward revealers
        let any_honest_revealers = if is_after_second_hard_fork {
            let honests_count = commits_count - liars_count - errors_count - non_reveals_count;

            honests_count > 0
        } else {
            reveals_count > 0
        };
        let mut outputs: Vec<ValueTransferOutput> = if any_honest_revealers {
            revealers
                .iter()
                .zip(liars.iter().zip(errors.iter()))
                .filter_map(|(&revealer, (liar, error))| match (liar, error) {
                    // If an out-of-consensus commitment was an error report, collateral is refunded
                    (true, true) => {
                        let vt_output = ValueTransferOutput {
                            pkh: revealer,
                            value: collateral,
                            time_lock: 0,
                        };
                        error_committers.push(revealer);
                        Some(vt_output)
                    }
                    // Case out-of-consensus value commitment
                    (true, false) => None,
                    // If the result of the tally is an error report,
                    // commitments containing error reports are rewarded
                    (false, true) => {
                        let vt_output = ValueTransferOutput {
                            pkh: revealer,
                            value: reward,
                            time_lock: 0,
                        };
                        out_of_consensus.remove(&revealer);
                        error_committers.push(revealer);
                        Some(vt_output)
                    }
                    // Case in consensus value
                    (false, false) => {
                        let vt_output = ValueTransferOutput {
                            pkh: revealer,
                            value: reward,
                            time_lock: 0,
                        };
                        out_of_consensus.remove(&revealer);
                        Some(vt_output)
                    }
                })
                .collect()
        } else {
            // In case of no honests, collateral returns to their owners

            if is_after_second_hard_fork {
                // After second hard fork, mark all the revealers as errors to avoid penalizing them:
                // If honests_count == 0, all revealers are out of consensus errors
                for revealer in revealers {
                    error_committers.push(revealer);
                }
            }

            out_of_consensus
                .iter()
                .map(|&committer| ValueTransferOutput {
                    pkh: committer,
                    value: collateral,
                    time_lock: 0,
                })
                .collect()
        };

        let honests_count = reveals_count - liars_count - errors_count;
        let tally_change =
            calculate_tally_change(commits_count, reveals_count, honests_count, dr_output);
        if tally_change > 0 {
            let vt_output_change = ValueTransferOutput {
                pkh,
                value: tally_change,
                time_lock: 0,
            };
            outputs.push(vt_output_change);
        }

        let tally_bytes = Vec::try_from(report).unwrap_or_else(|e| {
            log::warn!("Failed to serialize tally result. Error was: {:?}", e);

            tally_bytes_on_encode_error
        });
        let out_of_consensus = out_of_consensus.into_iter().collect();

        TallyTransaction::new(
            dr_pointer,
            tally_bytes,
            outputs,
            out_of_consensus,
            error_committers,
        )
    } else {
        panic!("{}", TransactionError::NoTallyStage)
    }
}

#[cfg(test)]
mod tests {
    use crate::{chain::*, transaction::*, vrf::*};

    use super::*;

    fn add_data_requests() -> (u32, Hash, DataRequestPool, Hash) {
        let fake_block_hash = Hash::SHA256([1; 32]);
        let epoch = 0;
        let empty_info = DataRequestInfo {
            block_hash_dr_tx: Some(fake_block_hash),
            ..DataRequestInfo::default()
        };
        let dr_transaction = DRTransaction::new(
            DRTransactionBody::new(vec![Input::default()], vec![], DataRequestOutput::default()),
            vec![KeyedSignature::default()],
        );
        let dr_pointer = dr_transaction.hash();

        let mut p = DataRequestPool::default();
        p.process_data_request(&dr_transaction, epoch, &fake_block_hash)
            .unwrap();

        assert!(p.waiting_for_reveal.is_empty());
        assert!(p.data_requests_by_epoch[&epoch].contains(&dr_pointer));
        assert_eq!(p.data_request_pool[&dr_pointer].info, empty_info);
        assert_eq!(
            p.data_request_pool[&dr_pointer].stage,
            DataRequestStage::COMMIT
        );
        assert!(p.to_be_stored.is_empty());

        assert!(p.update_data_request_stages().is_empty());

        (epoch, fake_block_hash, p, dr_transaction.hash())
    }

    fn add_data_requests_with_3_commit_stages() -> (u32, Hash, DataRequestPool, Hash) {
        let fake_block_hash = Hash::SHA256([1; 32]);
        let epoch = 0;
        let empty_info = DataRequestInfo {
            block_hash_dr_tx: Some(fake_block_hash),
            ..DataRequestInfo::default()
        };
        let dr_output = DataRequestOutput {
            witnesses: 2,
            ..DataRequestOutput::default()
        };
        let dr_transaction = DRTransaction::new(
            DRTransactionBody::new(vec![Input::default()], vec![], dr_output),
            vec![KeyedSignature::default()],
        );
        let dr_pointer = dr_transaction.hash();

        let mut p = DataRequestPool::new(2);
        p.process_data_request(&dr_transaction, epoch, &fake_block_hash)
            .unwrap();

        assert!(p.waiting_for_reveal.is_empty());
        assert!(p.data_requests_by_epoch[&epoch].contains(&dr_pointer));
        assert_eq!(p.data_request_pool[&dr_pointer].info, empty_info);
        assert_eq!(
            p.data_request_pool[&dr_pointer].stage,
            DataRequestStage::COMMIT
        );
        assert!(p.to_be_stored.is_empty());

        assert!(p.update_data_request_stages().is_empty());

        (epoch, fake_block_hash, p, dr_transaction.hash())
    }

    fn add_data_requests_with_3_reveal_stages() -> (u32, Hash, DataRequestPool, Hash) {
        let fake_block_hash = Hash::SHA256([1; 32]);
        let epoch = 0;
        let empty_info = DataRequestInfo {
            block_hash_dr_tx: Some(fake_block_hash),
            ..DataRequestInfo::default()
        };
        let dr_output = DataRequestOutput {
            witnesses: 2,
            ..DataRequestOutput::default()
        };
        let dr_transaction = DRTransaction::new(
            DRTransactionBody::new(vec![Input::default()], vec![], dr_output),
            vec![KeyedSignature::default()],
        );
        let dr_pointer = dr_transaction.hash();

        let mut p = DataRequestPool::new(2);
        p.process_data_request(&dr_transaction, epoch, &fake_block_hash)
            .unwrap();

        assert!(p.waiting_for_reveal.is_empty());
        assert!(p.data_requests_by_epoch[&epoch].contains(&dr_pointer));
        assert_eq!(p.data_request_pool[&dr_pointer].info, empty_info);
        assert_eq!(
            p.data_request_pool[&dr_pointer].stage,
            DataRequestStage::COMMIT
        );
        assert!(p.to_be_stored.is_empty());

        assert!(p.update_data_request_stages().is_empty());

        (epoch, fake_block_hash, p, dr_transaction.hash())
    }

    fn from_commit_to_reveal(
        epoch: u32,
        fake_block_hash: Hash,
        mut p: DataRequestPool,
        dr_pointer: Hash,
    ) -> (Hash, DataRequestPool, Hash) {
        let commit_transaction = CommitTransaction::new(
            CommitTransactionBody::without_collateral(
                dr_pointer,
                Hash::default(),
                DataRequestEligibilityClaim::default(),
            ),
            vec![KeyedSignature::default()],
        );

        p.process_commit(&commit_transaction, &fake_block_hash)
            .unwrap();

        // And we can also get all the commit pointers from the data request
        assert_eq!(
            p.data_request_pool[&dr_pointer]
                .info
                .commits
                .values()
                .collect::<Vec<_>>(),
            vec![&commit_transaction],
        );

        // Still in commit stage until we update
        assert_eq!(
            p.data_request_pool[&dr_pointer].stage,
            DataRequestStage::COMMIT
        );

        assert!(p.data_requests_by_epoch[&epoch].contains(&dr_pointer));

        // Update stages
        assert!(p.update_data_request_stages().is_empty());

        // Now in reveal stage
        assert_eq!(
            p.data_request_pool[&dr_pointer].stage,
            DataRequestStage::REVEAL
        );

        // The data request was removed from the data_requests_by_epoch map
        assert!(!p
            .data_requests_by_epoch
            .get(&epoch)
            .map(|x| x.contains(&dr_pointer))
            .unwrap_or(false));

        (fake_block_hash, p, dr_pointer)
    }

    fn from_reveal_to_tally(
        fake_block_hash: Hash,
        mut p: DataRequestPool,
        dr_pointer: Hash,
    ) -> (Hash, DataRequestPool, Hash) {
        let reveal_transaction = RevealTransaction::new(
            RevealTransactionBody::new(dr_pointer, vec![], PublicKeyHash::default()),
            vec![KeyedSignature::default()],
        );

        p.process_reveal(&reveal_transaction, &fake_block_hash)
            .unwrap();

        assert_eq!(
            p.data_request_pool[&dr_pointer]
                .info
                .reveals
                .values()
                .collect::<Vec<_>>(),
            vec![&reveal_transaction],
        );

        // Still in reveal stage until we update
        assert_eq!(
            p.data_request_pool[&dr_pointer].stage,
            DataRequestStage::REVEAL
        );

        // Update stages
        assert!(p.update_data_request_stages().is_empty());

        // Now in tally stage
        assert_eq!(
            p.data_request_pool[&dr_pointer].stage,
            DataRequestStage::TALLY
        );

        (fake_block_hash, p, dr_pointer)
    }

    fn from_tally_to_storage(fake_block_hash: Hash, mut p: DataRequestPool, dr_pointer: Hash) {
        let tally_transaction = TallyTransaction::new(dr_pointer, vec![], vec![], vec![], vec![]);

        // There is nothing to be stored yet
        assert_eq!(p.to_be_stored.len(), 0);

        // Process tally: this will remove the data request from the pool
        p.process_tally(&tally_transaction, &fake_block_hash)
            .unwrap();

        // And the data request has been removed from the pool
        assert_eq!(p.data_request_state(&dr_pointer), None);

        // Update stages
        assert!(p.update_data_request_stages().is_empty());

        assert_eq!(p.to_be_stored.len(), 1);
        assert_eq!(
            p.to_be_stored[0].tally.as_ref().unwrap().dr_pointer,
            dr_pointer
        );
    }

    #[test]
    fn test_add_data_requests() {
        add_data_requests();
    }

    #[test]
    fn test_add_data_requests_with_3_reveal_stages() {
        add_data_requests_with_3_reveal_stages();
    }

    #[test]
    fn test_from_commit_to_reveal() {
        let (epoch, fake_block_hash, p, dr_pointer) = add_data_requests();

        from_commit_to_reveal(epoch, fake_block_hash, p, dr_pointer);
    }

    #[test]
    fn test_from_reveal_to_tally() {
        let (epoch, fake_block_hash, p, dr_pointer) = add_data_requests();
        let (fake_block_hash, p, dr_pointer) =
            from_commit_to_reveal(epoch, fake_block_hash, p, dr_pointer);

        from_reveal_to_tally(fake_block_hash, p, dr_pointer);
    }

    #[test]
    fn test_from_reveal_to_tally_3_stages_uncompleted() {
        let (_epoch, fake_block_hash, mut p, dr_pointer) = add_data_requests_with_3_reveal_stages();

        let commit_transaction = CommitTransaction::new(
            CommitTransactionBody::without_collateral(
                dr_pointer,
                Hash::default(),
                DataRequestEligibilityClaim::default(),
            ),
            vec![KeyedSignature::default()],
        );

        let pk2 = PublicKey {
            compressed: 0,
            bytes: [1; 32],
        };
        let commit_transaction2 = CommitTransaction::new(
            CommitTransactionBody::without_collateral(
                dr_pointer,
                Hash::default(),
                DataRequestEligibilityClaim::default(),
            ),
            vec![KeyedSignature {
                signature: Signature::default(),
                public_key: pk2,
            }],
        );

        p.process_commit(&commit_transaction, &fake_block_hash)
            .unwrap();
        p.process_commit(&commit_transaction2, &fake_block_hash)
            .unwrap();

        // Update stages
        assert!(p.update_data_request_stages().is_empty());

        // Now in reveal stage 1
        assert_eq!(
            p.data_request_pool[&dr_pointer].stage,
            DataRequestStage::REVEAL
        );
        assert_eq!(
            p.data_request_pool[&dr_pointer].info.current_reveal_round,
            1
        );

        let reveal_transaction = RevealTransaction::new(
            RevealTransactionBody::new(dr_pointer, vec![], PublicKeyHash::default()),
            vec![KeyedSignature::default()],
        );

        p.process_reveal(&reveal_transaction, &fake_block_hash)
            .unwrap();

        // Update stages
        assert!(p.update_data_request_stages().is_empty());
        // Now in reveal stage 2
        assert_eq!(
            p.data_request_pool[&dr_pointer].stage,
            DataRequestStage::REVEAL
        );
        assert_eq!(
            p.data_request_pool[&dr_pointer].info.current_reveal_round,
            2
        );

        // Update stages
        assert!(p.update_data_request_stages().is_empty());
        // Now in reveal stage 3
        assert_eq!(
            p.data_request_pool[&dr_pointer].stage,
            DataRequestStage::REVEAL
        );
        assert_eq!(
            p.data_request_pool[&dr_pointer].info.current_reveal_round,
            3
        );

        // Update stages
        assert!(p.update_data_request_stages().is_empty());
        // Now in tally stage
        assert_eq!(
            p.data_request_pool[&dr_pointer].stage,
            DataRequestStage::TALLY
        );
        let mut expected_hs = HashSet::new();
        expected_hs.insert(dr_pointer);
        assert_eq!(p.get_tally_ready_drs(), expected_hs)
    }

    #[test]
    fn test_from_reveal_to_tally_3_stages_completed() {
        let (_epoch, fake_block_hash, mut p, dr_pointer) = add_data_requests_with_3_reveal_stages();

        let commit_transaction = CommitTransaction::new(
            CommitTransactionBody::without_collateral(
                dr_pointer,
                Hash::default(),
                DataRequestEligibilityClaim::default(),
            ),
            vec![KeyedSignature::default()],
        );

        let pk2 = PublicKey {
            compressed: 0,
            bytes: [1; 32],
        };
        let commit_transaction2 = CommitTransaction::new(
            CommitTransactionBody::without_collateral(
                dr_pointer,
                Hash::default(),
                DataRequestEligibilityClaim::default(),
            ),
            vec![KeyedSignature {
                signature: Signature::default(),
                public_key: pk2.clone(),
            }],
        );

        p.process_commit(&commit_transaction, &fake_block_hash)
            .unwrap();
        p.process_commit(&commit_transaction2, &fake_block_hash)
            .unwrap();

        // Update stages
        assert!(p.update_data_request_stages().is_empty());

        // Now in reveal stage 1
        assert_eq!(
            p.data_request_pool[&dr_pointer].stage,
            DataRequestStage::REVEAL
        );
        assert_eq!(
            p.data_request_pool[&dr_pointer].info.current_reveal_round,
            1
        );

        let reveal_transaction = RevealTransaction::new(
            RevealTransactionBody::new(dr_pointer, vec![], PublicKeyHash::default()),
            vec![KeyedSignature::default()],
        );

        p.process_reveal(&reveal_transaction, &fake_block_hash)
            .unwrap();

        // Update stages
        assert!(p.update_data_request_stages().is_empty());
        // Now in reveal stage 2
        assert_eq!(
            p.data_request_pool[&dr_pointer].stage,
            DataRequestStage::REVEAL
        );
        assert_eq!(
            p.data_request_pool[&dr_pointer].info.current_reveal_round,
            2
        );

        let reveal_transaction2 = RevealTransaction::new(
            RevealTransactionBody::new(dr_pointer, vec![], pk2.pkh()),
            vec![KeyedSignature {
                signature: Signature::default(),
                public_key: pk2,
            }],
        );
        p.process_reveal(&reveal_transaction2, &fake_block_hash)
            .unwrap();

        // Update stages
        assert!(p.update_data_request_stages().is_empty());
        // Now in tally stage
        assert_eq!(
            p.data_request_pool[&dr_pointer].stage,
            DataRequestStage::TALLY
        );
    }

    #[test]
    fn test_from_reveal_to_tally_3_stages_zero_reveals() {
        let (_epoch, fake_block_hash, mut p, dr_pointer) = add_data_requests_with_3_reveal_stages();

        let commit_transaction = CommitTransaction::new(
            CommitTransactionBody::without_collateral(
                dr_pointer,
                Hash::default(),
                DataRequestEligibilityClaim::default(),
            ),
            vec![KeyedSignature::default()],
        );

        let pk2 = PublicKey {
            compressed: 0,
            bytes: [1; 32],
        };
        let commit_transaction2 = CommitTransaction::new(
            CommitTransactionBody::without_collateral(
                dr_pointer,
                Hash::default(),
                DataRequestEligibilityClaim::default(),
            ),
            vec![KeyedSignature {
                signature: Signature::default(),
                public_key: pk2,
            }],
        );

        p.process_commit(&commit_transaction, &fake_block_hash)
            .unwrap();
        p.process_commit(&commit_transaction2, &fake_block_hash)
            .unwrap();

        // Update stages
        assert!(p.update_data_request_stages().is_empty());
        // Now in reveal stage 1
        assert_eq!(
            p.data_request_pool[&dr_pointer].stage,
            DataRequestStage::REVEAL
        );
        assert_eq!(
            p.data_request_pool[&dr_pointer].info.current_reveal_round,
            1
        );

        // Update stages
        assert!(p.update_data_request_stages().is_empty());
        // Now in reveal stage 2
        assert_eq!(
            p.data_request_pool[&dr_pointer].stage,
            DataRequestStage::REVEAL
        );
        assert_eq!(
            p.data_request_pool[&dr_pointer].info.current_reveal_round,
            2
        );

        // Update stages
        assert!(p.update_data_request_stages().is_empty());
        // Now in reveal stage 3
        assert_eq!(
            p.data_request_pool[&dr_pointer].stage,
            DataRequestStage::REVEAL
        );
        assert_eq!(
            p.data_request_pool[&dr_pointer].info.current_reveal_round,
            3
        );

        // Update stages
        assert!(p.update_data_request_stages().is_empty());
        // Now in tally stage, after 3 reveal stages with no reveals
        assert_eq!(
            p.data_request_pool[&dr_pointer].stage,
            DataRequestStage::TALLY
        );
    }

    #[test]
    fn test_from_tally_to_storage() {
        let (epoch, fake_block_hash, p, dr_pointer) = add_data_requests();
        let (fake_block_hash, p, dr_pointer) =
            from_commit_to_reveal(epoch, fake_block_hash, p, dr_pointer);
        let (fake_block_hash, p, dr_pointer) = from_reveal_to_tally(fake_block_hash, p, dr_pointer);

        from_tally_to_storage(fake_block_hash, p, dr_pointer);
    }

    #[test]
    fn my_claims() {
        // Test the `add_own_reveal` function
        let (_epoch, fake_block_hash, mut p, dr_pointer) = add_data_requests();

        let commit_transaction = CommitTransaction::new(
            CommitTransactionBody::without_collateral(
                dr_pointer,
                Hash::default(),
                DataRequestEligibilityClaim::default(),
            ),
            vec![KeyedSignature::default()],
        );

        let reveal_transaction = RevealTransaction::new(
            RevealTransactionBody::new(dr_pointer, vec![], PublicKeyHash::default()),
            vec![KeyedSignature::default()],
        );

        // Add reveal transaction for this commit, will be returned by the update_data_request_stages
        // function when the data request is in reveal stage
        p.insert_reveal(dr_pointer, reveal_transaction.clone());

        assert_eq!(
            p.waiting_for_reveal.get(&dr_pointer),
            Some(&reveal_transaction)
        );

        p.process_commit(&commit_transaction, &fake_block_hash)
            .unwrap();

        // Still in commit stage until we update
        assert_eq!(
            p.data_request_pool[&dr_pointer].stage,
            DataRequestStage::COMMIT
        );

        // Update stages. This will return our reveal transaction
        let my_reveals = p.update_data_request_stages();
        assert_eq!(my_reveals.len(), 1);
        let my_reveal = &my_reveals[0];
        assert_eq!(my_reveal, &reveal_transaction);
        assert_eq!(
            p.waiting_for_reveal.get(&dr_pointer),
            Some(&reveal_transaction)
        );

        // Now in reveal stage
        assert_eq!(
            p.data_request_pool[&dr_pointer].stage,
            DataRequestStage::REVEAL
        );

        let (_, p, dr_pointer) = from_reveal_to_tally(fake_block_hash, p, dr_pointer);

        assert_eq!(p.waiting_for_reveal.get(&dr_pointer), None);

        // Now in tally stage
        assert_eq!(
            p.data_request_pool[&dr_pointer].stage,
            DataRequestStage::TALLY
        );
    }

    #[test]
    fn update_no_commits() {
        let (_epoch, fake_block_hash, mut p, dr_pointer) = add_data_requests();

        assert_eq!(
            p.data_request_pool[&dr_pointer].stage,
            DataRequestStage::COMMIT
        );
        assert_eq!(
            p.data_request_pool[&dr_pointer].info.current_commit_round,
            1
        );

        // Since extra_commit_rounds = 0, updating again in commit stage will
        // move the data request to tally stage
        assert!(p.update_data_request_stages().is_empty());

        // Now in tally stage
        assert_eq!(
            p.data_request_pool[&dr_pointer].stage,
            DataRequestStage::TALLY
        );

        from_tally_to_storage(fake_block_hash, p, dr_pointer);
    }

    #[test]
    fn update_no_commits_1_extra() {
        let (_epoch, fake_block_hash, mut p, dr_pointer) = add_data_requests_with_3_commit_stages();

        // First commitment round
        assert_eq!(
            p.data_request_pool[&dr_pointer].stage,
            DataRequestStage::COMMIT
        );
        assert_eq!(
            p.data_request_pool[&dr_pointer].info.current_commit_round,
            1
        );
        assert_eq!(p.data_request_pool[&dr_pointer].backup_witnesses(), 1);

        // Second commitment round
        assert!(p.update_data_request_stages().is_empty());
        assert_eq!(
            p.data_request_pool[&dr_pointer].stage,
            DataRequestStage::COMMIT
        );
        assert_eq!(
            p.data_request_pool[&dr_pointer].info.current_commit_round,
            2
        );
        assert_eq!(p.data_request_pool[&dr_pointer].backup_witnesses(), 2);

        // Third commitment round
        assert!(p.update_data_request_stages().is_empty());
        assert_eq!(
            p.data_request_pool[&dr_pointer].stage,
            DataRequestStage::COMMIT
        );
        assert_eq!(
            p.data_request_pool[&dr_pointer].info.current_commit_round,
            3
        );
        assert_eq!(p.data_request_pool[&dr_pointer].backup_witnesses(), 4);

        // Since extra_commit_rounds = 1, updating again in commit stage will
        // move the data request to tally stage
        assert!(p.update_data_request_stages().is_empty());

        // Now in tally stage
        assert_eq!(
            p.data_request_pool[&dr_pointer].stage,
            DataRequestStage::TALLY
        );

        from_tally_to_storage(fake_block_hash, p, dr_pointer);
    }

    #[test]
    fn update_commits_1_extra() {
        let (epoch, fake_block_hash, mut p, dr_pointer) = add_data_requests_with_3_commit_stages();

        // 2 extra commit rounds
        assert_eq!(
            p.data_request_pool[&dr_pointer].stage,
            DataRequestStage::COMMIT
        );
        assert_eq!(
            p.data_request_pool[&dr_pointer].info.current_commit_round,
            1
        );
        assert_eq!(p.data_request_pool[&dr_pointer].backup_witnesses(), 1);

        assert!(p.update_data_request_stages().is_empty());

        // Add commit and update stage
        let (_fake_block_hash, p, dr_pointer) =
            from_commit_to_reveal(epoch, fake_block_hash, p, dr_pointer);

        // Since extra_commit_rounds = 1, and we received all the commits,
        // updating again in commit stage will move the data request to reveal stage
        assert_eq!(
            p.data_request_pool[&dr_pointer].stage,
            DataRequestStage::REVEAL
        );
    }
}
