use std::{
    cmp::Ordering,
    collections::{HashMap, HashSet},
    convert::{TryFrom, TryInto},
    fmt,
};

use itertools::Itertools;

use witnet_crypto::{
    hash::{calculate_sha256, Sha256},
    key::CryptoEngine,
    merkle::{merkle_tree_root as crypto_merkle_tree_root, ProgressiveMerkleTree},
    signature::{verify, PublicKey, Signature},
};
use witnet_data_structures::chain::ConsensusConstants;
use witnet_data_structures::{
    chain::{
        Block, BlockMerkleRoots, CheckpointBeacon, CheckpointVRF, DataRequestOutput,
        DataRequestStage, DataRequestState, Epoch, EpochConstants, Hash, Hashable, Input,
        KeyedSignature, OutputPointer, PublicKeyHash, RADRequest, RADTally, Reputation,
        ReputationEngine, SignaturesToVerify, UnspentOutputsPool, ValueTransferOutput,
    },
    data_request::{
        calculate_tally_change, calculate_witness_reward, create_tally, DataRequestPool,
    },
    error::{BlockError, DataRequestError, TransactionError},
    radon_error::RadonError,
    radon_report::{RadonReport, ReportContext, Stage, TallyMetaData},
    transaction::{
        CommitTransaction, DRTransaction, MintTransaction, RevealTransaction, TallyTransaction,
        Transaction, VTTransaction,
    },
    vrf::{BlockEligibilityClaim, DataRequestEligibilityClaim, VrfCtx},
};
use witnet_rad::{
    error::RadError,
    reducers::mode::mode,
    run_tally_report,
    script::{
        create_radon_script_from_filters_and_reducer, unpack_radon_script,
        RadonScriptExecutionSettings,
    },
    types::{array::RadonArray, serial_iter_decode, RadonType, RadonTypes},
};

/// Calculate the sum of the values of the outputs pointed by the
/// inputs of a transaction. If an input pointed-output is not
/// found in `pool`, then an error is returned instead indicating
/// it. If a Signature is invalid an error is returned too
pub fn transaction_inputs_sum(
    inputs: &[Input],
    utxo_diff: &UtxoDiff,
    epoch: Epoch,
    epoch_constants: EpochConstants,
) -> Result<u64, failure::Error> {
    let mut total_value: u64 = 0;
    let mut seen_output_pointers = HashSet::with_capacity(inputs.len());

    for input in inputs {
        let vt_output = utxo_diff.get(&input.output_pointer()).ok_or_else(|| {
            TransactionError::OutputNotFound {
                output: input.output_pointer().clone(),
            }
        })?;

        // Verify that commits are only accepted after the time lock expired
        let epoch_timestamp = epoch_constants.epoch_timestamp(epoch)?;
        let vt_time_lock = i64::try_from(vt_output.time_lock)?;
        if vt_time_lock > epoch_timestamp {
            return Err(TransactionError::TimeLock {
                expected: vt_time_lock,
                current: epoch_timestamp,
            }
            .into());
        } else {
            if !seen_output_pointers.insert(input.output_pointer()) {
                // If the set already contained this output pointer
                return Err(TransactionError::OutputNotFound {
                    output: input.output_pointer().clone(),
                }
                .into());
            }
            total_value = total_value
                .checked_add(vt_output.value)
                .ok_or(TransactionError::InputValueOverflow)?;
        }
    }

    Ok(total_value)
}

/// Calculate the sum of the values of the outputs of a transaction.
pub fn transaction_outputs_sum(outputs: &[ValueTransferOutput]) -> Result<u64, TransactionError> {
    let mut total_value: u64 = 0;
    for vt_output in outputs {
        total_value = total_value
            .checked_add(vt_output.value)
            .ok_or(TransactionError::OutputValueOverflow)?
    }

    Ok(total_value)
}

/// Returns the fee of a value transfer transaction.
///
/// The fee is the difference between the outputs and the inputs
/// of the transaction. The pool parameter is used to find the
/// outputs pointed by the inputs and that contain the actual
/// their value.
pub fn vt_transaction_fee(
    vt_tx: &VTTransaction,
    utxo_diff: &UtxoDiff,
    epoch: Epoch,
    epoch_constants: EpochConstants,
) -> Result<u64, failure::Error> {
    let in_value = transaction_inputs_sum(&vt_tx.body.inputs, utxo_diff, epoch, epoch_constants)?;
    let out_value = transaction_outputs_sum(&vt_tx.body.outputs)?;

    if out_value > in_value {
        Err(TransactionError::NegativeFee.into())
    } else {
        Ok(in_value - out_value)
    }
}

/// Returns the fee of a data request transaction.
///
/// The fee is the difference between the outputs (with the data request value)
/// and the inputs of the transaction. The pool parameter is used to find the
/// outputs pointed by the inputs and that contain the actual
/// their value.
pub fn dr_transaction_fee(
    dr_tx: &DRTransaction,
    utxo_diff: &UtxoDiff,
    epoch: Epoch,
    epoch_constants: EpochConstants,
) -> Result<u64, failure::Error> {
    let in_value = transaction_inputs_sum(&dr_tx.body.inputs, utxo_diff, epoch, epoch_constants)?;
    let out_value = transaction_outputs_sum(&dr_tx.body.outputs)?
        .checked_add(dr_tx.body.dr_output.checked_total_value()?)
        .ok_or(TransactionError::OutputValueOverflow)?;

    if out_value > in_value {
        Err(TransactionError::NegativeFee.into())
    } else {
        Ok(in_value - out_value)
    }
}

/// Returns the fee of a data request transaction.
///
/// The fee is the difference between the outputs (with the data request value)
/// and the inputs of the transaction. The pool parameter is used to find the
/// outputs pointed by the inputs and that contain the actual
/// their value.
pub fn validate_commit_collateral(
    co_tx: &CommitTransaction,
    utxo_diff: &UtxoDiff,
    epoch: Epoch,
    epoch_constants: EpochConstants,
    required_collateral: u64,
    block_number: u32,
    collateral_age: u32,
) -> Result<(), failure::Error> {
    let block_number_limit = block_number.saturating_sub(collateral_age);
    let commit_pkh = co_tx.body.proof.proof.pkh();
    let mut in_value: u64 = 0;
    let mut seen_output_pointers = HashSet::with_capacity(co_tx.body.collateral.len());

    for input in &co_tx.body.collateral {
        let vt_output = utxo_diff.get(&input.output_pointer()).ok_or_else(|| {
            TransactionError::OutputNotFound {
                output: input.output_pointer().clone(),
            }
        })?;

        // The inputs used as collateral do not need any additional signatures
        // as long as the commit transaction is signed by the same public key
        // So check that the public key matches
        if vt_output.pkh != commit_pkh {
            return Err(TransactionError::CollateralPkhMismatch {
                output: input.output_pointer().clone(),
                output_pkh: vt_output.pkh,
                proof_pkh: commit_pkh,
            }
            .into());
        }

        // Verify that commits are only accepted after the time lock expired
        let epoch_timestamp = epoch_constants.epoch_timestamp(epoch)?;
        let vt_time_lock = i64::try_from(vt_output.time_lock)?;
        if vt_time_lock > epoch_timestamp {
            return Err(TransactionError::TimeLock {
                expected: vt_time_lock,
                current: epoch_timestamp,
            }
            .into());
        }

        // Outputs to be spent in commitment inputs need to be older than `block_number_limit`.
        // All outputs from the genesis block are fulfill this requirement because
        // `block_number_limit` can't go lower than `0`.
        let included_in_block_number = utxo_diff
            .included_in_block_number(input.output_pointer())
            .unwrap();
        if included_in_block_number > block_number_limit {
            return Err(TransactionError::CollateralNotMature {
                output: input.output_pointer().clone(),
                must_be_older_than: collateral_age,
                found: block_number - included_in_block_number,
            }
            .into());
        }

        if !seen_output_pointers.insert(input.output_pointer()) {
            // If the set already contained this output pointer
            return Err(TransactionError::OutputNotFound {
                output: input.output_pointer().clone(),
            }
            .into());
        }

        in_value = in_value
            .checked_add(vt_output.value)
            .ok_or(TransactionError::InputValueOverflow)?;
    }

    let out_value = transaction_outputs_sum(&co_tx.body.outputs)?;

    if out_value > in_value {
        Err(TransactionError::NegativeCollateral {
            input_value: in_value,
            output_value: out_value,
        }
        .into())
    } else if in_value - out_value != required_collateral {
        let found_collateral = in_value - out_value;
        Err(TransactionError::IncorrectCollateral {
            expected: required_collateral,
            found: found_collateral,
        }
        .into())
    } else {
        Ok(())
    }
}

/// Function to validate a mint transaction
pub fn validate_mint_transaction(
    mint_tx: &MintTransaction,
    total_fees: u64,
    block_epoch: Epoch,
) -> Result<(), failure::Error> {
    // Mint epoch must be equal to block epoch
    if mint_tx.epoch != block_epoch {
        return Err(BlockError::InvalidMintEpoch {
            mint_epoch: mint_tx.epoch,
            block_epoch,
        }
        .into());
    }

    let mint_value = transaction_outputs_sum(&mint_tx.outputs)?;
    let block_reward_value = block_reward(mint_tx.epoch);
    // Mint value must be equal to block_reward + transaction fees
    if mint_value != total_fees + block_reward_value {
        return Err(BlockError::MismatchedMintValue {
            mint_value,
            fees_value: total_fees,
            reward_value: block_reward_value,
        }
        .into());
    }

    if mint_tx.outputs.len() > 2 {
        return Err(BlockError::TooSplitMint.into());
    }

    for (idx, output) in mint_tx.outputs.iter().enumerate() {
        if output.value == 0 {
            return Err(TransactionError::ZeroValueOutput {
                tx_hash: mint_tx.hash(),
                output_id: idx,
            }
            .into());
        }
    }

    Ok(())
}

/// Function to validate a rad request
pub fn validate_rad_request(rad_request: &RADRequest) -> Result<(), failure::Error> {
    let retrieval_paths = &rad_request.retrieve;
    for path in retrieval_paths {
        unpack_radon_script(path.script.as_slice())?;
    }

    let aggregate = &rad_request.aggregate;
    let filters = aggregate.filters.as_slice();
    let reducer = aggregate.reducer;
    create_radon_script_from_filters_and_reducer(filters, reducer)?;

    let consensus = &rad_request.tally;
    let filters = consensus.filters.as_slice();
    let reducer = consensus.reducer;
    create_radon_script_from_filters_and_reducer(filters, reducer)?;

    Ok(())
}

/// An histogram-like counter that helps counting occurrences of different numeric categories.
struct Counter {
    /// Tracks the position inside `values` of the category that appears the most.
    /// This MUST be initialized to `None`.
    /// As long as `values` is not empty, `None` means there was a tie between multiple categories.
    max_pos: Option<usize>,
    /// Tracks how many times does the most frequent category appear.
    /// This is a cached version of `self.values[self.max_pos]`.
    max_val: i32,
    /// Tracks how many times does each different category appear.
    categories: Vec<i32>,
}

/// Implementations for `struct Counter`
impl Counter {
    /// Increment by one the counter for a particular category.
    fn increment(&mut self, category_id: usize) {
        // Increment the counter by 1.
        self.categories[category_id] += 1;

        // Tell whether `max_pos` and `max_val` need to be updated.
        match self.categories[category_id].cmp(&self.max_val) {
            // If the recently updated counter is less than `max_pos`, do nothing.
            Ordering::Less => {}
            // If the recently updated counter is equal than `max_pos`, it is a tie.
            Ordering::Equal => {
                self.max_pos = None;
            }
            // If the recently updated counter outgrows `max_pos`, update `max_val` and `max_pos`.
            Ordering::Greater => {
                self.max_val = self.categories[category_id];
                self.max_pos = Some(category_id);
            }
        }
    }

    /// Create a new `struct Counter` that is initialized to truck a provided number of categories.
    fn new(n: usize) -> Self {
        let categories = vec![0; n];

        Self {
            max_pos: None,
            max_val: 0,
            categories,
        }
    }
}

fn update_liars(liars: &mut Vec<bool>, item: RadonTypes, condition: bool) -> Option<RadonTypes> {
    liars.push(!condition);
    if condition {
        Some(item)
    } else {
        None
    }
}

/// An `Either`-like structure that covers the two possible return types of the
/// `evaluate_tally_precondition_clause` method.
#[derive(Debug)]
pub enum TallyPreconditionClauseResult {
    MajorityOfValues {
        values: Vec<RadonTypes>,
        liars: Vec<bool>,
        errors: Vec<bool>,
    },
    MajorityOfErrors {
        errors_mode: RadonError<RadError>,
    },
}

