use log::{debug, info};
use std::collections::{BTreeMap, HashMap, HashSet};
use witnet_data_structures::{
    chain::{
        Block, DataRequestOutput, Epoch, Hash, Hashable, Input, Output, OutputPointer, RevealInput,
        Transaction,
    },
    serializers::decoders::TryFrom,
};

#[derive(Clone, Debug, Default)]
pub struct DataRequestPool {
    /// Current active data request, in which this node has announced commitments.
    /// Key: Data Request Pointer, Value: Reveal Transaction
    waiting_for_reveal: HashMap<OutputPointer, Transaction>,
    /// List of active data request output pointers ordered by epoch (for mining purposes)
    data_requests_by_epoch: BTreeMap<Epoch, HashSet<OutputPointer>>,
    /// List of active data requests indexed by output pointer
    data_request_pool: HashMap<OutputPointer, DataRequestState>,
    /// List of data requests that should be persisted into storage
    to_be_stored: Vec<(OutputPointer, DataRequestInfoStorage)>,
    /// Cache which maps commit_pointer to data_request_pointer
    /// and reveal_pointer to data_request_pointer
    dr_pointer_cache: HashMap<OutputPointer, OutputPointer>,
}

impl DataRequestPool {
    /// Add a data request to the data request pool
    pub fn add_data_request(
        &mut self,
        epoch: Epoch,
        output_pointer: OutputPointer,
        data_request: DataRequestOutput,
    ) {
        let dr_state = DataRequestState::new(data_request, epoch);

        self.data_requests_by_epoch
            .entry(epoch)
            .or_insert_with(HashSet::new)
            .insert(output_pointer.clone());
        self.data_request_pool.insert(output_pointer, dr_state);
    }

    /// Our node made a commitment and is waiting for it to be included in a block.
    /// When that happens, it should send the reveal transaction.
    /// Returns the old reveal for this data request, if it exists, on success.
    /// On failure returns the reveal transaction back.
    #[allow(unused)]
    pub fn add_own_reveal(
        &mut self,
        data_request_pointer: OutputPointer,
        reveal: Transaction,
    ) -> Result<Option<Transaction>, Transaction> {
        // TODO: this checks could be avoided if instead of `Transaction` we accept a
        // `RevealTransaction`, which is defined as
        // struct RevealTransaction(Transaction)
        // but can only be constructed using a method which checks the validity
        // The reveal transaction can only have one input and one output
        if reveal.inputs.len() == 1 && reveal.outputs.len() == 1 {
            // Input: CommitInput, Output: RevealOutput
            match (&reveal.inputs[0], &reveal.outputs[0]) {
                (&Input::Commit(..), &Output::Reveal(..)) => {
                    Ok(self.waiting_for_reveal.insert(data_request_pointer, reveal))
                }
                _ => Err(reveal),
            }
        } else {
            Err(reveal)
        }
    }

    /// Add a commit to the corresponding data request
    pub fn add_commit(&mut self, z: &Input, pointer: OutputPointer, block_hash: &Hash) {
        let transaction_id = pointer.transaction_id;
        // For a commit output, we need to get the corresponding data request input
        if let Input::DataRequest(dri) = z {
            let dr_pointer = dri.output_pointer();

            // The data request must be from a previous block, and must not be timelocked.
            // This is not checked here, as it should have made the block invalid.
            if let Some(dr) = self.data_request_pool.get_mut(&dr_pointer) {
                dr.add_commit(pointer.clone());
                // Save the commit output pointer into a cache, to be able to
                // retrieve data requests when we have the commit output pointer
                // but no data request output pointer
                self.dr_pointer_cache.insert(pointer, dr_pointer);
            } else {
                // This can happen when a data request was not stored into the dr_pool.
                // For example, a very old data request that just now got a commitment.
                // Since we currently store all the data requests in memory, a failure
                // here is a logic error, therefore we panic.
                panic!(
                    "Block contains a commitment for an unknown data request:\n\
                     Block hash: {b}\n\
                     Transaction hash: {t}\n\
                     Commit output pointer: {p:?}\n\
                     Data request pointer: {d:?}",
                    b = block_hash,
                    t = transaction_id,
                    p = pointer,
                    d = dr_pointer,
                );
            }
        } else {
            // This panic implies a logic error in the block validation
            panic!(
                "Invalid transaction got accepted into a valid block.\n\
                 Missing Input::DataRequest\n\
                 Block hash: {b}\n\
                 Transaction hash: {t}\n\
                 Commit output pointer: {p:?}",
                b = block_hash,
                t = transaction_id,
                p = pointer,
            );
        }
    }

