use std::collections::{BTreeMap, HashMap, HashSet};
use std::convert::TryFrom;

use log::{debug, info};
use serde::{Deserialize, Serialize};

use crate::{
    chain::{
        DataRequestOutput, DataRequestReport, DataRequestStage, DataRequestState, Epoch,
        EpochConstants, Hash, Hashable, PublicKeyHash, ValueTransferOutput,
    },
    error::{DataRequestError, TransactionError},
    radon_report::{RadonReport, Stage, TypeLike},
    transaction::{CommitTransaction, DRTransaction, RevealTransaction, TallyTransaction},
};

/// Pool of active data requests
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct DataRequestPool {
    /// Current active data request, in which this node has announced commitments.
    /// Key: DRTransaction hash, Value: Reveal Transaction
    pub waiting_for_reveal: HashMap<Hash, RevealTransaction>,
    /// List of active data request output pointers ordered by epoch (for mining purposes)
    pub data_requests_by_epoch: BTreeMap<Epoch, HashSet<Hash>>,
    /// List of active data requests indexed by output pointer
    pub data_request_pool: HashMap<Hash, DataRequestState>,
    /// List of data requests that should be persisted into storage
    pub to_be_stored: Vec<DataRequestReport>,
}

impl DataRequestPool {
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
    pub fn get_reveals(&self, dr_pointer: &Hash) -> Option<Vec<&RevealTransaction>> {
        self.data_request_pool.get(dr_pointer).map(|dr_state| {
            let mut reveals: Vec<&RevealTransaction> = dr_state.info.reveals.values().collect();
            reveals.sort_unstable_by_key(|reveal| reveal.body.pkh);

            reveals
        })
    }

    /// Insert a reveal transaction into the pool
    pub fn insert_reveal(&mut self, dr_pointer: Hash, reveal: RevealTransaction) {
        self.waiting_for_reveal.insert(dr_pointer, reveal);
    }