/// Run a precondition clause on an array of `RadonTypes` so as to check if the mode is a value or
/// an error, which has clear consequences in regards to consensus, rewards and punishments.
// FIXME: Allow for now, since there is no safe cast function from a usize to float yet
#[allow(clippy::cast_precision_loss)]
pub fn evaluate_tally_precondition_clause(
    reveals: Vec<RadonReport<RadonTypes>>,
    minimum_consensus: f64,
    num_commits: usize,
) -> Result<TallyPreconditionClauseResult, RadError> {
    // Short-circuit if there were no commits
    if num_commits == 0 {
        return Err(RadError::InsufficientCommits);
    }
    // Short-circuit if there were no reveals
    if reveals.is_empty() {
        return Err(RadError::NoReveals);
    }

    let error_type_discriminant =
        RadonTypes::RadonError(RadonError::try_from(RadError::default()).unwrap()).discriminant();

    // Count how many times is each RADON type featured in `reveals`, but count `RadonError` items
    // separately as they need to be handled differently.
    let reveals_len = u32::try_from(reveals.len()).unwrap();
    let mut counter = Counter::new(RadonTypes::num_types());
    let mut errors = vec![];
    for reveal in &reveals {
        counter.increment(reveal.result.discriminant());
        if reveal.result.discriminant() == error_type_discriminant {
            errors.push(true);
        } else {
            errors.push(false);
        }
    }

    // Compute ratio of type consensus amongst reveals (percentage of reveals that have same type
    // as the frequent type).
    let achieved_consensus = f64::from(counter.max_val) / f64::from(reveals_len);

    // If the achieved consensus is over the user-defined threshold, continue.
    // Otherwise, return `RadError::InsufficientConsensus`.
    if achieved_consensus >= minimum_consensus {
        // Decide based on the most frequent type.
        match counter.max_pos {
            // Handle tie cases (there is the same amount of revealed values for multiple types).
            None => Err(RadError::ModeTie {
                values: RadonArray::from(
                    reveals
                        .into_iter()
                        .map(RadonReport::into_inner)
                        .collect::<Vec<RadonTypes>>(),
                ),
                max_count: u16::try_from(counter.max_val).unwrap(),
            }),
            // Majority of errors, return errors mode.
            Some(most_frequent_type) if most_frequent_type == error_type_discriminant => {
                let errors: Vec<RadonTypes> = reveals
                    .into_iter()
                    .filter_map(|reveal| match reveal.into_inner() {
                        radon_types @ RadonTypes::RadonError(_) => Some(radon_types),
                        _ => None,
                    })
                    .collect();

                let errors_array = RadonArray::from(errors);
                // Use the mode filter to get the count of the most common error.
                // That count must be greater than or equal to minimum_consensus,
                // otherwise RadError::InsufficientConsensus is returned
                let most_common_error_array = witnet_rad::filters::mode::mode_filter(
                    &errors_array,
                    &mut ReportContext::default(),
                );

                match most_common_error_array {
                    Ok(RadonTypes::Array(x)) => {
                        let x_value = x.value();
                        let achieved_consensus = x_value.len() as f64 / f64::from(reveals_len);
                        if achieved_consensus >= minimum_consensus {
                            match mode(&errors_array)? {
                                RadonTypes::RadonError(errors_mode) => {
                                    Ok(TallyPreconditionClauseResult::MajorityOfErrors { errors_mode })
                                }
                                _ => unreachable!("Mode of `RadonArray` containing only `RadonError`s cannot possibly be different from `RadonError`"),
                            }
                        } else {
                            Err(RadError::InsufficientConsensus {
                                achieved: achieved_consensus,
                                required: minimum_consensus,
                            })
                        }
                    }
                    Ok(_) => {
                        unreachable!("Mode filter should always return a `RadonArray`");
                    }
                    Err(RadError::ModeTie { values, max_count }) => {
                        let achieved_consensus = f64::from(max_count) / f64::from(reveals_len);
                        if achieved_consensus < minimum_consensus {
                            Err(RadError::InsufficientConsensus {
                                achieved: achieved_consensus,
                                required: minimum_consensus,
                            })
                        } else {
                            // This is only possible if minimum_consensus <= 0.50
                            Err(RadError::ModeTie { values, max_count })
                        }
                    }
                    Err(e) => panic!(
                        "Unexpected error when applying filter_mode on array of errors: {}",
                        e
                    ),
                }
            }
            // Majority of values, compute and filter liars
            Some(most_frequent_type) => {
                let mut liars = vec![];
                let results = reveals
                    .into_iter()
                    .filter_map(|reveal| {
                        let radon_types = reveal.into_inner();
                        let condition = most_frequent_type == radon_types.discriminant();
                        update_liars(&mut liars, radon_types, condition)
                    })
                    .collect();

                Ok(TallyPreconditionClauseResult::MajorityOfValues {
                    values: results,
                    liars,
                    errors,
                })
            }
        }
    } else {
        Err(RadError::InsufficientConsensus {
            achieved: achieved_consensus,
            required: minimum_consensus,
        })
    }
}

/// Construct a `RadonReport` from a `TallyPreconditionClauseResult`
pub fn construct_report_from_clause_result(
    clause_result: Result<TallyPreconditionClauseResult, RadError>,
    script: &RADTally,
    reports_len: usize,
) -> RadonReport<RadonTypes> {
    // This TallyMetadata would be included in case of Error Result, in that case,
    // no one has to be classified as a lier, but everyone as an error
    let mut metadata = TallyMetaData::default();
    metadata.update_liars(vec![false; reports_len]);
    metadata.errors = vec![true; reports_len];
    match clause_result {
        // The reveals passed the precondition clause (a parametric majority of them were successful
        // values). Run the tally, which will add more liars if any.
        Ok(TallyPreconditionClauseResult::MajorityOfValues {
            values,
            liars,
            errors,
        }) => {
            let mut metadata = TallyMetaData::default();
            metadata.update_liars(vec![false; reports_len]);

            match run_tally_report(
                values,
                script,
                Some(liars),
                Some(errors),
                RadonScriptExecutionSettings::all_but_partial_results(),
            ) {
                Ok(x) => x,
                Err(e) => RadonReport::from_result(
                    Err(RadError::TallyExecution {
                        inner: Some(Box::new(e)),
                        message: None,
                    }),
                    &ReportContext::from_stage(Stage::Tally(metadata)),
                ),
            }
        }
        // The reveals did not pass the precondition clause (a parametric majority of them were
        // errors). Tally will not be run, and the mode of the errors will be committed.
        Ok(TallyPreconditionClauseResult::MajorityOfErrors { errors_mode }) => {
            // Do not impose penalties on any of the revealers.
            RadonReport::from_result(
                Ok(RadonTypes::RadonError(errors_mode)),
                &ReportContext::from_stage(Stage::Tally(metadata)),
            )
        }
        // Failed to evaluate the precondition clause. `RadonReport::from_result()?` is the last
        // chance for errors to be intercepted and used for consensus.
        Err(e) => {
            RadonReport::from_result(Err(e), &ReportContext::from_stage(Stage::Tally(metadata)))
        }
    }
}

/// Function to validate a value transfer transaction
pub fn validate_vt_transaction<'a>(
    vt_tx: &'a VTTransaction,
    utxo_diff: &UtxoDiff,
    epoch: Epoch,
    epoch_constants: EpochConstants,
    signatures_to_verify: &mut Vec<SignaturesToVerify>,
    max_vt_weight: u32,
) -> Result<(Vec<&'a Input>, Vec<&'a ValueTransferOutput>, u64), failure::Error> {
    if vt_tx.weight() > max_vt_weight {
        return Err(TransactionError::ValueTransferWeightLimitExceeded {
            weight: vt_tx.weight(),
            max_weight: max_vt_weight,
        }
        .into());
    }

    validate_transaction_signature(
        &vt_tx.signatures,
        &vt_tx.body.inputs,
        vt_tx.hash(),
        utxo_diff,
        signatures_to_verify,
    )?;

    // A value transfer transaction must have at least one input
    if vt_tx.body.inputs.is_empty() {
        return Err(TransactionError::NoInputs {
            tx_hash: vt_tx.hash(),
        }
        .into());
    }

    // A value transfer output cannot have zero value
    for (idx, output) in vt_tx.body.outputs.iter().enumerate() {
        if output.value == 0 {
            return Err(TransactionError::ZeroValueOutput {
                tx_hash: vt_tx.hash(),
                output_id: idx,
            }
            .into());
        }
    }

    let fee = vt_transaction_fee(vt_tx, utxo_diff, epoch, epoch_constants)?;

    // FIXME(#514): Implement value transfer transaction validation

    Ok((
        vt_tx.body.inputs.iter().collect(),
        vt_tx.body.outputs.iter().collect(),
        fee,
    ))
}

/// Function to validate a value transfer transaction from the genesis block
/// These are special because they can create value
pub fn validate_genesis_vt_transaction(
    vt_tx: &VTTransaction,
) -> Result<(Vec<&ValueTransferOutput>, u64), TransactionError> {
    // Genesis VTTs should have 0 inputs
    if !vt_tx.body.inputs.is_empty() {
        return Err(TransactionError::InputsInGenesis {
            inputs_n: vt_tx.body.inputs.len(),
        });
    }
    // Genesis VTTs should have 0 signatures
    if !vt_tx.signatures.is_empty() {
        return Err(TransactionError::MismatchingSignaturesNumber {
            signatures_n: u8::try_from(vt_tx.signatures.len()).unwrap(),
            inputs_n: 0,
        });
    }
    // Genesis VTTs must have at least one output
    if vt_tx.body.outputs.is_empty() {
        return Err(TransactionError::NoOutputsInGenesis);
    }
    for (idx, output) in vt_tx.body.outputs.iter().enumerate() {
        // Genesis VTT outputs must have some value
        if output.value == 0 {
            return Err(TransactionError::ZeroValueOutput {
                tx_hash: vt_tx.hash(),
                output_id: idx,
            });
        }
    }

    let outputs = vt_tx.body.outputs.iter().collect();
    let value_created = transaction_outputs_sum(&vt_tx.body.outputs)?;

    Ok((outputs, value_created))
}

/// Function to validate a data request transaction
pub fn validate_dr_transaction<'a>(
    dr_tx: &'a DRTransaction,
    utxo_diff: &UtxoDiff,
    epoch: Epoch,
    epoch_constants: EpochConstants,
    signatures_to_verify: &mut Vec<SignaturesToVerify>,
    collateral_minimum: u64,
    max_dr_weight: u32,
) -> Result<(Vec<&'a Input>, Vec<&'a ValueTransferOutput>, u64), failure::Error> {
    if dr_tx.weight() > max_dr_weight {
        return Err(TransactionError::DataRequestWeightLimitExceeded {
            weight: dr_tx.weight(),
            max_weight: max_dr_weight,
        }
        .into());
    }

    validate_transaction_signature(
        &dr_tx.signatures,
        &dr_tx.body.inputs,
        dr_tx.hash(),
        utxo_diff,
        signatures_to_verify,
    )?;

    // A value transfer output cannot have zero value
    for (idx, output) in dr_tx.body.outputs.iter().enumerate() {
        if output.value == 0 {
            return Err(TransactionError::ZeroValueOutput {
                tx_hash: dr_tx.hash(),
                output_id: idx,
            }
            .into());
        }
    }

    let fee = dr_transaction_fee(dr_tx, utxo_diff, epoch, epoch_constants)?;

    validate_data_request_output(&dr_tx.body.dr_output)?;

    // Collateral value validation
    // If collateral is equal to 0 means that is equal to collateral_minimum value
    if (dr_tx.body.dr_output.collateral != 0)
        && (dr_tx.body.dr_output.collateral < collateral_minimum)
    {
        return Err(TransactionError::InvalidCollateral {
            value: dr_tx.body.dr_output.collateral,
            min: collateral_minimum,
        }
        .into());
    }

    validate_rad_request(&dr_tx.body.dr_output.data_request)?;

    Ok((
        dr_tx.body.inputs.iter().collect(),
        dr_tx.body.outputs.iter().collect(),
        fee,
    ))
}

/// Function to validate a data request output.
///
/// A data request output is valid under the following conditions:
/// - The number of witnesses is at least 1
/// - The witness reward is at least 1
/// - The min_consensus_percentage is >50 and <100
pub fn validate_data_request_output(request: &DataRequestOutput) -> Result<(), TransactionError> {
    if request.witnesses < 1 {
        return Err(TransactionError::InsufficientWitnesses);
    }

    if request.witness_reward < 1 {
        return Err(TransactionError::NoReward);
    }

    if !((51..100).contains(&request.min_consensus_percentage)) {
        return Err(TransactionError::InvalidMinConsensus {
            value: request.min_consensus_percentage,
        });
    }

    // Data request fees are checked in validate_dr_transaction
    Ok(())
}

/// Function to validate a commit transaction
#[allow(clippy::too_many_arguments)]
pub fn validate_commit_transaction(
    co_tx: &CommitTransaction,
    dr_pool: &DataRequestPool,
    vrf_input: CheckpointVRF,
    signatures_to_verify: &mut Vec<SignaturesToVerify>,
    rep_eng: &ReputationEngine,
    epoch: Epoch,
    epoch_constants: EpochConstants,
    utxo_diff: &UtxoDiff,
    collateral_minimum: u64,
    collateral_age: u32,
    block_number: u32,
) -> Result<(Hash, u16, u64), failure::Error> {
    // Get DataRequest information
    let dr_pointer = co_tx.body.dr_pointer;
    let dr_state = dr_pool
        .data_request_state(&dr_pointer)
        .ok_or(TransactionError::DataRequestNotFound { hash: dr_pointer })?;
    if dr_state.stage != DataRequestStage::COMMIT {
        return Err(DataRequestError::NotCommitStage.into());
    }

    let dr_output = &dr_state.data_request;

    // Commitment's output is only for change propose, so it only has to be one output and the
    // address has to be the same than the address which creates the commitment
    let proof_pkh = co_tx.body.proof.proof.pkh();
    if co_tx.body.outputs.len() > 1 {
        return Err(TransactionError::SeveralCommitOutputs.into());
    }
    if let Some(output) = &co_tx.body.outputs.first() {
        if output.value == 0 {
            return Err(TransactionError::ZeroValueOutput {
                tx_hash: co_tx.hash(),
                output_id: 0,
            }
            .into());
        }
        if output.pkh != proof_pkh {
            return Err(TransactionError::PublicKeyHashMismatch {
                expected_pkh: proof_pkh,
                signature_pkh: output.pkh,
            }
            .into());
        }
    }

    // Check that collateral has correct amount and age
    let mut required_collateral = dr_output.collateral;
    if required_collateral == 0 {
        required_collateral = collateral_minimum;
    }
    validate_commit_collateral(
        co_tx,
        utxo_diff,
        epoch,
        epoch_constants,
        required_collateral,
        block_number,
        collateral_age,
    )?;

    // Verify that commits are only accepted after the time lock expired
    let epoch_timestamp = epoch_constants.epoch_timestamp(epoch)?;
    let dr_time_lock = i64::try_from(dr_output.data_request.time_lock)?;
    if dr_time_lock > epoch_timestamp {
        return Err(TransactionError::TimeLock {
            expected: dr_time_lock,
            current: epoch_timestamp,
        }
        .into());
    }

    let commit_signature =
        validate_commit_reveal_signature(co_tx.hash(), &co_tx.signatures, signatures_to_verify)?;

    let sign_pkh = commit_signature.public_key.pkh();
    if proof_pkh != sign_pkh {
        return Err(TransactionError::PublicKeyHashMismatch {
            expected_pkh: proof_pkh,
            signature_pkh: sign_pkh,
        }
        .into());
    }

    let pkh = proof_pkh;
    let backup_witnesses = dr_state.backup_witnesses();
    let num_witnesses = dr_output.witnesses + backup_witnesses;
    let (target_hash, _) = calculate_reppoe_threshold(rep_eng, &pkh, num_witnesses);
    add_dr_vrf_signature_to_verify(
        signatures_to_verify,
        &co_tx.body.proof,
        vrf_input,
        co_tx.body.dr_pointer,
        target_hash,
    );

    // The commit fee here is the fee to include one commit
    Ok((dr_pointer, dr_output.witnesses, dr_output.commit_fee))
}