    pub fn add_reveal(&mut self, z: &Input, pointer: OutputPointer, block_hash: &Hash) {
        let transaction_id = pointer.transaction_id;
        // For a reveal output, we need to get the corresponding commit input
        if let Input::Commit(commit_input) = z {
            let commit_pointer = commit_input.output_pointer();
            if let Some(dr_pointer) = self.dr_pointer_cache.get(&commit_pointer) {
                if let Some(dr) = self.data_request_pool.get_mut(&dr_pointer) {
                    dr.add_reveal(pointer.clone());
                    // Save the reveal output pointer into a cache
                    self.dr_pointer_cache.insert(pointer, dr_pointer.clone());
                } else {
                    panic!(
                        "Block contains a reveal for an unknown commitment:\n\
                         Block hash: {b}\n\
                         Transaction hash: {t}\n\
                         Reveal output pointer: {p:?}\n\
                         Commit output pointer: {c:?}\n\
                         Data request pointer: {d:?}",
                        b = block_hash,
                        t = transaction_id,
                        p = pointer,
                        c = commit_pointer,
                        d = dr_pointer,
                    );
                }
            } else {
                // TODO: on cache miss we should query the storage for the required
                // output pointer and retry the validation. Since this function should
                // not depend on the storage, maybe a list of pending transactions similar
                // to "to_be_stored" would be useful: we need to store pending transactions
                // and missing output pointers. However that's currently unnecessary because
                // we will always persist the cache
                panic!(
                    "Block contains a reveal for a commitment not in the cache:\n\
                     Block hash: {b}\n\
                     Transaction hash: {t}\n\
                     Reveal output pointer: {p:?}\n\
                     Commit output pointer: {c:?}",
                    b = block_hash,
                    t = transaction_id,
                    p = pointer,
                    c = commit_pointer,
                );
            }
        } else {
            // This panic implies a logic error in the block validation
            panic!(
                "Invalid transaction got accepted into a valid block:\n\
                 Missing Input::Commit\n\
                 Block hash: {b}\n\
                 Transaction hash: {t}\n\
                 Reveal output pointer: {p:?}",
                b = block_hash,
                t = transaction_id,
                p = pointer,
            );
        }
    }

    #[allow(clippy::needless_pass_by_value)]
    pub fn add_tally(&mut self, reveal: &RevealInput, pointer: OutputPointer, block_hash: &Hash) {
        let transaction_id = pointer.transaction_id;
        // For a tally output, we need to get the corresponding reveal input
        // Which is the previous transaction in the list
        // inputs[counter - 1] must exists because counter == min(inputs.len(), outputs.len())
        let reveal_pointer = reveal.output_pointer();

        if let Some(dr_pointer) = self.dr_pointer_cache.get(&reveal_pointer).cloned() {
            if let Ok((_dr, dr_info)) = Self::resolve_data_request(
                &mut self.data_request_pool,
                &dr_pointer,
                pointer.clone(),
            ) {
                // Since this method does not have access to the storage, we save the
                // "to be stored" inside a vector and provide another method to store them
                self.to_be_stored.push((dr_pointer, dr_info.clone()));
                // Remove all the commit/reveal pointers from the dr_pointer_cache
                for p in dr_info.commits.iter().chain(dr_info.reveals.iter()) {
                    self.dr_pointer_cache.remove(p);
                }
            } else {
                panic!(
                    "Block contains a tally for an unknown data request:\n\
                     Block hash: {b}\n\
                     Transaction hash: {t}\n\
                     Reveal output pointer: {p:?}\n\
                     Data request pointer: {d:?}",
                    b = block_hash,
                    t = transaction_id,
                    p = pointer,
                    d = dr_pointer,
                );
            }
        } else {
            // TODO: on cache miss
            panic!(
                "Block contains a tally for a reveal not in the cache:\n\
                 Block hash: {b}\n\
                 Transaction hash: {t}\n\
                 Reveal output pointer: {r:?}",
                b = block_hash,
                t = transaction_id,
                r = reveal_pointer,
            );
        }
    }

    /// Removes a resolved data request from the data request pool, returning the `DataRequestOutput`
    /// and a `DataRequestInfoStorage` which should be persisted into storage.
    pub fn resolve_data_request(
        data_request_pool: &mut HashMap<OutputPointer, DataRequestState>,
        dr_pointer: &OutputPointer,
        tally_pointer: OutputPointer,
    ) -> Result<(DataRequestOutput, DataRequestInfoStorage), ()> {
        let dr_state = data_request_pool.remove(dr_pointer).ok_or(())?;
        let (dr, dr_info) = dr_state.add_tally(tally_pointer);

        Ok((dr, dr_info))
    }

