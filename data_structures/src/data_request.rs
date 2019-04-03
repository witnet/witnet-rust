use log::{debug, info};
use std::collections::{BTreeMap, HashMap, HashSet};

use witnet_crypto::hash::calculate_sha256;

use super::chain::{
    CommitInput, CommitOutput, DataRequestInput, DataRequestOutput, DataRequestReport,
    DataRequestStage, DataRequestState, Epoch, Hash, Hashable, Input, Output, OutputPointer,
    RevealInput, RevealOutput, TallyOutput, Transaction, TransactionBody, UnspentOutputsPool,
    ValueTransferOutput,
};

use serde::{Deserialize, Serialize};

type DataRequestsWithReveals = Vec<(
    (OutputPointer, DataRequestOutput),
    Vec<(OutputPointer, RevealOutput)>,
)>;

/// Pool of active data requests
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct DataRequestPool {
    /// Current active data request, in which this node has announced commitments.
    /// Key: Data Request Pointer, Value: Reveal Transaction
    pub waiting_for_reveal: HashMap<OutputPointer, Transaction>,
    /// List of active data request output pointers ordered by epoch (for mining purposes)
    pub data_requests_by_epoch: BTreeMap<Epoch, HashSet<OutputPointer>>,
    /// List of active data requests indexed by output pointer
    pub data_request_pool: HashMap<OutputPointer, DataRequestState>,
    /// List of data requests that should be persisted into storage
    pub to_be_stored: Vec<(OutputPointer, DataRequestReport)>,
    /// Cache which maps commit_pointer to data_request_pointer
    /// and reveal_pointer to data_request_pointer
    pub dr_pointer_cache: HashMap<OutputPointer, OutputPointer>,
}

impl DataRequestPool {
    /// Get all available data requests output pointers for an epoch
    pub fn get_dr_output_pointers_by_epoch(&self, epoch: Epoch) -> Vec<OutputPointer> {
        let range = 0..=epoch;
        self.data_requests_by_epoch
            .range(range)
            .flat_map(|(_epoch, hashset)| hashset.iter().cloned())
            .collect()
    }

    /// Get a `DataRequestOuput` for a `OutputPointer`
    pub fn get_dr_output(&self, output_pointer: &OutputPointer) -> Option<DataRequestOutput> {
        self.data_request_pool
            .get(output_pointer)
            .map(|dr_state| dr_state.data_request.clone())
    }

    /// Insert a reveal transaction into the pool
    pub fn insert_reveal(&mut self, data_request_pointer: OutputPointer, reveal: Transaction) {
        self.waiting_for_reveal.insert(data_request_pointer, reveal);
    }