/// Function to validate a reveal transaction
pub fn validate_reveal_transaction(
    re_tx: &RevealTransaction,
    dr_pool: &DataRequestPool,
    signatures_to_verify: &mut Vec<SignaturesToVerify>,
) -> Result<u64, failure::Error> {
    // Get DataRequest information
    let dr_pointer = re_tx.body.dr_pointer;
    let dr_state = dr_pool
        .data_request_state(&dr_pointer)
        .ok_or(TransactionError::DataRequestNotFound { hash: dr_pointer })?;

    if dr_state.stage != DataRequestStage::REVEAL {
        return Err(DataRequestError::NotRevealStage.into());
    }

    let reveal_signature =
        validate_commit_reveal_signature(re_tx.hash(), &re_tx.signatures, signatures_to_verify)?;
    let pkh = reveal_signature.public_key.pkh();
    let pkh2 = re_tx.body.pkh;
    if pkh != pkh2 {
        return Err(TransactionError::PublicKeyHashMismatch {
            expected_pkh: pkh2,
            signature_pkh: pkh,
        }
        .into());
    }

    if dr_state.info.reveals.contains_key(&pkh) {
        return Err(TransactionError::DuplicatedReveal { pkh, dr_pointer }.into());
    }

    let commit = dr_state
        .info
        .commits
        .get(&pkh)
        .ok_or_else(|| TransactionError::CommitNotFound)?;

    if commit.body.commitment != reveal_signature.signature.hash() {
        return Err(TransactionError::MismatchedCommitment.into());
    }

    // The reveal fee here is the fee to include one reveal
    Ok(dr_state.data_request.reveal_fee)
}

fn create_expected_report(
    reveals: &[&RevealTransaction],
    tally: &RADTally,
    non_error_min: f64,
    commits_count: usize,
) -> RadonReport<RadonTypes> {
    let results = serial_iter_decode(
        &mut reveals
            .iter()
            .map(|&reveal_tx| (reveal_tx.body.reveal.as_slice(), reveal_tx)),
        |e: RadError, slice: &[u8], reveal_tx: &RevealTransaction| {
            log::warn!(
                "Could not decode reveal from {:?} (revealed bytes were `{:?}`): {:?}",
                reveal_tx,
                &slice,
                e
            );
            Some(RadonReport::from_result(
                Err(RadError::MalformedReveal),
                &ReportContext::default(),
            ))
        },
    );

    let results_len = results.len();
    let clause_result = evaluate_tally_precondition_clause(results, non_error_min, commits_count);
    construct_report_from_clause_result(clause_result, &tally, results_len)
}

fn create_expected_tally_transaction(
    ta_tx: &TallyTransaction,
    dr_pool: &DataRequestPool,
    collateral_minimum: u64,
) -> Result<(TallyTransaction, DataRequestState), failure::Error> {
    // Get DataRequestState
    let dr_pointer = ta_tx.dr_pointer;
    let dr_state = dr_pool
        .data_request_state(&dr_pointer)
        .ok_or(TransactionError::DataRequestNotFound { hash: dr_pointer })?;

    if dr_state.stage != DataRequestStage::TALLY {
        return Err(DataRequestError::NotTallyStage.into());
    }

    let dr_output = &dr_state.data_request;

    // The unwrap is safe because we know that the data request exists
    let reveal_txns = dr_pool.get_reveals(&dr_pointer).unwrap();
    let non_error_min = f64::from(dr_output.min_consensus_percentage) / 100.0;
    let committers = dr_state
        .info
        .commits
        .keys()
        .cloned()
        .collect::<HashSet<PublicKeyHash>>();
    let commit_length = committers.len();

    let report = create_expected_report(
        &reveal_txns,
        &dr_output.data_request.tally,
        non_error_min,
        commit_length,
    );

    let ta_tx = create_tally(
        dr_pointer,
        &dr_output,
        dr_state.pkh,
        &report,
        reveal_txns.into_iter().map(|tx| tx.body.pkh).collect(),
        committers,
        collateral_minimum,
    )?;

    Ok((ta_tx, dr_state.clone()))
}

pub fn calculate_liars_and_errors_count_from_tally(tally_tx: &TallyTransaction) -> (usize, usize) {
    tally_tx
        .out_of_consensus
        .iter()
        .fold((0, 0), |(l_count, e_count), x| {
            if tally_tx.error_committers.contains(x) {
                (l_count, e_count + 1)
            } else {
                (l_count + 1, e_count)
            }
        })
}

/// Function to validate a tally transaction
pub fn validate_tally_transaction<'a>(
    ta_tx: &'a TallyTransaction,
    dr_pool: &DataRequestPool,
    collateral_minimum: u64,
) -> Result<(Vec<&'a ValueTransferOutput>, u64), failure::Error> {
    let (expected_ta_tx, dr_state) =
        create_expected_tally_transaction(ta_tx, dr_pool, collateral_minimum)?;

    let sorted_out_of_consensus = ta_tx.out_of_consensus.iter().cloned().sorted().collect();
    let sorted_expected_out_of_consensus = expected_ta_tx
        .out_of_consensus
        .into_iter()
        .sorted()
        .collect();

    // Validation of slashed witnesses
    if sorted_expected_out_of_consensus != sorted_out_of_consensus {
        return Err(TransactionError::MismatchingOutOfConsensusCount {
            expected: sorted_expected_out_of_consensus,
            found: sorted_out_of_consensus,
        }
        .into());
    }

    let sorted_error = ta_tx.error_committers.iter().cloned().sorted().collect();
    let sorted_expected_error = expected_ta_tx
        .error_committers
        .into_iter()
        .sorted()
        .collect();

    // Validation of error witnesses
    if sorted_expected_error != sorted_error {
        return Err(TransactionError::MismatchingErrorCount {
            expected: sorted_expected_error,
            found: sorted_error,
        }
        .into());
    }

    // Validation of outputs number
    if expected_ta_tx.outputs.len() != ta_tx.outputs.len() {
        return Err(TransactionError::WrongNumberOutputs {
            expected_outputs: expected_ta_tx.outputs.len(),
            outputs: ta_tx.outputs.len(),
        }
        .into());
    }

    // Validation of tally result
    if expected_ta_tx.tally != ta_tx.tally {
        return Err(TransactionError::MismatchedConsensus {
            expected_tally: expected_ta_tx.tally,
            miner_tally: ta_tx.tally.clone(),
        }
        .into());
    }

    let commits_count = dr_state.info.commits.len();
    let reveals_count = dr_state.info.reveals.len();
    let honests_count = commits_count - ta_tx.out_of_consensus.len();

    let (liars_count, errors_count) = calculate_liars_and_errors_count_from_tally(ta_tx);

    let expected_tally_change = calculate_tally_change(
        commits_count,
        reveals_count,
        honests_count,
        &dr_state.data_request,
    );

    let collateral = if dr_state.data_request.collateral == 0 {
        collateral_minimum
    } else {
        dr_state.data_request.collateral
    };

    let mut pkh_rewarded: HashSet<PublicKeyHash> = HashSet::default();
    let mut total_tally_value = 0;
    let (reward, tally_extra_fee) = calculate_witness_reward(
        commits_count,
        reveals_count,
        liars_count,
        errors_count,
        dr_state.data_request.witness_reward,
        collateral,
    );
    for (i, output) in ta_tx.outputs.iter().enumerate() {
        // Validation of tally change value
        if expected_tally_change > 0 && i == ta_tx.outputs.len() - 1 && output.pkh == dr_state.pkh {
            if output.value != expected_tally_change {
                return Err(TransactionError::InvalidTallyChange {
                    change: output.value,
                    expected_change: expected_tally_change,
                }
                .into());
            }
        } else {
            if reveals_count > 0 {
                // Make sure every rewarded address is a revealer
                if dr_state.info.reveals.get(&output.pkh).is_none() {
                    return Err(TransactionError::RevealNotFound.into());
                }
                // Make sure every rewarded address passed the tally function, a.k.a. "is honest" / "is not a liar"
                if sorted_out_of_consensus.contains(&output.pkh)
                    && !sorted_error.contains(&output.pkh)
                {
                    return Err(TransactionError::DishonestReward.into());
                }
            }

            // Validation out of consensus error
            if sorted_out_of_consensus.contains(&output.pkh)
                && sorted_error.contains(&output.pkh)
                && output.value != collateral
            {
                return Err(TransactionError::InvalidReward {
                    value: output.value,
                    expected_value: collateral,
                }
                .into());
            }
            // Validation of the reward is according to the DataRequestOutput
            if !sorted_out_of_consensus.contains(&output.pkh) && output.value != reward {
                return Err(TransactionError::InvalidReward {
                    value: output.value,
                    expected_value: reward,
                }
                .into());
            }

            // Validation of a honest witness would not be rewarded more than once
            if pkh_rewarded.contains(&output.pkh) {
                return Err(TransactionError::MultipleRewards { pkh: output.pkh }.into());
            }
            pkh_rewarded.insert(output.pkh);
        }
        total_tally_value += output.value;
    }

    let expected_collateral_value = if commits_count > 0 {
        (if dr_state.data_request.collateral == 0 {
            collateral_minimum
        } else {
            dr_state.data_request.collateral
        }) * u64::from(dr_state.data_request.witnesses)
    } else {
        // In case of no commits, collateral does not affect
        0
    };
    let expected_dr_value = dr_state.data_request.checked_total_value()?;
    let found_dr_value = dr_state.info.commits.len() as u64 * dr_state.data_request.commit_fee
        + dr_state.info.reveals.len() as u64 * dr_state.data_request.reveal_fee
        + dr_state.data_request.tally_fee
        + tally_extra_fee
        + total_tally_value;

    // Validation of the total value of the data request
    if found_dr_value != expected_dr_value + expected_collateral_value {
        return Err(TransactionError::InvalidTallyValue {
            value: found_dr_value,
            expected_value: expected_dr_value + expected_collateral_value,
        }
        .into());
    }

    Ok((
        ta_tx.outputs.iter().collect(),
        dr_state.data_request.tally_fee + tally_extra_fee,
    ))
}

/// Function to validate a block signature
pub fn validate_block_signature(
    block: &Block,
    signatures_to_verify: &mut Vec<SignaturesToVerify>,
) -> Result<(), failure::Error> {
    let proof_pkh = block.block_header.proof.proof.pkh();
    let signature_pkh = block.block_sig.public_key.pkh();
    if proof_pkh != signature_pkh {
        return Err(BlockError::PublicKeyHashMismatch {
            proof_pkh,
            signature_pkh,
        }
        .into());
    }

    let keyed_signature = &block.block_sig;

    let signature = keyed_signature.signature.clone().try_into()?;
    let public_key = keyed_signature.public_key.clone().try_into()?;

    let Hash::SHA256(message) = block.hash();

    add_secp_block_signature_to_verify(signatures_to_verify, &public_key, &message, &signature);

    Ok(())
}

/// Function to validate a pkh signature
pub fn validate_pkh_signature(
    input: &Input,
    keyed_signature: &KeyedSignature,
    utxo_diff: &UtxoDiff,
) -> Result<(), failure::Error> {
    let output = utxo_diff.get(&input.output_pointer());
    if let Some(x) = output {
        let signature_pkh = PublicKeyHash::from_public_key(&keyed_signature.public_key);
        let expected_pkh = x.pkh;
        if signature_pkh != expected_pkh {
            return Err(TransactionError::PublicKeyHashMismatch {
                expected_pkh,
                signature_pkh,
            }
            .into());
        }
    }
    Ok(())
}

/// Add secp tx signatures to verification list
pub fn add_secp_tx_signature_to_verify(
    signatures_to_verify: &mut Vec<SignaturesToVerify>,
    public_key: &PublicKey,
    data: &[u8],
    sig: &Signature,
) {
    signatures_to_verify.push(SignaturesToVerify::SecpTx {
        public_key: *public_key,
        data: data.to_vec(),
        signature: *sig,
    });
}

/// Add secp tx signatures to verification list
pub fn add_secp_block_signature_to_verify(
    signatures_to_verify: &mut Vec<SignaturesToVerify>,
    public_key: &PublicKey,
    data: &[u8],
    sig: &Signature,
) {
    signatures_to_verify.push(SignaturesToVerify::SecpBlock {
        public_key: *public_key,
        data: data.to_vec(),
        signature: *sig,
    });
}

/// Add vrf signatures to verification list
pub fn add_dr_vrf_signature_to_verify(
    signatures_to_verify: &mut Vec<SignaturesToVerify>,
    proof: &DataRequestEligibilityClaim,
    vrf_input: CheckpointVRF,
    dr_hash: Hash,
    target_hash: Hash,
) {
    signatures_to_verify.push(SignaturesToVerify::VrfDr {
        proof: proof.clone(),
        vrf_input,
        dr_hash,
        target_hash,
    })
}

/// Add vrf signatures to verification list
pub fn add_block_vrf_signature_to_verify(
    signatures_to_verify: &mut Vec<SignaturesToVerify>,
    proof: &BlockEligibilityClaim,
    vrf_input: CheckpointVRF,
    target_hash: Hash,
) {
    signatures_to_verify.push(SignaturesToVerify::VrfBlock {
        proof: proof.clone(),
        vrf_input,
        target_hash,
    })
}

/// Function to validate a commit/reveal transaction signature
pub fn validate_commit_reveal_signature<'a>(
    tx_hash: Hash,
    signatures: &'a [KeyedSignature],
    signatures_to_verify: &mut Vec<SignaturesToVerify>,
) -> Result<&'a KeyedSignature, failure::Error> {
    let tx_keyed_signature = signatures
        .get(0)
        .ok_or(TransactionError::SignatureNotFound)?;

    // Commitments and reveals should only have one signature
    if signatures.len() != 1 {
        return Err(TransactionError::MismatchingSignaturesNumber {
            signatures_n: u8::try_from(signatures.len())?,
            inputs_n: 1,
        }
        .into());
    }

    let Hash::SHA256(message) = tx_hash;

    let fte = |e: failure::Error| TransactionError::VerifyTransactionSignatureFail {
        hash: tx_hash,
        msg: e.to_string(),
    };

    let signature = tx_keyed_signature
        .signature
        .clone()
        .try_into()
        .map_err(fte)?;
    let public_key = tx_keyed_signature
        .public_key
        .clone()
        .try_into()
        .map_err(fte)?;

    add_secp_tx_signature_to_verify(signatures_to_verify, &public_key, &message, &signature);

    Ok(tx_keyed_signature)
}