    /// Return the list of data requests in which this node has participated and are ready
    /// for reveal (the node should send a reveal transaction).
    /// This function must be called after `add_data_requests_from_block`, in order to update
    /// the stage of all the data requests.
    pub fn update_data_request_stages(&mut self) -> Vec<Transaction> {
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
                        if let Some(transaction) = waiting_for_reveal.remove(dr_pointer) {
                            // We submitted a commit for this data request!
                            // But has it been included into the block?
                            let commit_pointer = match &transaction.inputs[0] {
                                Input::Commit(commit) => commit.output_pointer(),
                                _ => panic!("Invalid format for reveal transaction"),
                            };
                            if dr_state.info.commits.contains(&commit_pointer) {
                                // We found our commit, return the reveal transaction to be sent
                                return Some(transaction);
                            } else {
                                info!(
                                    "The sent commit transaction has not been \
                                     selected to be part of the data request {:?}",
                                    dr_pointer
                                );
                                debug!(
                                    "Commit {:?} removed from the list of commits waiting \
                                     for reveal",
                                    commit_pointer
                                );
                            }
                        }

                        // When a data request changes from commit stage to reveal stage, it should
                        // be removed from the "data_requests_by_epoch" map, which stores the data
                        // requests potentially available for commitment
                        if let Some(hs) = data_requests_by_epoch.get_mut(&dr_state.epoch) {
                            let present = hs.remove(dr_pointer);
                            if !present {
                                // This could be a warn! or a debug! instead of a panic
                                panic!(
                                    "Data request {:?} was not present in the \
                                     data_requests_by_epoch map (epoch #{})",
                                    dr_pointer, dr_state.epoch
                                );
                            }
                        }
                    }
                }

                None
            })
            .collect()
    }

    /// Process a transaction from a block and update the data request pool accordingly:
    /// * New data requests are inserted and wait for commitments
    /// * New commitments are added to their respective data requests, updating the stage to reveal
    /// * New reveals are added to their respective data requests, updating the stage to tally
    /// The epoch is needed as the key to the available data requests map
    /// The block hash is only used for debugging purposes
    pub fn process_transaction(&mut self, t: &Transaction, epoch: Epoch, block_hash: &Hash) {
        let transaction_id = t.hash();
        for (i, (z, s)) in t.inputs.iter().zip(t.outputs.iter()).enumerate() {
            let output_index = i as u32;
            let pointer = OutputPointer {
                transaction_id,
                output_index,
            };
            match s {
                Output::DataRequest(dr) => {
                    // A data request output should have a valid value transfer input
                    // Which we assume valid as it should have been already verified
                    // time_lock_epoch: The epoch during which we will start accepting
                    // commitments for this data request
                    // FIXME(#338): implement time lock
                    // An enhancement to the epoch manager would be a handler GetState which returns
                    // the needed constants to calculate the current epoch. This way we avoid all the
                    // calls to GetEpoch
                    let time_lock_epoch = 0;
                    let dr_epoch = std::cmp::max(epoch, time_lock_epoch);
                    self.add_data_request(dr_epoch, pointer.clone(), dr.clone());
                }
                Output::Commit(_commit) => {
                    self.add_commit(z, pointer, block_hash);
                }
                Output::Reveal(_reveal) => {
                    self.add_reveal(z, pointer, block_hash);
                }
                Output::Tally(tally) => {
                    // It is impossible to have a tally in this iterator, because we are
                    // iterating in pairs and the tally output does not have an input.
                    // This panic implies a logic error in the block validation
                    panic!(
                        "Invalid transaction got accepted into a valid block:\n\
                         Tally output {tally:?} has an invalid input:\n\
                         {input:?}\n\
                         Block hash: {b}\n\
                         Transaction hash: {t}",
                        tally = tally,
                        input = z,
                        b = block_hash,
                        t = transaction_id,
                    );
                }
                Output::ValueTransfer(_) => {}
            }
        }

        // Handle tally. A tally transaction has N inputs and N+1 outputs,
        // at least 1 input and 2 outputs.
        // The last output is the tally, with no corresponding input,
        // and the last input-output pair is (RevealInput, ValueTransferOutput).
        let possibly_tally = t.outputs.len() >= 2 && t.inputs.len() == t.outputs.len() - 1;

        if possibly_tally {
            let tally_index = t.outputs.len() - 1;
            match (&t.inputs[tally_index - 1], &t.outputs[tally_index]) {
                (Input::Reveal(reveal), Output::Tally(_tally)) => {
                    // Assume that all the reveal inputs point to the same data request
                    // (as that should have been already validated)
                    // And assume that the tally transaction contains as many reveal inputs
                    // as there are reveals for this data request (also should have been validated)
                    let pointer = OutputPointer {
                        transaction_id,
                        output_index: tally_index as u32,
                    };
                    self.add_tally(reveal, pointer, block_hash);
                }
                (_, Output::Tally(_)) => {
                    // This panic implies a logic error in the block validation
                    panic!(
                        "Tally transaction must be next to a transaction with reveal input \
                         and value transfer output.\n\
                         Block hash: {b}\n\
                         Transaction hash: {t}\n\
                         Inputs: {inputs:?}\n\
                         Outputs: {outputs:?}",
                        b = block_hash,
                        t = transaction_id,
                        inputs = t.inputs,
                        outputs = t.outputs
                    );
                }
                _ => {
                    // Assume there is no tally in this transaction
                }
            }
        }
    }

    /// Add all the data request-related transactions into the data request pool.
    /// This includes new data requests, commitments, reveals and tallys.
    /// Return the list of data requests in which this node has participated and are ready
    /// for reveal (the node should send a reveal transaction).
    pub fn add_data_requests_from_block(
        &mut self,
        block: &Block,
        epoch: Epoch,
    ) -> Vec<Transaction> {
        let block_hash = block.hash();
        for t in &block.txns {
            self.process_transaction(t, epoch, &block_hash);
        }

        self.update_data_request_stages()
    }

    /// Get the detailed state of a data request.
    #[allow(unused)]
    pub fn data_request_state(
        &self,
        data_request_pointer: &OutputPointer,
    ) -> Option<&DataRequestState> {
        self.data_request_pool.get(data_request_pointer)
    }

    /// Get the data request info of the finished data requests, to be persisted to the storage
    #[allow(unused)]
    pub fn finished_data_requests(&mut self) -> Vec<(OutputPointer, DataRequestInfoStorage)> {
        std::mem::replace(&mut self.to_be_stored, vec![])
    }
}