    /// Get all the reveals
    pub fn get_all_reveals(&self) -> HashMap<Hash, Vec<RevealTransaction>> {
        self.data_request_pool
            .iter()
            .filter_map(|(dr_pointer, dr_state)| {
                if let DataRequestStage::TALLY = dr_state.stage {
                    let mut reveals: Vec<RevealTransaction> =
                        dr_state.info.reveals.values().cloned().collect();
                    reveals.sort_unstable_by_key(|reveal| reveal.body.pkh);

                    Some((*dr_pointer, reveals))
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
        let dr_report = Self::resolve_data_request(&mut self.data_request_pool, tally, block_hash)?;

        // Since this method does not have access to the storage, we save the
        // "to be stored" inside a vector and provide another method to store them
        self.to_be_stored.push(dr_report);

        Ok(())
    }

    /// Removes a resolved data request from the data request pool, returning the `DataRequestOutput`
    /// and a `DataRequestInfoStorage` which should be persisted into storage.
    fn resolve_data_request(
        data_request_pool: &mut HashMap<Hash, DataRequestState>,
        tally_tx: TallyTransaction,
        block_hash: &Hash,
    ) -> Result<DataRequestReport, failure::Error> {
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
        // Update the stage of the active data requests
        self.data_request_pool
            .iter_mut()
            .filter_map(|(dr_pointer, dr_state)| {
                // We can notify the user that a data request from "my_claims" is available
                // for reveal.
                if dr_state.update_stage() {
                    if let DataRequestStage::REVEAL = dr_state.stage {
                        // When a data request changes from commit stage to reveal stage, it should
                        // be removed from the "data_requests_by_epoch" map, which stores the data
                        // requests potentially available for commitment
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

                        if let Some(transaction) = waiting_for_reveal.remove(dr_pointer) {
                            // We submitted a commit for this data request!
                            // But has it been included into the block?
                            let pkh = PublicKeyHash::from_public_key(
                                &transaction.signatures[0].public_key,
                            );
                            if dr_state.info.commits.contains_key(&pkh) {
                                // We found our commit, return the reveal transaction to be sent
                                return Some(transaction);
                            } else {
                                info!(
                                    "The sent commit transaction has not been \
                                     selected to be part of the data request {:?}",
                                    dr_pointer
                                );
                                debug!(
                                    "Commit with pkh ({}) removed from the list of commits waiting \
                                     for reveal",
                                    pkh
                                );
                            }
                        }
                    }
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
        epoch_constants: EpochConstants,
        block_hash: &Hash,
    ) -> Result<(), failure::Error> {
        // A data request output should have a valid value transfer input
        // Which we assume valid as it should have been already verified

        // time_lock_epoch: The epoch during which we will start accepting
        // commitments for this data request
        // This is not the epoch which contains the timestamp, but the next one
        let time_lock_epoch = epoch_constants
            .epoch_at(
                (dr_transaction.body.dr_output.data_request.time_lock as i64)
                    .saturating_add(i64::from(epoch_constants.checkpoints_period - 1)),
            )
            // Any data request with time lock set to before checkpoint zero
            // will be executed as soon as possible.
            .unwrap_or(0);
        let dr_epoch = std::cmp::max(epoch, time_lock_epoch);
        self.add_data_request(dr_epoch, dr_transaction.clone(), block_hash)
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
    pub fn finished_data_requests(&mut self) -> Vec<DataRequestReport> {
        std::mem::replace(&mut self.to_be_stored, vec![])
    }
}

/// Function to calculate the value transfer reward
pub fn calculate_dr_vt_reward(dr_output: &DataRequestOutput) -> u64 {
    let total_reward = dr_output.total_witnesses_reward();

    total_reward / u64::from(dr_output.witnesses)
}

// FIXME(#640): replace with real truthness check function from radon engine
// (currently we assume that all nodes are honest)
pub fn true_revealer<RT>(_reveal: &RevealTransaction, _report: &RadonReport<RT>) -> bool
where
    RT: TypeLike,
{
    true
}

pub fn create_tally<RT>(
    dr_pointer: Hash,
    dr_output: &DataRequestOutput,
    pkh: PublicKeyHash,
    report: &RadonReport<RT>,
    reveals: Vec<RevealTransaction>,
) -> Result<TallyTransaction, failure::Error>
where
    RT: TypeLike,
{
    if let Stage::Tally(tally_metadata) = &report.metadata {
        let reveal_reward = calculate_dr_vt_reward(dr_output);

        let liars = &tally_metadata.liars;

        if reveals.len() != liars.len() {
            return Err(TransactionError::MismatchingLiarsNumber {
                reveals_n: reveals.len(),
                inputs_n: liars.len(),
            }
            .into());
        }

        let mut outputs: Vec<ValueTransferOutput> = reveals
            .iter()
            .zip(liars.iter())
            .filter_map(|(reveal, &liar)| {
                // Only reward reveals in consensus
                if !liar {
                    let vt_output = ValueTransferOutput {
                        pkh: reveal.body.pkh,
                        value: reveal_reward,
                        time_lock: 0,
                    };
                    Some(vt_output)
                } else {
                    None
                }
            })
            .collect();

        let n_honest = outputs.len() as u16;
        let n_reveals = reveals.len() as u16;
        // Create tally change for the data request creator
        if dr_output.witnesses > n_honest {
            debug!("Created tally change for the data request creator");
            let tally_change = reveal_reward * u64::from(dr_output.witnesses - n_honest)
                + dr_output.reveal_fee * u64::from(dr_output.witnesses - n_reveals);
            let vt_output_change = ValueTransferOutput {
                pkh,
                value: tally_change,
                time_lock: 0,
            };
            outputs.push(vt_output_change);
        }

        let tally_bytes = Vec::try_from(report)?;

        Ok(TallyTransaction::new(dr_pointer, tally_bytes, outputs))
    } else {
        Err(TransactionError::NoTallyStage.into())
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
        p.process_data_request(
            &dr_transaction,
            epoch,
            EpochConstants::default(),
            &fake_block_hash,
        )
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
            extra_reveal_rounds: 2,
            ..DataRequestOutput::default()
        };
        let dr_transaction = DRTransaction::new(
            DRTransactionBody::new(vec![Input::default()], vec![], dr_output),
            vec![KeyedSignature::default()],
        );
        let dr_pointer = dr_transaction.hash();

        let mut p = DataRequestPool::default();
        p.process_data_request(
            &dr_transaction,
            epoch,
            EpochConstants::default(),
            &fake_block_hash,
        )
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
            CommitTransactionBody::new(
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
        let tally_transaction = TallyTransaction::new(dr_pointer, vec![], vec![]);

        // There is nothing to be stored yet
        assert_eq!(p.to_be_stored.len(), 0);

        // Process tally: this will remove the data request from the pool
        p.process_tally(&tally_transaction, &fake_block_hash)
            .unwrap();

        // And the data request has been removed from the pool
        assert_eq!(p.data_request_pool.get(&dr_pointer), None);

        // Update stages
        assert!(p.update_data_request_stages().is_empty());

        assert_eq!(p.to_be_stored.len(), 1);
        assert_eq!(p.to_be_stored[0].tally.dr_pointer, dr_pointer);
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
            CommitTransactionBody::new(
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
            CommitTransactionBody::new(
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

        // Now in reveal stage 0
        assert_eq!(
            p.data_request_pool[&dr_pointer].stage,
            DataRequestStage::REVEAL
        );
        assert_eq!(
            p.data_request_pool[&dr_pointer].info.current_reveal_round,
            0
        );

        let reveal_transaction = RevealTransaction::new(
            RevealTransactionBody::new(dr_pointer, vec![], PublicKeyHash::default()),
            vec![KeyedSignature::default()],
        );

        p.process_reveal(&reveal_transaction, &fake_block_hash)
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
        // Now in tally stage
        assert_eq!(
            p.data_request_pool[&dr_pointer].stage,
            DataRequestStage::TALLY
        );
    }

    #[test]
    fn test_from_reveal_to_tally_3_stages_completed() {
        let (_epoch, fake_block_hash, mut p, dr_pointer) = add_data_requests_with_3_reveal_stages();

        let commit_transaction = CommitTransaction::new(
            CommitTransactionBody::new(
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
            CommitTransactionBody::new(
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

        // Now in reveal stage 0
        assert_eq!(
            p.data_request_pool[&dr_pointer].stage,
            DataRequestStage::REVEAL
        );
        assert_eq!(
            p.data_request_pool[&dr_pointer].info.current_reveal_round,
            0
        );

        let reveal_transaction = RevealTransaction::new(
            RevealTransactionBody::new(dr_pointer, vec![], PublicKeyHash::default()),
            vec![KeyedSignature::default()],
        );

        p.process_reveal(&reveal_transaction, &fake_block_hash)
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
            CommitTransactionBody::new(
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
        assert_eq!(p.waiting_for_reveal.get(&dr_pointer), None);

        // Now in reveal stage
        assert_eq!(
            p.data_request_pool[&dr_pointer].stage,
            DataRequestStage::REVEAL
        );

        from_reveal_to_tally(fake_block_hash, p, dr_pointer);
    }

    #[test]
    fn update_multiple_times() {
        // Only the first consecutive call to update_data_request_stages should change the state
        let (epoch, fake_block_hash, mut p, dr_pointer) = add_data_requests();

        assert!(p.update_data_request_stages().is_empty());

        assert_eq!(
            p.data_request_pool[&dr_pointer].stage,
            DataRequestStage::COMMIT
        );

        let (fake_block_hash, mut p, dr_pointer) =
            from_commit_to_reveal(epoch, fake_block_hash, p, dr_pointer);

        // Update stages
        assert!(p.update_data_request_stages().is_empty());

        // Now in reveal stage
        assert_eq!(
            p.data_request_pool[&dr_pointer].stage,
            DataRequestStage::REVEAL
        );

        let (fake_block_hash, mut p, dr_pointer) =
            from_reveal_to_tally(fake_block_hash, p, dr_pointer);

        // Update stages
        assert!(p.update_data_request_stages().is_empty());

        // Now in tally stage
        assert_eq!(
            p.data_request_pool[&dr_pointer].stage,
            DataRequestStage::TALLY
        );

        from_tally_to_storage(fake_block_hash, p, dr_pointer);
    }
}