/// Function to validate a transaction signature
pub fn validate_transaction_signature(
    signatures: &[KeyedSignature],
    inputs: &[Input],
    tx_hash: Hash,
    utxo_set: &UtxoDiff,
    signatures_to_verify: &mut Vec<SignaturesToVerify>,
) -> Result<(), failure::Error> {
    if signatures.len() != inputs.len() {
        return Err(TransactionError::MismatchingSignaturesNumber {
            signatures_n: u8::try_from(signatures.len())?,
            inputs_n: u8::try_from(inputs.len())?,
        }
        .into());
    }

    let tx_hash_bytes = match tx_hash {
        Hash::SHA256(x) => x.to_vec(),
    };

    for (input, keyed_signature) in inputs.iter().zip(signatures.iter()) {
        // Helper function to map errors to include transaction hash and input
        // index, as well as the error message.
        let fte = |e: failure::Error| TransactionError::VerifyTransactionSignatureFail {
            hash: tx_hash,
            msg: e.to_string(),
        };
        // All of the following map_err can be removed if we refactor this to
        // use a try block, however that's still unstable. See tracking issue:
        // https://github.com/rust-lang/rust/issues/31436

        // Validate that public key hash of the pointed output matches public
        // key in the provided signature
        validate_pkh_signature(input, keyed_signature, utxo_set).map_err(fte)?;

        // Validate the actual signature
        let public_key = keyed_signature.public_key.clone().try_into().map_err(fte)?;
        let signature = keyed_signature.signature.clone().try_into().map_err(fte)?;
        add_secp_tx_signature_to_verify(
            signatures_to_verify,
            &public_key,
            &tx_hash_bytes,
            &signature,
        );
    }

    Ok(())
}

/// HashMap to count commit transactions need for a Data Request
struct WitnessesCount {
    current: u32,
    target: u32,
}
type WitnessesCounter<S> = HashMap<Hash, WitnessesCount, S>;

// Add 1 in the number assigned to a OutputPointer
fn increment_witnesses_counter<S: ::std::hash::BuildHasher>(
    hm: &mut WitnessesCounter<S>,
    k: &Hash,
    rf: u32,
) {
    hm.entry(*k)
        .or_insert(WitnessesCount {
            current: 0,
            target: rf,
        })
        .current += 1;
}

pub fn update_utxo_diff(
    utxo_diff: &mut UtxoDiff,
    inputs: Vec<&Input>,
    outputs: Vec<&ValueTransferOutput>,
    tx_hash: Hash,
) {
    let mut input_pkh = inputs
        .first()
        .and_then(|first| utxo_diff.get(first.output_pointer()).map(|vt| vt.pkh));

    let mut block_number_input = 0;
    for input in inputs {
        // Obtain the OuputPointer of each input and remove it from the utxo_diff
        let output_pointer = input.output_pointer();

        // Returns the input PKH in case that all PKHs are the same
        if input_pkh != utxo_diff.get(output_pointer).map(|vt| vt.pkh) {
            input_pkh = None;
        }

        let block_number = utxo_diff
            .included_in_block_number(output_pointer)
            .unwrap_or(0);
        block_number_input = std::cmp::max(block_number, block_number_input);

        utxo_diff.remove_utxo(output_pointer.clone());
    }

    for (index, output) in outputs.into_iter().enumerate() {
        // Add the new outputs to the utxo_diff
        let output_pointer = OutputPointer {
            transaction_id: tx_hash,
            output_index: u32::try_from(index).unwrap(),
        };

        let block_number = if input_pkh == Some(output.pkh) {
            Some(block_number_input)
        } else {
            None
        };

        utxo_diff.insert_utxo(output_pointer, output.clone(), block_number);
    }
}

/// Function to validate transactions in a block and update a utxo_set and a `TransactionsPool`
#[allow(clippy::too_many_arguments)]
pub fn validate_block_transactions(
    utxo_set: &UnspentOutputsPool,
    dr_pool: &DataRequestPool,
    block: &Block,
    vrf_input: CheckpointVRF,
    signatures_to_verify: &mut Vec<SignaturesToVerify>,
    rep_eng: &ReputationEngine,
    epoch_constants: EpochConstants,
    block_number: u32,
    consensus_constants: &ConsensusConstants,
) -> Result<Diff, failure::Error> {
    let epoch = block.block_header.beacon.checkpoint;
    let is_genesis = block.hash() == consensus_constants.genesis_hash;
    let mut utxo_diff = UtxoDiff::new(utxo_set, block_number);

    // Init total fee
    let mut total_fee = 0;
    // When validating genesis block, keep track of total value created
    // The value created in the genesis block cannot be greater than 2^64 - the total block reward,
    // So the total amount is always representable by a u64
    let max_total_value_genesis = u64::max_value() - total_block_reward();
    let mut genesis_value_available = max_total_value_genesis;

    // TODO: replace for loop with a try_fold
    // Validate value transfer transactions in a block
    let mut vt_mt = ProgressiveMerkleTree::sha256();
    let mut vt_weight: u32 = 0;
    for transaction in &block.txns.value_transfer_txns {
        let (inputs, outputs, fee, weight) = if is_genesis {
            let (outputs, value_created) = validate_genesis_vt_transaction(transaction)?;
            // Update value available, and return error on overflow
            genesis_value_available = genesis_value_available.checked_sub(value_created).ok_or(
                BlockError::GenesisValueOverflow {
                    max_total_value: max_total_value_genesis,
                },
            )?;

            (vec![], outputs, 0, 0)
        } else {
            let (inputs, outputs, fee) = validate_vt_transaction(
                transaction,
                &utxo_diff,
                epoch,
                epoch_constants,
                signatures_to_verify,
                consensus_constants.max_vt_weight,
            )?;

            (inputs, outputs, fee, transaction.weight())
        };
        total_fee += fee;

        // Update vt weight
        let acc_weight = vt_weight.saturating_add(weight);
        if acc_weight > consensus_constants.max_vt_weight {
            return Err(BlockError::TotalValueTransferWeightLimitExceeded {
                weight: acc_weight,
                max_weight: consensus_constants.max_vt_weight,
            }
            .into());
        }
        vt_weight = acc_weight;

        update_utxo_diff(&mut utxo_diff, inputs, outputs, transaction.hash());

        // Add new hash to merkle tree
        let txn_hash = transaction.hash();
        let Hash::SHA256(sha) = txn_hash;
        vt_mt.push(Sha256(sha));
    }
    let vt_hash_merkle_root = vt_mt.root();

    // Validate data request transactions in a block
    let mut dr_mt = ProgressiveMerkleTree::sha256();
    let mut dr_weight: u32 = 0;
    for transaction in &block.txns.data_request_txns {
        let (inputs, outputs, fee) = validate_dr_transaction(
            transaction,
            &utxo_diff,
            epoch,
            epoch_constants,
            signatures_to_verify,
            consensus_constants.collateral_minimum,
            consensus_constants.max_dr_weight,
        )?;
        total_fee += fee;

        update_utxo_diff(&mut utxo_diff, inputs, outputs, transaction.hash());

        // Add new hash to merkle tree
        let txn_hash = transaction.hash();
        let Hash::SHA256(sha) = txn_hash;
        dr_mt.push(Sha256(sha));

        // Update dr weight
        let acc_weight = dr_weight.saturating_add(transaction.weight());
        if acc_weight > consensus_constants.max_dr_weight {
            return Err(BlockError::TotalDataRequestWeightLimitExceeded {
                weight: acc_weight,
                max_weight: consensus_constants.max_dr_weight,
            }
            .into());
        }
        dr_weight = acc_weight;
    }
    let dr_hash_merkle_root = dr_mt.root();

    // Validate commit transactions in a block
    let mut co_mt = ProgressiveMerkleTree::sha256();
    let mut commits_number = HashMap::new();
    let block_beacon = block.block_header.beacon;
    let mut commit_hs = HashSet::with_capacity(block.txns.commit_txns.len());
    for transaction in &block.txns.commit_txns {
        let (dr_pointer, dr_witnesses, fee) = validate_commit_transaction(
            &transaction,
            dr_pool,
            vrf_input,
            signatures_to_verify,
            rep_eng,
            epoch,
            epoch_constants,
            &utxo_diff,
            consensus_constants.collateral_minimum,
            consensus_constants.collateral_age,
            block_number,
        )?;

        // Validation for only one commit for pkh/data request in a block
        let pkh = transaction.body.proof.proof.pkh();
        if !commit_hs.insert((dr_pointer, pkh)) {
            return Err(TransactionError::DuplicatedCommit { pkh, dr_pointer }.into());
        }

        total_fee += fee;

        increment_witnesses_counter(&mut commits_number, &dr_pointer, u32::from(dr_witnesses));

        let (inputs, outputs) = (
            transaction.body.collateral.iter().collect(),
            transaction.body.outputs.iter().collect(),
        );
        update_utxo_diff(&mut utxo_diff, inputs, outputs, transaction.hash());

        // Add new hash to merkle tree
        let txn_hash = transaction.hash();
        let Hash::SHA256(sha) = txn_hash;
        co_mt.push(Sha256(sha));
    }
    let co_hash_merkle_root = co_mt.root();

    // Validate commits number and add commit fees
    for WitnessesCount { current, target } in commits_number.values() {
        if current != target {
            return Err(BlockError::MismatchingCommitsNumber {
                commits: *current,
                rf: *target,
            }
            .into());
        }
    }

    // Validate reveal transactions in a block
    let mut re_mt = ProgressiveMerkleTree::sha256();
    let mut reveal_hs = HashSet::with_capacity(block.txns.reveal_txns.len());
    for transaction in &block.txns.reveal_txns {
        let fee = validate_reveal_transaction(&transaction, dr_pool, signatures_to_verify)?;

        // Validation for only one reveal for pkh/data request in a block
        let pkh = transaction.body.pkh;
        let dr_pointer = transaction.body.dr_pointer;
        if !reveal_hs.insert((dr_pointer, pkh)) {
            return Err(TransactionError::DuplicatedReveal { pkh, dr_pointer }.into());
        }

        total_fee += fee;

        // Add new hash to merkle tree
        let txn_hash = transaction.hash();
        let Hash::SHA256(sha) = txn_hash;
        re_mt.push(Sha256(sha));
    }
    let re_hash_merkle_root = re_mt.root();

    // Validate tally transactions in a block
    let mut ta_mt = ProgressiveMerkleTree::sha256();
    let mut tally_hs = HashSet::with_capacity(block.txns.tally_txns.len());
    for transaction in &block.txns.tally_txns {
        let (outputs, fee) = validate_tally_transaction(
            transaction,
            dr_pool,
            consensus_constants.collateral_minimum,
        )?;

        // Validation for only one tally for data request in a block
        let dr_pointer = transaction.dr_pointer;
        if !tally_hs.insert(dr_pointer) {
            return Err(TransactionError::DuplicatedTally { dr_pointer }.into());
        }

        total_fee += fee;

        update_utxo_diff(&mut utxo_diff, vec![], outputs, transaction.hash());

        // Add new hash to merkle tree
        let txn_hash = transaction.hash();
        let Hash::SHA256(sha) = txn_hash;
        ta_mt.push(Sha256(sha));
    }
    let ta_hash_merkle_root = ta_mt.root();

    if !is_genesis {
        // Validate mint
        validate_mint_transaction(&block.txns.mint, total_fee, block_beacon.checkpoint)?;

        // Insert mint in utxo
        update_utxo_diff(
            &mut utxo_diff,
            vec![],
            block.txns.mint.outputs.iter().collect(),
            block.txns.mint.hash(),
        );
    }

    // Validate Merkle Root
    let merkle_roots = BlockMerkleRoots {
        mint_hash: block.txns.mint.hash(),
        vt_hash_merkle_root: Hash::from(vt_hash_merkle_root),
        dr_hash_merkle_root: Hash::from(dr_hash_merkle_root),
        commit_hash_merkle_root: Hash::from(co_hash_merkle_root),
        reveal_hash_merkle_root: Hash::from(re_hash_merkle_root),
        tally_hash_merkle_root: Hash::from(ta_hash_merkle_root),
    };

    if merkle_roots != block.block_header.merkle_roots {
        Err(BlockError::NotValidMerkleTree.into())
    } else {
        Ok(utxo_diff.take_diff())
    }
}

/// Function to validate a block
#[allow(clippy::too_many_arguments)]
pub fn validate_block(
    block: &Block,
    current_epoch: Epoch,
    vrf_input: CheckpointVRF,
    chain_beacon: CheckpointBeacon,
    signatures_to_verify: &mut Vec<SignaturesToVerify>,
    rep_eng: &ReputationEngine,
    consensus_constants: &ConsensusConstants,
) -> Result<(), failure::Error> {
    let block_epoch = block.block_header.beacon.checkpoint;
    let hash_prev_block = block.block_header.beacon.hash_prev_block;

    if block_epoch > current_epoch {
        Err(BlockError::BlockFromFuture {
            block_epoch,
            current_epoch,
        }
        .into())
    } else if chain_beacon.checkpoint > block_epoch {
        Err(BlockError::BlockOlderThanTip {
            chain_epoch: chain_beacon.checkpoint,
            block_epoch,
        }
        .into())
    } else if chain_beacon.hash_prev_block != hash_prev_block {
        Err(BlockError::PreviousHashMismatch {
            block_hash: hash_prev_block,
            our_hash: chain_beacon.hash_prev_block,
        }
        .into())
    } else if chain_beacon.hash_prev_block == consensus_constants.bootstrap_hash {
        // If the chain_beacon hash_prev_block is the bootstrap hash, only accept blocks
        // with the genesis_block_hash
        validate_genesis_block(block, consensus_constants.genesis_hash).map_err(Into::into)
    } else {
        let total_identities = u32::try_from(rep_eng.ars().active_identities_number())?;
        let (target_hash, _) = calculate_randpoe_threshold(
            total_identities,
            consensus_constants.mining_backup_factor,
            current_epoch,
            consensus_constants.initial_difficulty,
            consensus_constants.epochs_with_initial_difficulty,
        );

        add_block_vrf_signature_to_verify(
            signatures_to_verify,
            &block.block_header.proof,
            vrf_input,
            target_hash,
        );

        validate_block_signature(&block, signatures_to_verify)
    }
}