/// State of data requests in progress (stored in memory)
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DataRequestState {
    /// Data request output (contains all required information to process it)
    pub data_request: DataRequestOutput,
    /// List of outputs related to this data request
    pub info: DataRequestInfo,
    /// Current stage of this data request
    pub stage: DataRequestStage,
    /// The epoch on which this data request has been or will be unlocked
    // (necessary for removing from the data_requests_by_epoch map)
    pub epoch: Epoch,
}

impl DataRequestState {
    pub fn new(data_request: DataRequestOutput, epoch: Epoch) -> Self {
        let info = DataRequestInfo::default();
        let stage = DataRequestStage::COMMIT;

        Self {
            data_request,
            info,
            stage,
            epoch,
        }
    }

    pub fn add_commit(&mut self, output_pointer: OutputPointer) {
        assert_eq!(self.stage, DataRequestStage::COMMIT);
        self.info.commits.insert(output_pointer);
    }

    pub fn add_reveal(&mut self, output_pointer: OutputPointer) {
        assert_eq!(self.stage, DataRequestStage::REVEAL);
        self.info.reveals.insert(output_pointer);
    }

    pub fn add_tally(
        mut self,
        output_pointer: OutputPointer,
    ) -> (DataRequestOutput, DataRequestInfoStorage) {
        assert_eq!(self.stage, DataRequestStage::TALLY);
        self.info.tally = Some(output_pointer);
        // This try_from can only fail if the tally is None, and we have just set it to Some
        (
            self.data_request,
            DataRequestInfoStorage::try_from(self.info).unwrap(),
        )
    }

    /// Advance to the next stage, returning true on success.
    /// Since the data requests are updated by looking at the transactions from a valid block,
    /// the only issue would be that there were no commits in that block.
    pub fn update_stage(&mut self) -> bool {
        let old_stage = self.stage;

        self.stage = match self.stage {
            DataRequestStage::COMMIT => {
                if self.info.commits.is_empty() {
                    DataRequestStage::COMMIT
                } else {
                    DataRequestStage::REVEAL
                }
            }
            DataRequestStage::REVEAL => {
                if self.info.reveals.is_empty() {
                    DataRequestStage::REVEAL
                } else {
                    DataRequestStage::TALLY
                }
            }
            DataRequestStage::TALLY => {
                if self.info.tally.is_none() {
                    DataRequestStage::TALLY
                } else {
                    panic!("Data request in tally stage should have been removed from the pool");
                }
            }
        };

        self.stage != old_stage
    }
}

/// List of outputs related to a data request
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct DataRequestInfo {
    /// List of commitments to resolve the data request
    pub commits: HashSet<OutputPointer>,
    /// List of reveals to the commitments (contains the data request witnet result)
    pub reveals: HashSet<OutputPointer>,
    /// Tally of data request (contains final result)
    pub tally: Option<OutputPointer>,
}

/// Data request information to be persisted into Storage (only for resolved data requests) and
/// using as index the Data Request OutputPointer
#[derive(Clone, Debug)]
pub struct DataRequestInfoStorage {
    /// List of commitment output pointers to resolve the data request
    pub commits: Vec<OutputPointer>,
    /// List of reveal output pointers to the commitments (contains the data request result of the witnet)
    pub reveals: Vec<OutputPointer>,
    /// Tally output pointer (contains final result)
    pub tally: OutputPointer,
}

impl TryFrom<DataRequestInfo> for DataRequestInfoStorage {
    type Error = &'static str;

    fn try_from(x: DataRequestInfo) -> Result<Self, &'static str> {
        if let Some(tally) = x.tally {
            Ok(DataRequestInfoStorage {
                commits: x.commits.into_iter().collect(),
                reveals: x.reveals.into_iter().collect(),
                tally,
            })
        } else {
            Err("Cannot persist unfinished data request (with no Tally)")
        }
    }
}

/// Data request current stage
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum DataRequestStage {
    /// Expecting commitments for data request
    COMMIT,
    /// Expecting reveals to previously published commitments
    REVEAL,
    /// Expecting tally to be included in block
    TALLY,
}

#[cfg(test)]
mod tests {
    use super::*;
    use witnet_data_structures::chain::*;

    fn empty_data_request() -> DataRequestOutput {
        DataRequestOutput {
            data_request: vec![],
            value: 0,
            witnesses: 0,
            backup_witnesses: 0,
            commit_fee: 0,
            reveal_fee: 0,
            tally_fee: 0,
            time_lock: 0,
            pkh: [45; 20],
        }
    }

    fn empty_commit_output() -> CommitOutput {
        CommitOutput {
            commitment: Hash::SHA256([50; 32]),
            value: 4,
        }
    }

    fn empty_reveal_output() -> RevealOutput {
        RevealOutput {
            reveal: vec![],
            pkh: [78; 20],
            value: 5,
        }
    }