    /// Get all the reveals
    pub fn get_all_reveals(&self, utxo: &UnspentOutputsPool) -> DataRequestsWithReveals {
        self.data_request_pool
            .iter()
            .filter_map(|(dr_pointer, dr_state)| {
                if let DataRequestStage::TALLY = dr_state.stage {
                    let reveals = dr_state
                        .info
                        .reveals
                        .iter()
                        .map(|reveal_pointer| {
                            (
                                reveal_pointer.clone(),
                                match utxo.get(reveal_pointer) {
                                    Some(Output::Reveal(reveal_output)) => reveal_output.clone(),
                                    _ => panic!("Reveal not in utxo"), // TODO: remove panic
                                },
                            )
                        })
                        .collect();
                    Some(((dr_pointer.clone(), dr_state.data_request.clone()), reveals))
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
        if reveal.body.inputs.len() == 1 && reveal.body.outputs.len() == 1 {
            // Input: CommitInput, Output: RevealOutput
            match (&reveal.body.inputs[0], &reveal.body.outputs[0]) {
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
    fn add_commit(&mut self, z: &Input, pointer: OutputPointer, block_hash: &Hash) {
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

    /// Add a reveal transaction
    fn add_reveal(&mut self, z: &Input, pointer: OutputPointer, block_hash: &Hash) {
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

    /// Add a tally transaction
    #[allow(clippy::needless_pass_by_value)]
    fn add_tally(&mut self, reveal: &RevealInput, pointer: OutputPointer, block_hash: &Hash) {
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
    fn resolve_data_request(
        data_request_pool: &mut HashMap<OutputPointer, DataRequestState>,
        dr_pointer: &OutputPointer,
        tally_pointer: OutputPointer,
    ) -> Result<(DataRequestOutput, DataRequestReport), ()> {
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
                        // When a data request changes from commit stage to reveal stage, it should
                        // be removed from the "data_requests_by_epoch" map, which stores the data
                        // requests potentially available for commitment
                        if let Some(hs) = data_requests_by_epoch.get_mut(&dr_state.epoch) {
                            let present = hs.remove(dr_pointer);
                            if hs.is_empty() {
                                data_requests_by_epoch.remove(&dr_state.epoch);
                            }
                            if !present {
                                // FIXME: This could be a warn! or a debug! instead of a panic
                                panic!(
                                    "Data request {:?} was not present in the \
                                     data_requests_by_epoch map (epoch #{})",
                                    dr_pointer, dr_state.epoch
                                );
                            }
                        }

                        if let Some(transaction) = waiting_for_reveal.remove(dr_pointer) {
                            // We submitted a commit for this data request!
                            // But has it been included into the block?
                            let commit_pointer = match &transaction.body.inputs[0] {
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
        for (i, (z, s)) in t.body.inputs.iter().zip(t.body.outputs.iter()).enumerate() {
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
        let possibly_tally =
            t.body.outputs.len() >= 2 && t.body.inputs.len() == t.body.outputs.len() - 1;

        if possibly_tally {
            let tally_index = t.body.outputs.len() - 1;
            match (
                &t.body.inputs[tally_index - 1],
                &t.body.outputs[tally_index],
            ) {
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
                        inputs = t.body.inputs,
                        outputs = t.body.outputs
                    );
                }
                _ => {
                    // Assume there is no tally in this transaction
                }
            }
        }
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
    pub fn finished_data_requests(&mut self) -> Vec<(OutputPointer, DataRequestReport)> {
        std::mem::replace(&mut self.to_be_stored, vec![])
    }
}

/// Function to calculate the commit reward
pub fn calculate_commit_reward(dr_output: &DataRequestOutput) -> u64 {
    dr_output.value / u64::from(dr_output.witnesses) - dr_output.commit_fee
}

/// Function to calculate the reveal reward
pub fn calculate_reveal_reward(dr_output: &DataRequestOutput) -> u64 {
    calculate_commit_reward(dr_output) - dr_output.reveal_fee
}

/// Function to calculate the value transfer reward
pub fn calculate_dr_vt_reward(dr_output: &DataRequestOutput) -> u64 {
    calculate_reveal_reward(dr_output) - dr_output.tally_fee
}

/// Function to calculate the tally change
pub fn calculate_tally_change(dr_output: &DataRequestOutput, n_reveals: u64) -> u64 {
    calculate_reveal_reward(dr_output) * (u64::from(dr_output.witnesses) - n_reveals)
}

/// Create data request commitment
pub fn create_commit_body(
    dr_output_pointer: &OutputPointer,
    dr_output: &DataRequestOutput,
    reveal: Vec<u8>,
) -> TransactionBody {
    // Create input
    let dr_input = Input::DataRequest(DataRequestInput {
        transaction_id: dr_output_pointer.transaction_id,
        output_index: dr_output_pointer.output_index,
        // TODO: create a proper poe
        poe: [0; 32],
    });

    // Calculate reveal_value
    let commit_value = calculate_commit_reward(&dr_output);

    let reveal_hash = calculate_sha256(reveal.as_slice()).into();

    // Create output
    let commit_output = Output::Commit(CommitOutput {
        commitment: reveal_hash,
        value: commit_value,
    });

    TransactionBody::new(0, vec![dr_input], vec![commit_output])
}

/// Create data request reveal
pub fn create_reveal_body(
    commit_pointer: OutputPointer,
    dr_output: &DataRequestOutput,
    reveal: Vec<u8>,
) -> TransactionBody {
    // Create input
    let commit_input = Input::Commit(CommitInput {
        transaction_id: commit_pointer.transaction_id,
        output_index: commit_pointer.output_index,
        nonce: 0,
    });

    // Calculate reveal_value
    let reveal_value = calculate_reveal_reward(&dr_output);

    // Create output
    let reveal_output = Output::Reveal(RevealOutput {
        reveal,
        // TODO: use a proper pkh
        pkh: [0; 20],
        value: reveal_value,
    });

    TransactionBody::new(0, vec![commit_input], vec![reveal_output])
}

pub fn create_vt_tally(
    dr_output: &DataRequestOutput,
    reveals: Vec<(OutputPointer, RevealOutput)>,
) -> (Vec<Input>, Vec<Output>, Vec<Vec<u8>>) {
    let mut inputs = vec![];
    let mut outputs = vec![];
    let mut results = vec![];
    // TODO: Do not reward dishonest witnesses
    let reveal_reward = calculate_dr_vt_reward(dr_output);

    for (reveal_pointer, reveal) in reveals {
        let reveal_input = RevealInput {
            transaction_id: reveal_pointer.transaction_id,
            output_index: reveal_pointer.output_index,
        };
        inputs.push(Input::Reveal(reveal_input));

        let vt_output = ValueTransferOutput {
            pkh: reveal.pkh,
            value: reveal_reward,
        };
        outputs.push(Output::ValueTransfer(vt_output));

        results.push(reveal.reveal);
    }

    (inputs, outputs, results)
}

pub fn create_tally_body(
    dr_output: &DataRequestOutput,
    inputs: Vec<Input>,
    mut outputs: Vec<Output>,
    consensus: Vec<u8>,
) -> TransactionBody {
    let change = calculate_tally_change(dr_output, inputs.len() as u64);
    let pkh = dr_output.pkh;

    let tally_output = TallyOutput {
        result: consensus,
        pkh,
        value: change,
    };
    outputs.push(Output::Tally(tally_output));
    TransactionBody::new(0, inputs, outputs)
}

#[cfg(test)]
mod tests {
    use crate::{chain::*, data_request::DataRequestPool};

    fn empty_data_request() -> DataRequestOutput {
        let data_request = RADRequest {
            not_before: 0,
            retrieve: vec![],
            aggregate: RADAggregate { script: vec![] },
            consensus: RADConsensus { script: vec![] },
            deliver: vec![],
        };

        DataRequestOutput {
            data_request,
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

        Transaction::new(
            TransactionBody::new(0, inputs, outputs),
            vec![KeyedSignature::default()],
        )
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
        assert!(!p
            .data_requests_by_epoch
            .get(&epoch)
            .map(|x| x.contains(&dr_pointer))
            .unwrap_or(false));
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
        tally_transaction
            .body
            .outputs
            .push(Output::Tally(tally.clone()));

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
        tally_transaction
            .body
            .outputs
            .push(Output::Tally(tally.clone()));

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