/// Validate a genesis block: a block with hash_prev_block = bootstrap_hash
pub fn validate_genesis_block(
    genesis_block: &Block,
    expected_genesis_hash: Hash,
) -> Result<(), BlockError> {
    // Compare the hash first
    if genesis_block.hash() != expected_genesis_hash {
        return Err(BlockError::GenesisBlockHashMismatch {
            block_hash: genesis_block.hash(),
            expected_hash: expected_genesis_hash,
        });
    }

    // Create a new genesis block with the same fields, and compare that they are equal
    // This ensure that there is no extra information in the unused fields, for example
    // in the signature as it does not affect the block hash
    let bootstrap_hash = genesis_block.block_header.beacon.hash_prev_block;
    let vtts = genesis_block.txns.value_transfer_txns.clone();
    let new_genesis = Block::genesis(bootstrap_hash, vtts);

    // Verify that the genesis block to validate has the same fields as the
    // empty block we just created
    if &new_genesis == genesis_block {
        Ok(())
    } else {
        Err(BlockError::GenesisBlockMismatch {
            block: format!("{:?}", genesis_block),
            expected: format!("{:?}", new_genesis),
        })
    }
}

/// Validate a standalone transaction received from the network
#[allow(clippy::too_many_arguments)]
pub fn validate_new_transaction(
    transaction: &Transaction,
    (reputation_engine, unspent_outputs_pool, data_request_pool): (
        &ReputationEngine,
        &UnspentOutputsPool,
        &DataRequestPool,
    ),
    vrf_input: CheckpointVRF,
    current_epoch: Epoch,
    epoch_constants: EpochConstants,
    block_number: u32,
    signatures_to_verify: &mut Vec<SignaturesToVerify>,
    collateral_minimum: u64,
    collateral_age: u32,
    max_vt_weight: u32,
    max_dr_weight: u32,
) -> Result<u64, failure::Error> {
    let utxo_diff = UtxoDiff::new(&unspent_outputs_pool, block_number);

    match transaction {
        Transaction::ValueTransfer(tx) => validate_vt_transaction(
            &tx,
            &utxo_diff,
            current_epoch,
            epoch_constants,
            signatures_to_verify,
            max_vt_weight,
        )
        .map(|(_, _, fee)| fee),

        Transaction::DataRequest(tx) => validate_dr_transaction(
            &tx,
            &utxo_diff,
            current_epoch,
            epoch_constants,
            signatures_to_verify,
            collateral_minimum,
            max_dr_weight,
        )
        .map(|(_, _, fee)| fee),
        Transaction::Commit(tx) => validate_commit_transaction(
            &tx,
            &data_request_pool,
            vrf_input,
            signatures_to_verify,
            &reputation_engine,
            current_epoch,
            epoch_constants,
            &utxo_diff,
            collateral_minimum,
            collateral_age,
            block_number,
        )
        .map(|(_, _, fee)| fee),
        Transaction::Reveal(tx) => {
            validate_reveal_transaction(&tx, &data_request_pool, signatures_to_verify)
        }
        _ => Err(TransactionError::NotValidTransaction.into()),
    }
}

pub fn calculate_randpoe_threshold(
    total_identities: u32,
    replication_factor: u32,
    current_epoch: u32,
    initial_difficulty: u32,
    epochs_with_initial_difficulty: u32,
) -> (Hash, f64) {
    let max = u64::max_value();
    let initial_difficulty = std::cmp::max(1, initial_difficulty);
    let target = if current_epoch <= epochs_with_initial_difficulty {
        max / u64::from(initial_difficulty)
    } else if total_identities == 0 || replication_factor >= total_identities {
        max
    } else {
        (max / u64::from(total_identities)) * u64::from(replication_factor)
    };
    let target = u32::try_from(target >> 32).unwrap();

    let probability = f64::from(target) / f64::from(u32::try_from(max >> 32).unwrap());
    (Hash::with_first_u32(target), probability)
}

pub fn calculate_reppoe_threshold(
    rep_eng: &ReputationEngine,
    pkh: &PublicKeyHash,
    num_witnesses: u16,
) -> (Hash, f64) {
    let my_reputation = rep_eng.trs().get(pkh);
    let total_active_rep = rep_eng.total_active_reputation();

    // Add 1 to reputation because otherwise a node with 0 reputation would
    // never be eligible for a data request
    let my_reputation = u64::from(my_reputation.0) + 1;
    let factor = u64::from(rep_eng.threshold_factor(num_witnesses));

    let max = u64::max_value();
    // Check for overflow: when the probability is more than 100%, cap it to 100%
    let target = if my_reputation.saturating_mul(factor) >= total_active_rep {
        max
    } else {
        (max / total_active_rep) * my_reputation.saturating_mul(factor)
    };
    let target = u32::try_from(target >> 32).unwrap();

    let probability = f64::from(target) / f64::from(u32::try_from(max >> 32).unwrap());
    (Hash::with_first_u32(target), probability)
}

/// Used to classify VRF hashes into slots.
///
/// When trying to mine a block, the node considers itself eligible if the hash of the VRF is lower
/// than `calculate_randpoe_threshold(total_identities, rf, 1001,0,0)` with `rf = mining_backup_factor`.
///
/// However, in order to consolidate a block, the nodes choose the best block that is valid under
/// `rf = mining_replication_factor`. If there is no valid block within that range, it retries with
/// increasing values of `rf`. For example, with `mining_backup_factor = 4` and
/// `mining_replication_factor = 8`, there are 5 different slots:
/// `rf = 4, rf = 5, rf = 6, rf = 7, rf = 8`. Blocks in later slots can only be better candidates
/// if the previous slots have zero valid blocks.
#[derive(Clone, Debug, Default)]
pub struct VrfSlots {
    target_hashes: Vec<Hash>,
}

impl VrfSlots {
    /// `target_hashes` must be sorted
    pub fn new(target_hashes: Vec<Hash>) -> Self {
        Self { target_hashes }
    }

    pub fn from_rf(
        total_identities: u32,
        replication_factor: u32,
        backup_factor: u32,
        current_epoch: u32,
        initial_difficulty: u32,
        epochs_with_initial_difficulty: u32,
    ) -> Self {
        Self::new(
            (replication_factor..=backup_factor)
                .map(|rf| {
                    calculate_randpoe_threshold(
                        total_identities,
                        rf,
                        current_epoch,
                        initial_difficulty,
                        epochs_with_initial_difficulty,
                    )
                    .0
                })
                .collect(),
        )
    }

    pub fn slot(&self, hash: &Hash) -> u32 {
        let num_sections = self.target_hashes.len();
        u32::try_from(
            self.target_hashes
                .iter()
                // The section is the index of the first section hash that is less
                // than or equal to the provided hash
                .position(|th| hash <= th)
                // If the provided hash is greater than all of the section hashes,
                // return the number of sections
                .unwrap_or(num_sections),
        )
        .unwrap()
    }
}

/// Function to calculate a merkle tree from a transaction vector
pub fn merkle_tree_root<T>(transactions: &[T]) -> Hash
where
    T: Hashable,
{
    let transactions_hashes: Vec<Sha256> = transactions
        .iter()
        .map(|x| match x.hash() {
            Hash::SHA256(x) => Sha256(x),
        })
        .collect();

    Hash::from(crypto_merkle_tree_root(&transactions_hashes))
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

/// Function to validate block's merkle tree
pub fn validate_merkle_tree(block: &Block) -> bool {
    // Compute `hash_merkle_root` and build block header
    let merkle_roots = BlockMerkleRoots {
        mint_hash: block.txns.mint.hash(),
        vt_hash_merkle_root: merkle_tree_root(&block.txns.value_transfer_txns),
        dr_hash_merkle_root: merkle_tree_root(&block.txns.data_request_txns),
        commit_hash_merkle_root: merkle_tree_root(&block.txns.commit_txns),
        reveal_hash_merkle_root: merkle_tree_root(&block.txns.reveal_txns),
        tally_hash_merkle_root: merkle_tree_root(&block.txns.tally_txns),
    };

    merkle_roots == block.block_header.merkle_roots
}

/// 1 nanowit is the minimal unit of value
/// 1 wit = 10^9 nanowits
pub const NANOWITS_PER_WIT: u64 = 1_000_000_000; // 10 ^ WIT_DECIMAL_PLACES
/// Number of decimal places used in the string representation of wit value.
pub const WIT_DECIMAL_PLACES: u8 = 9;

/// Unit of value
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord)]
pub struct Wit(u64);

impl Wit {
    pub fn from_wits(wits: u64) -> Self {
        Self(wits * NANOWITS_PER_WIT)
    }
    pub fn from_nanowits(nanowits: u64) -> Self {
        Self(nanowits)
    }
    /// Return integer and fractional part, useful for pretty printing
    fn wits_and_nanowits(self) -> (u64, u64) {
        let nanowits = self.0;
        let amount_wits = nanowits / NANOWITS_PER_WIT;
        let amount_nanowits = nanowits % NANOWITS_PER_WIT;

        (amount_wits, amount_nanowits)
    }
}

impl fmt::Display for Wit {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let (amount_wits, amount_nanowits) = self.wits_and_nanowits();
        let width = usize::from(WIT_DECIMAL_PLACES);

        write!(
            f,
            "{}.{:0width$}",
            amount_wits,
            amount_nanowits,
            width = width
        )
    }
}

const INITIAL_BLOCK_REWARD: u64 = 250 * NANOWITS_PER_WIT;
const HALVING_PERIOD: Epoch = 1_750_000;

/// Calculate the block mining reward.
/// Returns nanowits.
pub fn block_reward(epoch: Epoch) -> u64 {
    let initial_reward: u64 = INITIAL_BLOCK_REWARD;
    let halvings = epoch / HALVING_PERIOD;
    if halvings < 64 {
        initial_reward >> halvings
    } else {
        0
    }
}

/// Calculate the total amount of wits that will be rewarded to miners.
pub fn total_block_reward() -> u64 {
    let mut total_reward = 0;
    let mut base_reward = INITIAL_BLOCK_REWARD;
    while base_reward != 0 {
        total_reward += base_reward * u64::from(HALVING_PERIOD);
        base_reward >>= 1;
    }

    total_reward
}

/// Diffs to apply to an utxo set. This type does not contains a
/// reference to the original utxo set.
#[derive(Debug)]
pub struct Diff {
    utxos_to_add: UnspentOutputsPool,
    utxos_to_remove: HashSet<OutputPointer>,
    utxos_to_remove_dr: Vec<OutputPointer>,
    block_number: u32,
}

impl Diff {
    pub fn new(block_number: u32) -> Self {
        Self {
            utxos_to_add: Default::default(),
            utxos_to_remove: Default::default(),
            utxos_to_remove_dr: vec![],
            block_number,
        }
    }

    pub fn apply(mut self, utxo_set: &mut UnspentOutputsPool) {
        for (output_pointer, (output, block_number)) in self.utxos_to_add.drain() {
            utxo_set.insert(output_pointer, output, block_number);
        }

        for output_pointer in self.utxos_to_remove.iter() {
            utxo_set.remove(output_pointer);
        }

        for output_pointer in self.utxos_to_remove_dr.iter() {
            utxo_set.remove(output_pointer);
        }
    }
    /// Iterate over all the utxos_to_add and utxos_to_remove while applying a function.
    ///
    /// Any shared mutable state used by `F1` and `F2` can be used as the first argument:
    ///
    /// ```
    /// use std::collections::HashMap;
    /// use witnet_validations::validations::Diff;
    ///
    /// let block_number = 0;
    /// let diff = Diff::new(block_number);
    /// let mut hashmap = HashMap::new();
    /// diff.visit(&mut hashmap, |hashmap, output_pointer, output| {
    ///     hashmap.insert(output_pointer.clone(), output.clone());
    /// }, |hashmap, output_pointer| {
    ///     hashmap.remove(output_pointer);
    /// });
    /// ```
    pub fn visit<A, F1, F2>(&self, args: &mut A, fn_add: F1, fn_remove: F2)
    where
        F1: Fn(&mut A, &OutputPointer, &ValueTransferOutput) -> (),
        F2: Fn(&mut A, &OutputPointer) -> (),
    {
        for (output_pointer, (output, _)) in self.utxos_to_add.iter() {
            fn_add(args, output_pointer, output);
        }

        for output_pointer in self.utxos_to_remove.iter() {
            fn_remove(args, output_pointer);
        }
    }
}

/// Contains a reference to an UnspentOutputsPool plus subsequent
/// insertions and deletions to performed on that pool.
/// Use `.take_diff()` to obtain an instance of the `Diff` type.
pub struct UtxoDiff<'a> {
    diff: Diff,
    utxo_set: &'a UnspentOutputsPool,
}

impl<'a> UtxoDiff<'a> {
    /// Create a new UtxoDiff without additional insertions or deletions
    pub fn new(utxo_set: &'a UnspentOutputsPool, block_number: u32) -> Self {
        UtxoDiff {
            utxo_set,
            diff: Diff::new(block_number),
        }
    }

    /// Record an insertion to perform on the utxo set
    pub fn insert_utxo(
        &mut self,
        output_pointer: OutputPointer,
        output: ValueTransferOutput,
        block_number: Option<u32>,
    ) {
        self.diff.utxos_to_add.insert(
            output_pointer,
            output,
            block_number.unwrap_or(self.diff.block_number),
        );
    }

    /// Record a deletion to perform on the utxo set
    pub fn remove_utxo(&mut self, output_pointer: OutputPointer) {
        if self.diff.utxos_to_add.remove(&output_pointer).is_none() {
            self.diff.utxos_to_remove.insert(output_pointer);
        }
    }

    /// Record a deletion to perform on the utxo set but that it
    /// doesn't count when getting an utxo with `get` method.
    pub fn remove_utxo_dr(&mut self, output_pointer: OutputPointer) {
        self.diff.utxos_to_remove_dr.push(output_pointer);
    }