    fn empty_tally_output() -> TallyOutput {
        TallyOutput {
            result: vec![],
            pkh: [23; 20],
            value: 6,
        }
    }

    fn empty_value_transfer_input() -> ValueTransferInput {
        ValueTransferInput {
            transaction_id: Hash::SHA256([9; 32]),
            output_index: 0,
        }
    }

    fn empty_value_transfer_output() -> ValueTransferOutput {
        ValueTransferOutput {
            pkh: [25; 20],
            value: 7,
        }
    }

    fn fake_transaction_zip(z: Vec<(Input, Output)>) -> Transaction {
        let mut inputs = vec![];
        let mut outputs = vec![];

        for t in z {
            inputs.push(t.0);
            outputs.push(t.1);
        }

        Transaction {
            version: 0,
            inputs,
            outputs,
            signatures: vec![],
        }
    }

    #[test]
    fn add_data_requests() {
        let fake_block_hash = Hash::SHA256([1; 32]);
        let epoch = 0;
        let data_request = empty_data_request();
        let empty_info = DataRequestInfo::default();
        let transaction = fake_transaction_zip(vec![(
            Input::ValueTransfer(empty_value_transfer_input()),
            Output::DataRequest(data_request.clone()),
        )]);
        let dr_pointer = OutputPointer {
            transaction_id: transaction.hash(),
            output_index: 0,
        };

        let mut p = DataRequestPool::default();
        p.process_transaction(&transaction, epoch, &fake_block_hash);

        assert!(p.waiting_for_reveal.is_empty());
        assert!(p.data_requests_by_epoch[&epoch].contains(&dr_pointer));
        assert_eq!(p.data_request_pool[&dr_pointer].data_request, data_request);
        assert_eq!(p.data_request_pool[&dr_pointer].info, empty_info);
        assert_eq!(
            p.data_request_pool[&dr_pointer].stage,
            DataRequestStage::COMMIT
        );
        assert!(p.to_be_stored.is_empty());
        assert!(p.dr_pointer_cache.is_empty());

        assert!(p.update_data_request_stages().is_empty());
    }