    /// Get an utxo from the original utxo set or one that has been
    /// recorded as inserted later. If the same utxo has been recorded
    /// as removed, None will be returned.
    pub fn get(&self, output_pointer: &OutputPointer) -> Option<&ValueTransferOutput> {
        self.utxo_set
            .get(output_pointer)
            .or_else(|| self.diff.utxos_to_add.get(output_pointer))
            .and_then(|output| {
                if self.diff.utxos_to_remove.contains(output_pointer) {
                    None
                } else {
                    Some(output)
                }
            })
    }

    /// Consumes the UtxoDiff and returns only the diffs, without the
    /// reference to the utxo set.
    pub fn take_diff(self) -> Diff {
        self.diff
    }

    /// Returns the number of the block that included the transaction referenced
    /// by this OutputPointer. The difference between that number and the
    /// current number of consolidated blocks is the "collateral age".
    pub fn included_in_block_number(&self, output_pointer: &OutputPointer) -> Option<Epoch> {
        self.utxo_set
            .included_in_block_number(output_pointer)
            .and_then(|output| {
                if self.diff.utxos_to_remove.contains(output_pointer) {
                    None
                } else {
                    Some(output)
                }
            })
    }
}

/// Compare block candidates.
///
/// The comparison algorithm is:
/// * Calculate sections of each VRF hash. See [VrfSections] for more information.
/// * Choose the block in the smaller VRF section.
/// * In case of tie, choose the block whose pkh has most total reputation.
/// * In case of tie, choose the block with the lower VRF hash.
/// * In case of tie, choose the block with the lower block hash.
/// * In case of tie, they are the same block.
///
/// Returns `Ordering::Greater` if candidate 1 is better than candidate 2.
///
/// Note that this only compares the block candidates, it does not validate them. A block must be
/// the best candidate and additionally it must be valid in order to be the consolidated block.
pub fn compare_block_candidates(
    b1_hash: Hash,
    b1_rep: Reputation,
    b1_vrf_hash: Hash,
    b2_hash: Hash,
    b2_rep: Reputation,
    b2_vrf_hash: Hash,
    s: &VrfSlots,
) -> Ordering {
    let section1 = s.slot(&b1_vrf_hash);
    let section2 = s.slot(&b2_vrf_hash);
    // Bigger section implies worse block candidate
    section1
        .cmp(&section2)
        .reverse()
        // Bigger reputation implies better block candidate
        .then(b1_rep.cmp(&b2_rep))
        // Bigger vrf hash implies worse block candidate
        .then(b1_vrf_hash.cmp(&b2_vrf_hash).reverse())
        // Bigger block implies worse block candidate
        .then(b1_hash.cmp(&b2_hash).reverse())
}

/// Blocking process to verify signatures
pub fn verify_signatures(
    signatures_to_verify: Vec<SignaturesToVerify>,
    vrf: &mut VrfCtx,
    secp: &CryptoEngine,
) -> Result<Vec<Hash>, failure::Error> {
    let mut vrf_hashes = vec![];
    for x in signatures_to_verify {
        match x {
            SignaturesToVerify::VrfBlock {
                proof,
                vrf_input,
                target_hash,
            } => {
                let vrf_hash = proof
                    .verify(vrf, vrf_input)
                    .map_err(|_| BlockError::NotValidPoe)?;
                if vrf_hash > target_hash {
                    return Err(BlockError::BlockEligibilityDoesNotMeetTarget {
                        vrf_hash,
                        target_hash,
                    }
                    .into());
                }
                vrf_hashes.push(vrf_hash);
            }
            SignaturesToVerify::VrfDr {
                proof,
                vrf_input,
                dr_hash,
                target_hash,
            } => {
                let vrf_hash = proof
                    .verify(vrf, vrf_input, dr_hash)
                    .map_err(|_| TransactionError::InvalidDataRequestPoe)?;
                if vrf_hash > target_hash {
                    return Err(TransactionError::DataRequestEligibilityDoesNotMeetTarget {
                        vrf_hash,
                        target_hash,
                    }
                    .into());
                }
            }
            SignaturesToVerify::SecpTx {
                public_key,
                data,
                signature,
            } => verify(secp, &public_key, &data, &signature).map_err(|e| {
                TransactionError::VerifyTransactionSignatureFail {
                    hash: {
                        let mut sha256 = [0; 32];
                        sha256.copy_from_slice(&data);
                        Hash::SHA256(sha256)
                    },
                    msg: e.to_string(),
                }
            })?,
            SignaturesToVerify::SecpBlock {
                public_key,
                data,
                signature,
            } => verify(secp, &public_key, &data, &signature).map_err(|_e| {
                BlockError::VerifySignatureFail {
                    hash: {
                        let mut sha256 = [0; 32];
                        sha256.copy_from_slice(&data);
                        Hash::SHA256(sha256)
                    },
                }
            })?,
            SignaturesToVerify::SuperBlockVote { superblock_vote } => {
                // Validates secp256k1 signature only, bn256 signature is not validated
                let secp_message = superblock_vote.secp256k1_signature_message();
                let secp_message_hash = calculate_sha256(&secp_message);
                verify(
                    secp,
                    &superblock_vote
                        .secp256k1_signature
                        .public_key
                        .try_into()
                        .unwrap(),
                    &secp_message_hash.0,
                    &superblock_vote
                        .secp256k1_signature
                        .signature
                        .try_into()
                        .unwrap(),
                )
                .map_err(|e| e)?;
            }
        }
    }

    Ok(vrf_hashes)
}

#[cfg(test)]
mod tests {
    use witnet_data_structures::{chain::Alpha, radon_error::RadonError};
    use witnet_rad::types::{float::RadonFloat, integer::RadonInteger};

    use super::*;

    #[test]
    fn test_compare_candidate_same_section() {
        let bh_1 = Hash::SHA256([10; 32]);
        let bh_2 = Hash::SHA256([20; 32]);
        let rep_1 = Reputation(1);
        let rep_2 = Reputation(2);
        let vrf_1 = Hash::SHA256([1; 32]);
        let vrf_2 = Hash::SHA256([2; 32]);
        // Only one section and all VRFs are valid
        let vrf_sections = VrfSlots::default();

        // The candidate with greater reputation always wins
        for &bh_i in &[bh_1, bh_2] {
            for &bh_j in &[bh_1, bh_2] {
                for &vrf_i in &[vrf_1, vrf_2] {
                    for &vrf_j in &[vrf_1, vrf_2] {
                        assert_eq!(
                            compare_block_candidates(
                                bh_i,
                                rep_1,
                                vrf_i,
                                bh_j,
                                rep_2,
                                vrf_j,
                                &vrf_sections
                            ),
                            Ordering::Less
                        );
                        assert_eq!(
                            compare_block_candidates(
                                bh_i,
                                rep_2,
                                vrf_i,
                                bh_j,
                                rep_1,
                                vrf_j,
                                &vrf_sections
                            ),
                            Ordering::Greater
                        );
                    }
                }
            }
        }

        // Equal reputation: the candidate with lower VRF hash wins
        for &bh_i in &[bh_1, bh_2] {
            for &bh_j in &[bh_1, bh_2] {
                assert_eq!(
                    compare_block_candidates(bh_i, rep_1, vrf_1, bh_j, rep_1, vrf_2, &vrf_sections),
                    Ordering::Greater
                );
                assert_eq!(
                    compare_block_candidates(bh_i, rep_1, vrf_2, bh_j, rep_1, vrf_1, &vrf_sections),
                    Ordering::Less
                );
            }
        }

        // Equal reputation and equal VRF hash: the candidate with lower block hash wins
        assert_eq!(
            compare_block_candidates(bh_1, rep_1, vrf_1, bh_2, rep_1, vrf_1, &vrf_sections),
            Ordering::Greater
        );
        assert_eq!(
            compare_block_candidates(bh_2, rep_1, vrf_1, bh_1, rep_1, vrf_1, &vrf_sections),
            Ordering::Less
        );

        // Everything equal: it is the same block
        assert_eq!(
            compare_block_candidates(bh_1, rep_1, vrf_1, bh_1, rep_1, vrf_1, &vrf_sections),
            Ordering::Equal
        );
    }

    #[test]
    fn test_compare_candidate_different_section() {
        let bh_1 = Hash::SHA256([10; 32]);
        let bh_2 = Hash::SHA256([20; 32]);
        let rep_1 = Reputation(1);
        let rep_2 = Reputation(2);
        // Candidate 1 should always be better than candidate 2
        let vrf_sections = VrfSlots::from_rf(16, 1, 2, 1001, 0, 0);
        // Candidate 1 is in section 0
        let vrf_1 = vrf_sections.target_hashes[0];
        // Candidate 2 is in section 1
        let vrf_2 = vrf_sections.target_hashes[1];

        // The candidate in the lower section always wins
        for &bh_i in &[bh_1, bh_2] {
            for &bh_j in &[bh_1, bh_2] {
                for &rep_i in &[rep_1, rep_2] {
                    for &rep_j in &[rep_1, rep_2] {
                        assert_eq!(
                            compare_block_candidates(
                                bh_i,
                                rep_i,
                                vrf_1,
                                bh_j,
                                rep_j,
                                vrf_2,
                                &vrf_sections
                            ),
                            Ordering::Greater
                        );
                        assert_eq!(
                            compare_block_candidates(
                                bh_i,
                                rep_i,
                                vrf_2,
                                bh_j,
                                rep_j,
                                vrf_1,
                                &vrf_sections
                            ),
                            Ordering::Less
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn wit_decimal_places() {
        // 10 ^ WIT_DECIMAL_PLACES == NANOWITS_PER_WIT
        assert_eq!(10u64.pow(u32::from(WIT_DECIMAL_PLACES)), NANOWITS_PER_WIT);
    }

    #[test]
    fn wit_pretty_print() {
        assert_eq!(Wit::from_nanowits(0).to_string(), "0.000000000");
        assert_eq!(Wit::from_nanowits(1).to_string(), "0.000000001");
        assert_eq!(Wit::from_nanowits(90).to_string(), "0.000000090");
        assert_eq!(Wit::from_nanowits(890).to_string(), "0.000000890");
        assert_eq!(Wit::from_nanowits(7_890).to_string(), "0.000007890");
        assert_eq!(Wit::from_nanowits(67_890).to_string(), "0.000067890");
        assert_eq!(Wit::from_nanowits(567_890).to_string(), "0.000567890");
        assert_eq!(Wit::from_nanowits(4_567_890).to_string(), "0.004567890");
        assert_eq!(Wit::from_nanowits(34_567_890).to_string(), "0.034567890");
        assert_eq!(Wit::from_nanowits(234_567_890).to_string(), "0.234567890");
        assert_eq!(Wit::from_nanowits(1_234_567_890).to_string(), "1.234567890");
        assert_eq!(
            Wit::from_nanowits(21_234_567_890).to_string(),
            "21.234567890"
        );
        assert_eq!(
            Wit::from_nanowits(321_234_567_890).to_string(),
            "321.234567890"
        );
    }

    #[allow(clippy::cast_possible_truncation)]
    #[allow(clippy::cast_precision_loss)]
    #[allow(clippy::cast_sign_loss)]
    #[test]
    fn test_block_reward() {
        // 1 wit = 10^9 nanowits, block_reward returns nanowits
        let wit = 1_000_000_000;

        assert_eq!(block_reward(0), 250 * wit);
        assert_eq!(block_reward(1), 250 * wit);
        assert_eq!(block_reward(1_749_999), 250 * wit);
        assert_eq!(block_reward(1_750_000), 125 * wit);
        assert_eq!(block_reward(3_499_999), 125 * wit);
        assert_eq!(block_reward(3_500_000), (62.5 * wit as f64).floor() as u64);
        assert_eq!(block_reward(1_750_000 * 36), 3);
        assert_eq!(block_reward(1_750_000 * 37), 1);
        assert_eq!(block_reward(1_750_000 * 38), 0);
        assert_eq!(block_reward(1_750_000 * 63), 0);
        assert_eq!(block_reward(1_750_000 * 64), 0);
        assert_eq!(block_reward(1_750_000 * 65), 0);
        assert_eq!(block_reward(1_750_000 * 100), 0);
    }

    // FIXME: Allow for now, wait for https://github.com/rust-lang/rust/issues/67058 to reach stable
    #[allow(clippy::cast_possible_truncation)]
    #[test]
    fn target_randpoe() {
        let rf = 1;
        let max_hash = Hash::with_first_u32(0xFFFF_FFFF);
        let (t00, p00) = calculate_randpoe_threshold(0, rf, 1001, 0, 0);
        let (t01, p01) = calculate_randpoe_threshold(1, rf, 1001, 0, 0);
        assert_eq!(t00, max_hash);
        assert_eq!(t00, t01);
        assert_eq!((p00 * 100_f64).round() as i128, 100);
        assert_eq!(
            (p00 * 100_f64).round() as i128,
            (p01 * 100_f64).round() as i128
        );
        let (t02, p02) = calculate_randpoe_threshold(2, rf, 1001, 0, 0);
        assert_eq!(t02, Hash::with_first_u32(0x7FFF_FFFF));
        assert_eq!((p02 * 100_f64).round() as i128, 50);
        let (t03, p03) = calculate_randpoe_threshold(3, rf, 1001, 0, 0);
        assert_eq!(t03, Hash::with_first_u32(0x5555_5555));
        assert_eq!((p03 * 100_f64).round() as i128, 33);
        let (t04, p04) = calculate_randpoe_threshold(4, rf, 1001, 0, 0);
        assert_eq!(t04, Hash::with_first_u32(0x3FFF_FFFF));
        assert_eq!((p04 * 100_f64).round() as i128, 25);
        let (t05, p05) = calculate_randpoe_threshold(1024, rf, 1001, 0, 0);
        assert_eq!(t05, Hash::with_first_u32(0x003F_FFFF));
        assert_eq!((p05 * 100_f64).round() as i128, 0);
        let (t06, p06) = calculate_randpoe_threshold(1024 * 1024, rf, 1001, 0, 0);
        assert_eq!(t06, Hash::with_first_u32(0x0000_0FFF));
        assert_eq!((p06 * 100_f64).round() as i128, 0);
    }

    #[allow(clippy::cast_possible_truncation)]
    #[test]
    fn target_randpoe_initial_difficulty() {
        let (t, p) = calculate_randpoe_threshold(2, 1, 1, 4, 10);
        assert_eq!(t, Hash::with_first_u32(0x3FFF_FFFF));
        assert_eq!((p * 100_f64).round() as i128, 25);

        let (t, p) = calculate_randpoe_threshold(2, 1, 11, 4, 10);
        assert_eq!(t, Hash::with_first_u32(0x7FFF_FFFF));
        assert_eq!((p * 100_f64).round() as i128, 50);
    }

    // FIXME: Allow for now, wait for https://github.com/rust-lang/rust/issues/67058 to reach stable
    #[allow(clippy::cast_possible_truncation)]
    #[test]
    fn target_randpoe_rf_4() {
        let rf = 4;
        let max_hash = Hash::with_first_u32(0xFFFF_FFFF);
        let (t00, p00) = calculate_randpoe_threshold(0, rf, 1001, 0, 0);
        let (t01, p01) = calculate_randpoe_threshold(1, rf, 1001, 0, 0);
        assert_eq!(t00, max_hash);
        assert_eq!(t01, max_hash);
        assert_eq!((p00 * 100_f64).round() as i128, 100);
        assert_eq!((p01 * 100_f64).round() as i128, 100);
        let (t02, p02) = calculate_randpoe_threshold(2, rf, 1001, 0, 0);
        assert_eq!(t02, max_hash);
        assert_eq!((p02 * 100_f64).round() as i128, 100);
        let (t03, p03) = calculate_randpoe_threshold(3, rf, 1001, 0, 0);
        assert_eq!(t03, max_hash);
        assert_eq!((p03 * 100_f64).round() as i128, 100);
        let (t04, p04) = calculate_randpoe_threshold(4, rf, 1001, 0, 0);
        assert_eq!(t04, max_hash);
        assert_eq!((p04 * 100_f64).round() as i128, 100);
        let (t05, p05) = calculate_randpoe_threshold(1024, rf, 1001, 0, 0);
        assert_eq!(t05, Hash::with_first_u32(0x00FF_FFFF));
        assert_eq!((p05 * 100_f64).round() as i128, 0);
        let (t06, p06) = calculate_randpoe_threshold(1024 * 1024, rf, 1001, 0, 0);
        assert_eq!(t06, Hash::with_first_u32(0x0000_3FFF));
        assert_eq!((p06 * 100_f64).round() as i128, 0);
    }

    // FIXME: Allow for now, wait for https://github.com/rust-lang/rust/issues/67058 to reach stable
    #[allow(clippy::cast_possible_truncation)]
    #[test]
    fn target_reppoe() {
        let mut rep_engine = ReputationEngine::new(1000);
        let id1 = PublicKeyHash::from_bytes(&[1; 20]).unwrap();
        rep_engine
            .trs_mut()
            .gain(Alpha(10), vec![(id1, Reputation(50))])
            .unwrap();
        rep_engine.ars_mut().push_activity(vec![id1]);

        // 100% when we have all the reputation
        let (t00, p00) = calculate_reppoe_threshold(&rep_engine, &id1, 1);
        assert_eq!(t00, Hash::with_first_u32(0xFFFF_FFFF));
        assert_eq!((p00 * 100_f64).round() as i128, 100);

        let id2 = PublicKeyHash::from_bytes(&[2; 20]).unwrap();
        rep_engine
            .trs_mut()
            .gain(Alpha(10), vec![(id2, Reputation(50))])
            .unwrap();
        rep_engine.ars_mut().push_activity(vec![id2]);

        // 50% when there are 2 nodes with 50% of the reputation each
        let (t01, p01) = calculate_reppoe_threshold(&rep_engine, &id1, 1);
        // Since the calculate_reppoe function first divides and later
        // multiplies, we get a rounding error here
        assert_eq!(t01, Hash::with_first_u32(0x7FFF_FFFF));
        assert_eq!((p01 * 100_f64).round() as i128, 50);
    }

    #[test]
    // FIXME: Allow for now, wait for https://github.com/rust-lang/rust/issues/67058 to reach stable
    #[allow(clippy::cast_possible_truncation, clippy::cognitive_complexity)]
    fn target_reppoe_specific_example() {
        let mut rep_engine = ReputationEngine::new(1000);
        let mut ids = vec![];
        for i in 0..8 {
            ids.push(PublicKeyHash::from_bytes(&[i; 20]).unwrap());
        }
        rep_engine.ars_mut().push_activity(ids.clone());

        rep_engine
            .trs_mut()
            .gain(Alpha(10), vec![(ids[0], Reputation(79))])
            .unwrap();
        rep_engine
            .trs_mut()
            .gain(Alpha(10), vec![(ids[1], Reputation(9))])
            .unwrap();
        rep_engine
            .trs_mut()
            .gain(Alpha(10), vec![(ids[2], Reputation(1))])
            .unwrap();
        rep_engine
            .trs_mut()
            .gain(Alpha(10), vec![(ids[3], Reputation(1))])
            .unwrap();
        rep_engine
            .trs_mut()
            .gain(Alpha(10), vec![(ids[4], Reputation(1))])
            .unwrap();
        rep_engine
            .trs_mut()
            .gain(Alpha(10), vec![(ids[5], Reputation(1))])
            .unwrap();

        let (_, p00) = calculate_reppoe_threshold(&rep_engine, &ids[0], 1);
        assert_eq!((p00 * 100_f64).round() as i128, 80);
        let (_, p00) = calculate_reppoe_threshold(&rep_engine, &ids[1], 1);
        assert_eq!((p00 * 100_f64).round() as i128, 10);
        let (_, p00) = calculate_reppoe_threshold(&rep_engine, &ids[2], 1);
        assert_eq!((p00 * 100_f64).round() as i128, 2);
        let (_, p00) = calculate_reppoe_threshold(&rep_engine, &ids[3], 1);
        assert_eq!((p00 * 100_f64).round() as i128, 2);
        let (_, p00) = calculate_reppoe_threshold(&rep_engine, &ids[4], 1);
        assert_eq!((p00 * 100_f64).round() as i128, 2);
        let (_, p00) = calculate_reppoe_threshold(&rep_engine, &ids[5], 1);
        assert_eq!((p00 * 100_f64).round() as i128, 2);
        let (_, p00) = calculate_reppoe_threshold(&rep_engine, &ids[6], 1);
        assert_eq!((p00 * 100_f64).round() as i128, 1);
        let (_, p00) = calculate_reppoe_threshold(&rep_engine, &ids[7], 1);
        assert_eq!((p00 * 100_f64).round() as i128, 1);

        let (_, p00) = calculate_reppoe_threshold(&rep_engine, &ids[0], 2);
        assert_eq!((p00 * 100_f64).round() as i128, 100);
        let (_, p00) = calculate_reppoe_threshold(&rep_engine, &ids[1], 2);
        assert_eq!((p00 * 100_f64).round() as i128, 50);
        let (_, p00) = calculate_reppoe_threshold(&rep_engine, &ids[2], 2);
        assert_eq!((p00 * 100_f64).round() as i128, 10);
        let (_, p00) = calculate_reppoe_threshold(&rep_engine, &ids[3], 2);
        assert_eq!((p00 * 100_f64).round() as i128, 10);
        let (_, p00) = calculate_reppoe_threshold(&rep_engine, &ids[4], 2);
        assert_eq!((p00 * 100_f64).round() as i128, 10);
        let (_, p00) = calculate_reppoe_threshold(&rep_engine, &ids[5], 2);
        assert_eq!((p00 * 100_f64).round() as i128, 10);
        let (_, p00) = calculate_reppoe_threshold(&rep_engine, &ids[6], 2);
        assert_eq!((p00 * 100_f64).round() as i128, 5);
        let (_, p00) = calculate_reppoe_threshold(&rep_engine, &ids[7], 2);
        assert_eq!((p00 * 100_f64).round() as i128, 5);

        let (_, p00) = calculate_reppoe_threshold(&rep_engine, &ids[0], 3);
        assert_eq!((p00 * 100_f64).round() as i128, 100);
        let (_, p00) = calculate_reppoe_threshold(&rep_engine, &ids[1], 3);
        assert_eq!((p00 * 100_f64).round() as i128, 100);
        let (_, p00) = calculate_reppoe_threshold(&rep_engine, &ids[2], 3);
        assert_eq!((p00 * 100_f64).round() as i128, 20);
        let (_, p00) = calculate_reppoe_threshold(&rep_engine, &ids[3], 3);
        assert_eq!((p00 * 100_f64).round() as i128, 20);
        let (_, p00) = calculate_reppoe_threshold(&rep_engine, &ids[4], 3);
        assert_eq!((p00 * 100_f64).round() as i128, 20);
        let (_, p00) = calculate_reppoe_threshold(&rep_engine, &ids[5], 3);
        assert_eq!((p00 * 100_f64).round() as i128, 20);
        let (_, p00) = calculate_reppoe_threshold(&rep_engine, &ids[6], 3);
        assert_eq!((p00 * 100_f64).round() as i128, 10);
        let (_, p00) = calculate_reppoe_threshold(&rep_engine, &ids[7], 3);
        assert_eq!((p00 * 100_f64).round() as i128, 10);

        let (_, p00) = calculate_reppoe_threshold(&rep_engine, &ids[0], 4);
        assert_eq!((p00 * 100_f64).round() as i128, 100);
        let (_, p00) = calculate_reppoe_threshold(&rep_engine, &ids[1], 4);
        assert_eq!((p00 * 100_f64).round() as i128, 100);
        let (_, p00) = calculate_reppoe_threshold(&rep_engine, &ids[2], 4);
        assert_eq!((p00 * 100_f64).round() as i128, 40);
        let (_, p00) = calculate_reppoe_threshold(&rep_engine, &ids[3], 4);
        assert_eq!((p00 * 100_f64).round() as i128, 40);
        let (_, p00) = calculate_reppoe_threshold(&rep_engine, &ids[4], 4);
        assert_eq!((p00 * 100_f64).round() as i128, 40);
        let (_, p00) = calculate_reppoe_threshold(&rep_engine, &ids[5], 4);
        assert_eq!((p00 * 100_f64).round() as i128, 40);
        let (_, p00) = calculate_reppoe_threshold(&rep_engine, &ids[6], 4);
        assert_eq!((p00 * 100_f64).round() as i128, 20);
        let (_, p00) = calculate_reppoe_threshold(&rep_engine, &ids[7], 4);
        assert_eq!((p00 * 100_f64).round() as i128, 20);

        let (_, p00) = calculate_reppoe_threshold(&rep_engine, &ids[0], 5);
        assert_eq!((p00 * 100_f64).round() as i128, 100);
        let (_, p00) = calculate_reppoe_threshold(&rep_engine, &ids[1], 5);
        assert_eq!((p00 * 100_f64).round() as i128, 100);
        let (_, p00) = calculate_reppoe_threshold(&rep_engine, &ids[2], 5);
        assert_eq!((p00 * 100_f64).round() as i128, 60);
        let (_, p00) = calculate_reppoe_threshold(&rep_engine, &ids[3], 5);
        assert_eq!((p00 * 100_f64).round() as i128, 60);
        let (_, p00) = calculate_reppoe_threshold(&rep_engine, &ids[4], 5);
        assert_eq!((p00 * 100_f64).round() as i128, 60);
        let (_, p00) = calculate_reppoe_threshold(&rep_engine, &ids[5], 5);
        assert_eq!((p00 * 100_f64).round() as i128, 60);
        let (_, p00) = calculate_reppoe_threshold(&rep_engine, &ids[6], 5);
        assert_eq!((p00 * 100_f64).round() as i128, 30);
        let (_, p00) = calculate_reppoe_threshold(&rep_engine, &ids[7], 5);
        assert_eq!((p00 * 100_f64).round() as i128, 30);
    }

    // FIXME: Allow for now, wait for https://github.com/rust-lang/rust/issues/67058 to reach stable
    #[allow(clippy::cast_possible_truncation)]
    #[test]
    fn target_reppoe_zero_reputation() {
        // Test the behavior of the algorithm when our node has 0 reputation
        let mut rep_engine = ReputationEngine::new(1000);
        let id0 = PublicKeyHash::from_bytes(&[0; 20]).unwrap();

        // 100% when the total reputation is 0
        let (t00, p00) = calculate_reppoe_threshold(&rep_engine, &id0, 1);
        assert_eq!(t00, Hash::with_first_u32(0xFFFF_FFFF));
        assert_eq!((p00 * 100_f64).round() as i128, 100);
        let (t01, p01) = calculate_reppoe_threshold(&rep_engine, &id0, 100);
        assert_eq!(t01, Hash::with_first_u32(0xFFFF_FFFF));
        assert_eq!((p01 * 100_f64).round() as i128, 100);

        let id1 = PublicKeyHash::from_bytes(&[1; 20]).unwrap();
        rep_engine.ars_mut().push_activity(vec![id1]);
        let (t02, p02) = calculate_reppoe_threshold(&rep_engine, &id0, 1);
        assert_eq!(t02, Hash::with_first_u32(0xFFFF_FFFF));
        assert_eq!((p02 * 100_f64).round() as i128, 100);

        // 50% when the total reputation is 1
        rep_engine
            .trs_mut()
            .gain(Alpha(10), vec![(id1, Reputation(1))])
            .unwrap();
        let (t03, p03) = calculate_reppoe_threshold(&rep_engine, &id0, 1);
        assert_eq!(t03, Hash::with_first_u32(0x7FFF_FFFF));
        assert_eq!((p03 * 100_f64).round() as i128, 50);

        // 33% when the total reputation is 1 but there are 2 active identities
        let id2 = PublicKeyHash::from_bytes(&[2; 20]).unwrap();
        rep_engine.ars_mut().push_activity(vec![id2]);
        let (t04, p04) = calculate_reppoe_threshold(&rep_engine, &id0, 1);
        assert_eq!(t04, Hash::with_first_u32(0x5555_5555));
        assert_eq!((p04 * 100_f64).round() as i128, 33);

        // 10 identities with 100 total reputation: 1 / (100 + 10) ~= 0.9%
        for i in 3..=10 {
            rep_engine
                .ars_mut()
                .push_activity(vec![PublicKeyHash::from_bytes(&[i; 20]).unwrap()]);
        }
        rep_engine
            .trs_mut()
            .gain(Alpha(10), vec![(id1, Reputation(99))])
            .unwrap();
        let (t05, p05) = calculate_reppoe_threshold(&rep_engine, &id0, 1);
        assert_eq!(t05, Hash::with_first_u32(0x0253_C825));
        assert_eq!((p05 * 100_f64).round() as i128, 1);
    }

    // FIXME: Allow for now, wait for https://github.com/rust-lang/rust/issues/67058 to reach stable
    #[allow(clippy::cast_possible_truncation)]
    #[test]
    fn reppoe_overflow() {
        // Test the behavior of the algorithm when our node has 0 reputation
        let mut rep_engine = ReputationEngine::new(1000);
        let id0 = PublicKeyHash::from_bytes(&[0; 20]).unwrap();
        let id1 = PublicKeyHash::from_bytes(&[1; 20]).unwrap();
        rep_engine.ars_mut().push_activity(vec![id0]);
        rep_engine.ars_mut().push_activity(vec![id1]);
        rep_engine
            .trs_mut()
            .gain(Alpha(10), vec![(id0, Reputation(u32::max_value() - 2))])
            .unwrap();

        // Test big values that result in < 100%
        let (t01, p01) = calculate_reppoe_threshold(&rep_engine, &id0, 1);
        assert_eq!(t01, Hash::with_first_u32(0xFFFF_FFFE));
        assert_eq!((p01 * 100_f64).round() as i128, 100);
    }

    #[test]
    fn test_counter() {
        let mut counter = Counter::new(7);
        counter.increment(6);
        assert_eq!(counter.max_val, 1);
        assert_eq!(counter.max_pos, Some(6));

        counter.increment(6);
        assert_eq!(counter.max_val, 2);
        assert_eq!(counter.max_pos, Some(6));

        counter.increment(0);
        assert_eq!(counter.max_val, 2);
        assert_eq!(counter.max_pos, Some(6));

        counter.increment(0);
        counter.increment(0);
        assert_eq!(counter.max_val, 3);
        assert_eq!(counter.max_pos, Some(0));

        counter.increment(6);
        assert_eq!(counter.max_val, 3);
        assert_eq!(counter.max_pos, None);
    }

    #[test]
    fn test_tally_precondition_clause_3_ints_vs_1_float() {
        let rad_int = RadonTypes::Integer(RadonInteger::from(1));
        let rad_float = RadonTypes::Float(RadonFloat::from(1));

        let rad_rep_int = RadonReport::from_result(Ok(rad_int.clone()), &ReportContext::default());
        let rad_rep_float = RadonReport::from_result(Ok(rad_float), &ReportContext::default());

        let v = vec![
            rad_rep_int.clone(),
            rad_rep_int.clone(),
            rad_rep_int,
            rad_rep_float,
        ];
        let tally_precondition_clause_result =
            evaluate_tally_precondition_clause(v, 0.70, 4).unwrap();

        if let TallyPreconditionClauseResult::MajorityOfValues {
            values,
            liars,
            errors,
        } = tally_precondition_clause_result
        {
            assert_eq!(values, vec![rad_int.clone(), rad_int.clone(), rad_int]);
            assert_eq!(liars, vec![false, false, false, true]);
            assert_eq!(errors, vec![false, false, false, false]);
        } else {
            panic!("The result of the tally precondition clause was not `MajorityOfValues`. It was: {:?}", tally_precondition_clause_result);
        }
    }

    #[test]
    fn test_tally_precondition_clause_full_consensus() {
        let rad_int = RadonTypes::Integer(RadonInteger::from(1));

        let rad_rep_int = RadonReport::from_result(Ok(rad_int.clone()), &ReportContext::default());

        let v = vec![rad_rep_int.clone(), rad_rep_int];
        let tally_precondition_clause_result =
            evaluate_tally_precondition_clause(v, 0.99, 2).unwrap();

        if let TallyPreconditionClauseResult::MajorityOfValues {
            values,
            liars,
            errors,
        } = tally_precondition_clause_result
        {
            assert_eq!(values, vec![rad_int.clone(), rad_int]);
            assert_eq!(liars, vec![false, false]);
            assert_eq!(errors, vec![false, false]);
        } else {
            panic!("The result of the tally precondition clause was not `MajorityOfValues`. It was: {:?}", tally_precondition_clause_result);
        }
    }

    #[test]
    fn test_tally_precondition_clause_exact_consensus() {
        let rad_int = RadonTypes::Integer(RadonInteger::from(1));

        let rad_rep_int = RadonReport::from_result(Ok(rad_int.clone()), &ReportContext::default());

        let v = vec![rad_rep_int.clone(), rad_rep_int];
        let tally_precondition_clause_result =
            evaluate_tally_precondition_clause(v, 1., 2).unwrap();

        if let TallyPreconditionClauseResult::MajorityOfValues {
            values,
            liars,
            errors,
        } = tally_precondition_clause_result
        {
            assert_eq!(values, vec![rad_int.clone(), rad_int]);
            assert_eq!(liars, vec![false, false]);
            assert_eq!(errors, vec![false, false]);
        } else {
            panic!("The result of the tally precondition clause was not `MajorityOfValues`. It was: {:?}", tally_precondition_clause_result);
        }
    }

    #[test]
    fn test_tally_precondition_clause_3_ints_vs_1_error() {
        let rad_int = RadonTypes::Integer(RadonInteger::from(1));
        let rad_err = RadError::HttpStatus { status_code: 404 };

        let rad_rep_int = RadonReport::from_result(Ok(rad_int.clone()), &ReportContext::default());
        let rad_rep_err = RadonReport::from_result(Err(rad_err), &ReportContext::default());

        let v = vec![
            rad_rep_int.clone(),
            rad_rep_err,
            rad_rep_int.clone(),
            rad_rep_int,
        ];
        let tally_precondition_clause_result =
            evaluate_tally_precondition_clause(v, 0.70, 4).unwrap();

        if let TallyPreconditionClauseResult::MajorityOfValues {
            values,
            liars,
            errors,
        } = tally_precondition_clause_result
        {
            assert_eq!(values, vec![rad_int.clone(), rad_int.clone(), rad_int]);
            assert_eq!(liars, vec![false, true, false, false]);
            assert_eq!(errors, vec![false, true, false, false]);
        } else {
            panic!("The result of the tally precondition clause was not `MajorityOfValues`. It was: {:?}", tally_precondition_clause_result);
        }
    }

    #[test]
    fn test_tally_precondition_clause_majority_of_errors() {
        let rad_int = RadonTypes::Integer(RadonInteger::from(1));
        let rad_err = RadonError::try_from(RadError::HttpStatus { status_code: 0 }).unwrap();

        let rad_rep_int = RadonReport::from_result(Ok(rad_int), &ReportContext::default());
        let rad_rep_err = RadonReport::from_result(
            Ok(RadonTypes::RadonError(rad_err.clone())),
            &ReportContext::default(),
        );

        let v = vec![
            rad_rep_err.clone(),
            rad_rep_err.clone(),
            rad_rep_err,
            rad_rep_int,
        ];
        let tally_precondition_clause_result =
            evaluate_tally_precondition_clause(v, 0.70, 4).unwrap();

        if let TallyPreconditionClauseResult::MajorityOfErrors { errors_mode } =
            tally_precondition_clause_result
        {
            assert_eq!(errors_mode, rad_err);
        } else {
            panic!("The result of the tally precondition clause was not `MajorityOfErrors`. It was: {:?}", tally_precondition_clause_result);
        }
    }

    #[test]
    fn test_tally_precondition_clause_mode_tie() {
        let rad_int = RadonTypes::Integer(RadonInteger::from(1));
        let rad_float = RadonTypes::Float(RadonFloat::from(1));

        let rad_rep_int = RadonReport::from_result(Ok(rad_int), &ReportContext::default());
        let rad_rep_float = RadonReport::from_result(Ok(rad_float), &ReportContext::default());

        let v = vec![
            rad_rep_float.clone(),
            rad_rep_int.clone(),
            rad_rep_float,
            rad_rep_int,
        ];
        let out = evaluate_tally_precondition_clause(v.clone(), 0.49, 4).unwrap_err();

        assert_eq!(
            out,
            RadError::ModeTie {
                values: RadonArray::from(
                    v.into_iter()
                        .map(RadonReport::into_inner)
                        .collect::<Vec<RadonTypes>>()
                ),
                max_count: 2,
            }
        );
    }

    #[test]
    fn test_tally_precondition_clause_3_errors_vs_2_ints_and_2_floats() {
        let rad_int = RadonTypes::Integer(RadonInteger::from(1));
        let rad_float = RadonTypes::Float(RadonFloat::from(1));
        let rad_err = RadonError::try_from(RadError::HttpStatus { status_code: 0 }).unwrap();

        let rad_rep_int = RadonReport::from_result(Ok(rad_int), &ReportContext::default());
        let rad_rep_float = RadonReport::from_result(Ok(rad_float), &ReportContext::default());
        let rad_rep_err = RadonReport::from_result(
            Ok(RadonTypes::RadonError(rad_err.clone())),
            &ReportContext::default(),
        );

        let v = vec![
            rad_rep_err.clone(),
            rad_rep_err.clone(),
            rad_rep_err,
            rad_rep_float.clone(),
            rad_rep_int.clone(),
            rad_rep_float,
            rad_rep_int,
        ];
        let tally_precondition_clause_result =
            evaluate_tally_precondition_clause(v, 0.40, 7).unwrap();

        if let TallyPreconditionClauseResult::MajorityOfErrors { errors_mode } =
            tally_precondition_clause_result
        {
            assert_eq!(errors_mode, rad_err);
        } else {
            panic!("The result of the tally precondition clause was not `MajorityOfErrors`. It was: {:?}", tally_precondition_clause_result);
        }
    }

    #[test]
    fn test_tally_precondition_clause_no_commits() {
        let v = vec![];
        let out = evaluate_tally_precondition_clause(v, 0.51, 0).unwrap_err();

        assert_eq!(out, RadError::InsufficientCommits);
    }

    #[test]
    fn test_tally_precondition_clause_no_reveals() {
        let v = vec![];
        let out = evaluate_tally_precondition_clause(v, 0.51, 1).unwrap_err();

        assert_eq!(out, RadError::NoReveals);
    }

    #[test]
    fn test_tally_precondition_clause_all_errors() {
        let rad_err = RadonError::try_from(RadError::HttpStatus { status_code: 0 }).unwrap();
        let rad_rep_err = RadonReport::from_result(
            Ok(RadonTypes::RadonError(rad_err.clone())),
            &ReportContext::default(),
        );

        let v = vec![
            rad_rep_err.clone(),
            rad_rep_err.clone(),
            rad_rep_err.clone(),
            rad_rep_err,
        ];
        let tally_precondition_clause_result =
            evaluate_tally_precondition_clause(v, 0.51, 4).unwrap();

        if let TallyPreconditionClauseResult::MajorityOfErrors { errors_mode } =
            tally_precondition_clause_result
        {
            assert_eq!(errors_mode, rad_err);
        } else {
            panic!("The result of the tally precondition clause was not `MajorityOfErrors`. It was: {:?}", tally_precondition_clause_result);
        }
    }

    #[test]
    fn test_tally_precondition_clause_insufficient_consensus() {
        let rad_int = RadonTypes::Integer(RadonInteger::from(1));
        let rad_float = RadonTypes::Float(RadonFloat::from(1));

        let rad_rep_int = RadonReport::from_result(Ok(rad_int), &ReportContext::default());
        let rad_rep_float = RadonReport::from_result(Ok(rad_float), &ReportContext::default());

        let v = vec![
            rad_rep_float.clone(),
            rad_rep_int.clone(),
            rad_rep_float,
            rad_rep_int,
        ];
        let out = evaluate_tally_precondition_clause(v, 0.51, 4).unwrap_err();

        assert_eq!(
            out,
            RadError::InsufficientConsensus {
                achieved: 0.5,
                required: 0.51
            }
        );
    }

    #[test]
    fn test_tally_precondition_clause_errors_insufficient_consensus() {
        // Two revealers that report two different errors result in `InsufficientConsensus`
        // because there is only 50% consensus (1/2)
        let rad_err1 = RadonError::try_from(RadError::HttpStatus { status_code: 0 }).unwrap();
        let rad_err2 = RadonError::try_from(RadError::RetrieveTimeout).unwrap();
        let rad_rep_err1 = RadonReport::from_result(
            Ok(RadonTypes::RadonError(rad_err1)),
            &ReportContext::default(),
        );
        let rad_rep_err2 = RadonReport::from_result(
            Ok(RadonTypes::RadonError(rad_err2)),
            &ReportContext::default(),
        );

        let v = vec![rad_rep_err1, rad_rep_err2];
        let out = evaluate_tally_precondition_clause(v, 0.51, 2).unwrap_err();

        assert_eq!(
            out,
            RadError::InsufficientConsensus {
                achieved: 0.5,
                required: 0.51
            }
        );
    }

    #[test]
    fn test_tally_precondition_clause_errors_mode_tie() {
        // Two revealers that report two different errors when min_consensus is below 50%
        // result in RadError::ModeTie
        let rad_err1 = RadonError::try_from(RadError::HttpStatus { status_code: 0 }).unwrap();
        let rad_err2 = RadonError::try_from(RadError::RetrieveTimeout).unwrap();
        let rad_rep_err1 = RadonReport::from_result(
            Ok(RadonTypes::RadonError(rad_err1)),
            &ReportContext::default(),
        );
        let rad_rep_err2 = RadonReport::from_result(
            Ok(RadonTypes::RadonError(rad_err2)),
            &ReportContext::default(),
        );

        let v = vec![rad_rep_err1, rad_rep_err2];
        let out = evaluate_tally_precondition_clause(v.clone(), 0.49, 2).unwrap_err();

        assert_eq!(
            out,
            RadError::ModeTie {
                values: RadonArray::from(
                    v.into_iter()
                        .map(RadonReport::into_inner)
                        .collect::<Vec<RadonTypes>>()
                ),
                max_count: 1,
            }
        );
    }

    #[test]
    fn vrf_sections() {
        let h0 = Hash::default();
        let h1 = Hash::with_first_u32(1);
        let h2 = Hash::with_first_u32(2);
        let h3 = Hash::with_first_u32(3);
        let a = VrfSlots::new(vec![]);
        assert_eq!(a.slot(&h0), 0);

        let a = VrfSlots::new(vec![h0]);
        assert_eq!(a.slot(&h0), 0);
        assert_eq!(a.slot(&h1), 1);

        let a = VrfSlots::new(vec![h0, h2]);
        assert_eq!(a.slot(&h0), 0);
        assert_eq!(a.slot(&h1), 1);
        assert_eq!(a.slot(&h2), 1);
        assert_eq!(a.slot(&h3), 2);
    }
}