    #[test]
    fn from_commit_to_reveal() {
        let fake_block_hash = Hash::SHA256([1; 32]);
        let epoch = 0;
        let data_request = empty_data_request();
        let transaction = fake_transaction_zip(vec![(
            Input::ValueTransfer(empty_value_transfer_input()),
            Output::DataRequest(data_request.clone()),
        )]);
        let dr_pointer = OutputPointer {
            transaction_id: transaction.hash(),
            output_index: 0,
        };

        let mut p = DataRequestPool::default();
        p.process_transaction(&transaction, epoch, &fake_block_hash);

        assert!(p.data_requests_by_epoch[&epoch].contains(&dr_pointer));

        assert_eq!(
            p.data_request_pool[&dr_pointer].stage,
            DataRequestStage::COMMIT
        );
        // Since there are no commitments to the data request, it should stay in commit stage
        assert!(p.update_data_request_stages().is_empty());

        assert_eq!(
            p.data_request_pool[&dr_pointer].stage,
            DataRequestStage::COMMIT
        );

        let commit_transaction = fake_transaction_zip(vec![(
            Input::DataRequest(DataRequestInput {
                transaction_id: dr_pointer.transaction_id,
                output_index: dr_pointer.output_index,
                poe: [77; 32],
            }),
            Output::Commit(empty_commit_output()),
        )]);

        let commit_pointer = OutputPointer {
            transaction_id: commit_transaction.hash(),
            output_index: 0,
        };

        p.process_transaction(&commit_transaction, epoch + 1, &fake_block_hash);

        // Now we can get the data request pointer from the commit output pointer
        assert_eq!(p.dr_pointer_cache.get(&commit_pointer), Some(&dr_pointer));

        // And we can also get all the commit pointers from the data request
        assert_eq!(
            p.data_request_pool[&dr_pointer]
                .info
                .commits
                .iter()
                .collect::<Vec<_>>(),
            vec![&commit_pointer],
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
        assert!(!p.data_requests_by_epoch[&epoch].contains(&dr_pointer));
    }

    #[test]
    fn from_reveal_to_tally() {
        let fake_block_hash = Hash::SHA256([1; 32]);
        let epoch = 0;
        let data_request = empty_data_request();
        let transaction = fake_transaction_zip(vec![(
            Input::ValueTransfer(empty_value_transfer_input()),
            Output::DataRequest(data_request.clone()),
        )]);
        let dr_pointer = OutputPointer {
            transaction_id: transaction.hash(),
            output_index: 0,
        };

        let mut p = DataRequestPool::default();
        p.process_transaction(&transaction, epoch, &fake_block_hash);

        assert_eq!(
            p.data_request_pool[&dr_pointer].stage,
            DataRequestStage::COMMIT
        );
        // Since there are no commitments to the data request, it should stay in commit stage
        assert!(p.update_data_request_stages().is_empty());

        assert_eq!(
            p.data_request_pool[&dr_pointer].stage,
            DataRequestStage::COMMIT
        );

        let commit = empty_commit_output();
        let commit_transaction = fake_transaction_zip(vec![(
            Input::DataRequest(DataRequestInput {
                transaction_id: dr_pointer.transaction_id,
                output_index: dr_pointer.output_index,
                poe: [77; 32],
            }),
            Output::Commit(commit),
        )]);

        let commit_pointer = OutputPointer {
            transaction_id: commit_transaction.hash(),
            output_index: 0,
        };

        p.process_transaction(&commit_transaction, epoch + 1, &fake_block_hash);

        // Now we can get the data request pointer from the commit output pointer
        assert_eq!(p.dr_pointer_cache.get(&commit_pointer), Some(&dr_pointer));

        // Still in commit stage until we update
        assert_eq!(
            p.data_request_pool[&dr_pointer].stage,
            DataRequestStage::COMMIT
        );

        // Update stages
        assert!(p.update_data_request_stages().is_empty());

        // Now in reveal stage
        assert_eq!(
            p.data_request_pool[&dr_pointer].stage,
            DataRequestStage::REVEAL
        );

        let reveal = empty_reveal_output();
        let reveal_transaction = fake_transaction_zip(vec![(
            Input::Commit(CommitInput {
                transaction_id: commit_pointer.transaction_id,
                output_index: commit_pointer.output_index,
                reveal: vec![77],
                nonce: 444,
            }),
            Output::Reveal(reveal),
        )]);

        let reveal_pointer = OutputPointer {
            transaction_id: reveal_transaction.hash(),
            output_index: 0,
        };

        p.process_transaction(&reveal_transaction, epoch + 2, &fake_block_hash);

        // Now we can get the data request pointer from the commit output pointer and the reveal pointer
        assert_eq!(p.dr_pointer_cache.get(&commit_pointer), Some(&dr_pointer));
        assert_eq!(p.dr_pointer_cache.get(&reveal_pointer), Some(&dr_pointer));

        // And we can also get all the commit/reveal pointers from the data request
        assert_eq!(
            p.data_request_pool[&dr_pointer]
                .info
                .commits
                .iter()
                .collect::<Vec<_>>(),
            vec![&commit_pointer],
        );
        assert_eq!(
            p.data_request_pool[&dr_pointer]
                .info
                .reveals
                .iter()
                .collect::<Vec<_>>(),
            vec![&reveal_pointer],
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
    }

    #[test]
    fn from_tally_to_storage() {
        let fake_block_hash = Hash::SHA256([1; 32]);
        let epoch = 0;
        let data_request = empty_data_request();
        let transaction = fake_transaction_zip(vec![(
            Input::ValueTransfer(empty_value_transfer_input()),
            Output::DataRequest(data_request.clone()),
        )]);
        let dr_pointer = OutputPointer {
            transaction_id: transaction.hash(),
            output_index: 0,
        };

        let mut p = DataRequestPool::default();
        p.process_transaction(&transaction, epoch, &fake_block_hash);

        assert_eq!(
            p.data_request_pool[&dr_pointer].stage,
            DataRequestStage::COMMIT
        );
        // Since there are no commitments to the data request, it should stay in commit stage
        assert!(p.update_data_request_stages().is_empty());

        assert_eq!(
            p.data_request_pool[&dr_pointer].stage,
            DataRequestStage::COMMIT
        );

        let commit = empty_commit_output();
        let commit_transaction = fake_transaction_zip(vec![(
            Input::DataRequest(DataRequestInput {
                transaction_id: dr_pointer.transaction_id,
                output_index: dr_pointer.output_index,
                poe: [77; 32],
            }),
            Output::Commit(commit),
        )]);

        let commit_pointer = OutputPointer {
            transaction_id: commit_transaction.hash(),
            output_index: 0,
        };

        p.process_transaction(&commit_transaction, epoch + 1, &fake_block_hash);

        // Now we can get the data request pointer from the commit output pointer
        assert_eq!(p.dr_pointer_cache.get(&commit_pointer), Some(&dr_pointer),);

        // Still in commit stage until we update
        assert_eq!(
            p.data_request_pool[&dr_pointer].stage,
            DataRequestStage::COMMIT
        );

        // Update stages
        assert!(p.update_data_request_stages().is_empty());

        // Now in reveal stage
        assert_eq!(
            p.data_request_pool[&dr_pointer].stage,
            DataRequestStage::REVEAL
        );

        let reveal = empty_reveal_output();
        let reveal_transaction = fake_transaction_zip(vec![(
            Input::Commit(CommitInput {
                transaction_id: commit_pointer.transaction_id,
                output_index: commit_pointer.output_index,
                reveal: vec![77],
                nonce: 444,
            }),
            Output::Reveal(reveal),
        )]);

        let reveal_pointer = OutputPointer {
            transaction_id: reveal_transaction.hash(),
            output_index: 0,
        };

        p.process_transaction(&reveal_transaction, epoch + 2, &fake_block_hash);

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

        let tally = empty_tally_output();
        let mut tally_transaction = fake_transaction_zip(vec![(
            Input::Reveal(RevealInput {
                transaction_id: reveal_pointer.transaction_id,
                output_index: reveal_pointer.output_index,
            }),
            Output::ValueTransfer(empty_value_transfer_output()),
        )]);
        tally_transaction.outputs.push(Output::Tally(tally.clone()));

        // Now we can get the data request pointer from the commit/reveal pointers
        assert_eq!(p.dr_pointer_cache.get(&commit_pointer), Some(&dr_pointer));
        assert_eq!(p.dr_pointer_cache.get(&reveal_pointer), Some(&dr_pointer));

        // And we can also get all the commit/reveal pointers from the data request
        assert_eq!(
            p.data_request_pool[&dr_pointer]
                .info
                .commits
                .iter()
                .collect::<Vec<_>>(),
            vec![&commit_pointer],
        );
        assert_eq!(
            p.data_request_pool[&dr_pointer]
                .info
                .reveals
                .iter()
                .collect::<Vec<_>>(),
            vec![&reveal_pointer],
        );

        // There is nothing to be stored yet
        assert_eq!(p.to_be_stored.len(), 0);

        // Process tally: this will remove the data request from the pool
        p.process_transaction(&tally_transaction, epoch + 2, &fake_block_hash);

        // Now the cache has been cleared
        assert_eq!(p.dr_pointer_cache.get(&commit_pointer), None);
        assert_eq!(p.dr_pointer_cache.get(&reveal_pointer), None);

        // And the data request has been removed from the pool
        assert_eq!(p.data_request_pool.get(&dr_pointer), None);

        // Update stages
        assert!(p.update_data_request_stages().is_empty());

        assert_eq!(p.to_be_stored.len(), 1);
        assert_eq!(p.to_be_stored[0].0, dr_pointer);
    }

    #[test]
    fn my_claims() {
        // Test the `add_own_reveal` function
        let fake_block_hash = Hash::SHA256([1; 32]);
        let epoch = 0;
        let data_request = empty_data_request();
        let transaction = fake_transaction_zip(vec![(
            Input::ValueTransfer(empty_value_transfer_input()),
            Output::DataRequest(data_request.clone()),
        )]);
        let dr_pointer = OutputPointer {
            transaction_id: transaction.hash(),
            output_index: 0,
        };

        let mut p = DataRequestPool::default();
        p.process_transaction(&transaction, epoch, &fake_block_hash);

        assert_eq!(
            p.data_request_pool[&dr_pointer].stage,
            DataRequestStage::COMMIT
        );
        // Since there are no commitments to the data request, it should stay in commit stage
        assert!(p.update_data_request_stages().is_empty());

        assert_eq!(
            p.data_request_pool[&dr_pointer].stage,
            DataRequestStage::COMMIT
        );

        let commit = empty_commit_output();
        let commit_transaction = fake_transaction_zip(vec![(
            Input::DataRequest(DataRequestInput {
                transaction_id: dr_pointer.transaction_id,
                output_index: dr_pointer.output_index,
                poe: [77; 32],
            }),
            Output::Commit(commit),
        )]);

        let commit_pointer = OutputPointer {
            transaction_id: commit_transaction.hash(),
            output_index: 0,
        };

        let reveal = empty_reveal_output();
        let reveal_transaction = fake_transaction_zip(vec![(
            Input::Commit(CommitInput {
                transaction_id: commit_pointer.transaction_id,
                output_index: commit_pointer.output_index,
                reveal: vec![77],
                nonce: 444,
            }),
            Output::Reveal(reveal),
        )]);

        let reveal_pointer = OutputPointer {
            transaction_id: reveal_transaction.hash(),
            output_index: 0,
        };

        // Add reveal transaction for this commit, will be returned by the update_data_request_stages
        // function when the data request is in reveal stage
        p.add_own_reveal(dr_pointer.clone(), reveal_transaction.clone())
            .unwrap();

        assert_eq!(
            p.waiting_for_reveal.get(&dr_pointer),
            Some(&reveal_transaction)
        );

        p.process_transaction(&commit_transaction, epoch + 1, &fake_block_hash);

        // Now we can get the data request pointer from the commit output pointer
        assert_eq!(p.dr_pointer_cache.get(&commit_pointer), Some(&dr_pointer));

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

        // Send the reveal we got from the update function
        p.process_transaction(my_reveal, epoch + 2, &fake_block_hash);

        // Now we can get the data request pointer from the commit output pointer and the reveal pointer
        assert_eq!(p.dr_pointer_cache.get(&commit_pointer), Some(&dr_pointer));
        assert_eq!(p.dr_pointer_cache.get(&reveal_pointer), Some(&dr_pointer));

        // And we can also get all the commit/reveal pointers from the data request
        assert_eq!(
            p.data_request_pool[&dr_pointer]
                .info
                .commits
                .iter()
                .collect::<Vec<_>>(),
            vec![&commit_pointer],
        );
        assert_eq!(
            p.data_request_pool[&dr_pointer]
                .info
                .reveals
                .iter()
                .collect::<Vec<_>>(),
            vec![&reveal_pointer],
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
    }
    #[test]
    fn update_multiple_times() {
        // Only the first consecutive call to update_data_request_stages should change the state
        let fake_block_hash = Hash::SHA256([1; 32]);
        let epoch = 0;
        let data_request = empty_data_request();
        let transaction = fake_transaction_zip(vec![(
            Input::ValueTransfer(empty_value_transfer_input()),
            Output::DataRequest(data_request.clone()),
        )]);
        let dr_pointer = OutputPointer {
            transaction_id: transaction.hash(),
            output_index: 0,
        };

        let mut p = DataRequestPool::default();
        p.process_transaction(&transaction, epoch, &fake_block_hash);

        assert_eq!(
            p.data_request_pool[&dr_pointer].stage,
            DataRequestStage::COMMIT
        );
        // Since there are no commitments to the data request, it should stay in commit stage
        assert!(p.update_data_request_stages().is_empty());

        assert_eq!(
            p.data_request_pool[&dr_pointer].stage,
            DataRequestStage::COMMIT
        );

        assert!(p.update_data_request_stages().is_empty());

        assert_eq!(
            p.data_request_pool[&dr_pointer].stage,
            DataRequestStage::COMMIT
        );

        let commit = empty_commit_output();
        let commit_transaction = fake_transaction_zip(vec![(
            Input::DataRequest(DataRequestInput {
                transaction_id: dr_pointer.transaction_id,
                output_index: dr_pointer.output_index,
                poe: [77; 32],
            }),
            Output::Commit(commit),
        )]);

        let commit_pointer = OutputPointer {
            transaction_id: commit_transaction.hash(),
            output_index: 0,
        };

        p.process_transaction(&commit_transaction, epoch + 1, &fake_block_hash);

        // Now we can get the data request pointer from the commit output pointer
        assert_eq!(p.dr_pointer_cache.get(&commit_pointer), Some(&dr_pointer));

        // Still in commit stage until we update
        assert_eq!(
            p.data_request_pool[&dr_pointer].stage,
            DataRequestStage::COMMIT
        );

        // Update stages
        assert!(p.update_data_request_stages().is_empty());

        // Now in reveal stage
        assert_eq!(
            p.data_request_pool[&dr_pointer].stage,
            DataRequestStage::REVEAL
        );

        // Update stages
        assert!(p.update_data_request_stages().is_empty());

        // Now in reveal stage
        assert_eq!(
            p.data_request_pool[&dr_pointer].stage,
            DataRequestStage::REVEAL
        );

        let reveal = empty_reveal_output();
        let reveal_transaction = fake_transaction_zip(vec![(
            Input::Commit(CommitInput {
                transaction_id: commit_pointer.transaction_id,
                output_index: commit_pointer.output_index,
                reveal: vec![77],
                nonce: 444,
            }),
            Output::Reveal(reveal),
        )]);

        let reveal_pointer = OutputPointer {
            transaction_id: reveal_transaction.hash(),
            output_index: 0,
        };

        p.process_transaction(&reveal_transaction, epoch + 2, &fake_block_hash);

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

        // Update stages
        assert!(p.update_data_request_stages().is_empty());

        // Now in tally stage
        assert_eq!(
            p.data_request_pool[&dr_pointer].stage,
            DataRequestStage::TALLY
        );

        let tally = empty_tally_output();
        let mut tally_transaction = fake_transaction_zip(vec![(
            Input::Reveal(RevealInput {
                transaction_id: reveal_pointer.transaction_id,
                output_index: reveal_pointer.output_index,
            }),
            Output::ValueTransfer(empty_value_transfer_output()),
        )]);
        tally_transaction.outputs.push(Output::Tally(tally.clone()));

        // Now we can get the data request pointer from the commit/reveal pointers
        assert_eq!(p.dr_pointer_cache.get(&commit_pointer), Some(&dr_pointer));
        assert_eq!(p.dr_pointer_cache.get(&reveal_pointer), Some(&dr_pointer));

        // And we can also get all the commit/reveal pointers from the data request
        assert_eq!(
            p.data_request_pool[&dr_pointer]
                .info
                .commits
                .iter()
                .collect::<Vec<_>>(),
            vec![&commit_pointer],
        );
        assert_eq!(
            p.data_request_pool[&dr_pointer]
                .info
                .reveals
                .iter()
                .collect::<Vec<_>>(),
            vec![&reveal_pointer],
        );

        // There is nothing to be stored yet
        assert_eq!(p.to_be_stored.len(), 0);

        // Process tally: this will remove the data request from the pool
        p.process_transaction(&tally_transaction, epoch + 2, &fake_block_hash);

        // Now the cache has been cleared
        assert_eq!(p.dr_pointer_cache.get(&commit_pointer), None);
        assert_eq!(p.dr_pointer_cache.get(&reveal_pointer), None);

        // And the data request has been removed from the pool
        assert_eq!(p.data_request_pool.get(&dr_pointer), None);

        // Update stages
        assert!(p.update_data_request_stages().is_empty());

        assert_eq!(p.to_be_stored.len(), 1);
        assert_eq!(p.to_be_stored[0].0, dr_pointer);

        assert!(p.update_data_request_stages().is_empty());

        assert_eq!(p.to_be_stored.len(), 1);
        assert_eq!(p.to_be_stored[0].0, dr_pointer);
    }
}
