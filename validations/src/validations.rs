use std::{
    cmp::Ordering,
    collections::{HashMap, HashSet},
    convert::{TryFrom, TryInto},
    fmt, panic,
};

use itertools::Itertools;

use witnet_crypto::{
    hash::{calculate_sha256, Sha256},
    key::CryptoEngine,
    merkle::{merkle_tree_root as crypto_merkle_tree_root, ProgressiveMerkleTree},
    signature::{verify, PublicKey, Signature},
};
use witnet_data_structures::{
    chain::{
        Block, BlockMerkleRoots, CheckpointBeacon, CheckpointVRF, ConsensusConstants,
        DataRequestOutput, DataRequestStage, DataRequestState, Epoch, EpochConstants, Hash,
        Hashable, Input, KeyedSignature, OutputPointer, PublicKeyHash, RADRequest, RADTally,
        RADType, Reputation, ReputationEngine, SignaturesToVerify, ValueTransferOutput,
    },
    data_request::{
        calculate_tally_change, calculate_witness_reward,
        calculate_witness_reward_before_second_hard_fork, create_tally, DataRequestPool,
    },
    error::{BlockError, DataRequestError, TransactionError},
    mainnet_validations::ActiveWips,
    radon_report::{RadonReport, ReportContext},
    transaction::{
        CommitTransaction, DRTransaction, MintTransaction, RevealTransaction, TallyTransaction,
        Transaction, VTTransaction,
    },
    transaction_factory::{transaction_inputs_sum, transaction_outputs_sum},
    utxo_pool::{Diff, UnspentOutputsPool, UtxoDiff},
    vrf::{BlockEligibilityClaim, DataRequestEligibilityClaim, VrfCtx},
};
use witnet_rad::{
    conditions::{
        construct_report_from_clause_result, evaluate_tally_postcondition_clause,
        evaluate_tally_precondition_clause, radon_report_from_error,
    },
    error::RadError,
    operators::RadonOpCodes,
    script::{create_radon_script_from_filters_and_reducer, unpack_radon_script},
    types::{serial_iter_decode, RadonTypes},
};

/// Returns the fee of a value transfer transaction.
///
/// The fee is the difference between the outputs and the inputs
/// of the transaction. The pool parameter is used to find the
/// outputs pointed by the inputs and that contain the actual
/// their value.
pub fn vt_transaction_fee(
    vt_tx: &VTTransaction,
    utxo_diff: &UtxoDiff<'_>,
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
    utxo_diff: &UtxoDiff<'_>,
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
    utxo_diff: &UtxoDiff<'_>,
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
        let vt_output = utxo_diff.get(input.output_pointer()).ok_or_else(|| {
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
    initial_block_reward: u64,
    halving_period: u32,
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
    let block_reward_value = block_reward(mint_tx.epoch, initial_block_reward, halving_period);
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
pub fn validate_rad_request(
    rad_request: &RADRequest,
    active_wips: &ActiveWips,
) -> Result<(), failure::Error> {
    let retrieval_paths = &rad_request.retrieve;
    // If the data request has no sources to retrieve, it is set as invalid
    if retrieval_paths.is_empty() {
        return Err(DataRequestError::NoRetrievalSources.into());
    }

    for path in retrieval_paths {
        if active_wips.wip0020() {
            path.check_fields()?;
            unpack_radon_script(path.script.as_slice())?;

            // Regarding WIP-0019 activation:
            // Before -> Only RADType enum 0 position is valid
            // After -> Only RADType::HttpGet and RADType::Rng are valid
        } else if (!active_wips.wip0019() && path.kind != RADType::Unknown)
            || (active_wips.wip0019()
                && (path.kind != RADType::HttpGet && path.kind != RADType::Rng))
        {
            return Err(DataRequestError::InvalidRadType.into());
        } else {
            // This is before WIP-0020, so any fields introduced since then must be rejected
            path.check_fields_before_wip0020()?;
            let rad_script = unpack_radon_script(path.script.as_slice())?;

            // Scripts with new operators are invalid before TAPI activation
            for rad_call in rad_script {
                if rad_call.0 == RadonOpCodes::StringParseXMLMap {
                    return Err(RadError::UnknownOperator {
                        code: RadonOpCodes::StringParseXMLMap as i128,
                    }
                    .into());
                }
            }
        }
    }

    let aggregate = &rad_request.aggregate;
    let filters = aggregate.filters.as_slice();
    let reducer = aggregate.reducer;
    create_radon_script_from_filters_and_reducer(filters, reducer, active_wips)?;

    let consensus = &rad_request.tally;
    let filters = consensus.filters.as_slice();
    let reducer = consensus.reducer;
    create_radon_script_from_filters_and_reducer(filters, reducer, active_wips)?;

    Ok(())
}

/// Function to validate a value transfer transaction
pub fn validate_vt_transaction<'a>(
    vt_tx: &'a VTTransaction,
    utxo_diff: &UtxoDiff<'_>,
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
#[allow(clippy::too_many_arguments)]
pub fn validate_dr_transaction<'a>(
    dr_tx: &'a DRTransaction,
    utxo_diff: &UtxoDiff<'_>,
    epoch: Epoch,
    epoch_constants: EpochConstants,
    signatures_to_verify: &mut Vec<SignaturesToVerify>,
    collateral_minimum: u64,
    max_dr_weight: u32,
    active_wips: &ActiveWips,
) -> Result<(Vec<&'a Input>, Vec<&'a ValueTransferOutput>, u64), failure::Error> {
    if dr_tx.weight() > max_dr_weight {
        return Err(TransactionError::DataRequestWeightLimitExceeded {
            weight: dr_tx.weight(),
            max_weight: max_dr_weight,
            dr_output: dr_tx.body.dr_output.clone(),
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

    // A data request can only have 0 or 1 outputs
    if dr_tx.body.outputs.len() > 1 {
        return Err(TransactionError::WrongNumberOutputs {
            outputs: dr_tx.body.outputs.len(),
            expected_outputs: 1,
        }
        .into());
    }

    // A data request with 0 inputs can only be valid if the total cost of the data request is 0,
    // which is not possible
    if dr_tx.body.inputs.is_empty() {
        return Err(TransactionError::ZeroAmount.into());
    }

    let fee = dr_transaction_fee(dr_tx, utxo_diff, epoch, epoch_constants)?;

    if let Some(dr_output) = dr_tx.body.outputs.get(0) {
        // A value transfer output cannot have zero value
        if dr_output.value == 0 {
            return Err(TransactionError::ZeroValueOutput {
                tx_hash: dr_tx.hash(),
                output_id: 0,
            }
            .into());
        }

        // The output must have the same pkh as the first input
        let first_input = utxo_diff
            .get(dr_tx.body.inputs[0].output_pointer())
            .unwrap();
        let expected_pkh = first_input.pkh;

        if dr_output.pkh != expected_pkh {
            return Err(TransactionError::PublicKeyHashMismatch {
                expected_pkh,
                signature_pkh: dr_output.pkh,
            }
            .into());
        }
    } else {
        // 0 outputs: nothing to validate
    }

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

    validate_rad_request(&dr_tx.body.dr_output.data_request, active_wips)?;

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
    utxo_diff: &UtxoDiff<'_>,
    collateral_minimum: u64,
    collateral_age: u32,
    block_number: u32,
    minimum_reppoe_difficulty: u32,
    active_wips: &ActiveWips,
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

    // commit time_lock was disabled in the first hard fork
    if !active_wips.wip_0008() {
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
    let (target_hash, _) = calculate_reppoe_threshold(
        rep_eng,
        &pkh,
        num_witnesses,
        minimum_reppoe_difficulty,
        active_wips,
    );
    add_dr_vrf_signature_to_verify(
        signatures_to_verify,
        &co_tx.body.proof,
        vrf_input,
        co_tx.body.dr_pointer,
        target_hash,
    );

    // The commit fee here is the fee to include one commit
    Ok((
        dr_pointer,
        dr_output.witnesses,
        dr_output.commit_and_reveal_fee,
    ))
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
        .ok_or(TransactionError::CommitNotFound)?;

    if commit.body.commitment != reveal_signature.signature.hash() {
        return Err(TransactionError::MismatchedCommitment.into());
    }

    // The reveal fee here is the fee to include one reveal
    Ok(dr_state.data_request.commit_and_reveal_fee)
}

/// Execute RADON tally given a list of reveals.
/// If this function panics internally, the result will be set to RadError::Unknown.
pub fn run_tally_panic_safe(
    reveals: &[&RevealTransaction],
    tally: &RADTally,
    non_error_min: f64,
    commits_count: usize,
    active_wips: &ActiveWips,
) -> RadonReport<RadonTypes> {
    match panic::catch_unwind(|| {
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

        run_tally(results, tally, non_error_min, commits_count, active_wips)
    }) {
        Ok(x) => x,
        Err(_e) => {
            // If there is a panic during tally creation: set tally result to RadError::Unknown
            if active_wips.wips_0009_0011_0012() {
                radon_report_from_error(RadError::Unknown, reveals.len())
            } else {
                RadonReport::from_result(Err(RadError::Unknown), &ReportContext::default())
            }
        }
    }
}

/// Execute RADON tally given a list of reveals.
pub fn run_tally(
    results: Vec<RadonReport<RadonTypes>>,
    tally: &RADTally,
    non_error_min: f64,
    commits_count: usize,
    active_wips: &ActiveWips,
) -> RadonReport<RadonTypes> {
    let results_len = results.len();
    let clause_result =
        evaluate_tally_precondition_clause(results, non_error_min, commits_count, active_wips);
    let mut report =
        construct_report_from_clause_result(clause_result, tally, results_len, active_wips);
    if active_wips.wips_0009_0011_0012() {
        report = evaluate_tally_postcondition_clause(report, non_error_min, commits_count);
    }
    if active_wips.wip0018() {
        // If the result of a tally transaction is RadonError::UnhandledIntercept, this will
        // remove the message field, as specified in WIP0018.
        report
            .result
            .remove_message_from_error_unhandled_intercept();
    }

    report
}

fn create_expected_tally_transaction(
    ta_tx: &TallyTransaction,
    dr_pool: &DataRequestPool,
    collateral_minimum: u64,
    active_wips: &ActiveWips,
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
    let reveal_txns = dr_pool.get_reveals(&dr_pointer, active_wips).unwrap();
    let non_error_min = f64::from(dr_output.min_consensus_percentage) / 100.0;
    let committers = dr_state
        .info
        .commits
        .keys()
        .cloned()
        .collect::<HashSet<PublicKeyHash>>();
    let commit_length = committers.len();

    let report = run_tally_panic_safe(
        &reveal_txns,
        &dr_output.data_request.tally,
        non_error_min,
        commit_length,
        active_wips,
    );
    let ta_tx = create_tally(
        dr_pointer,
        dr_output,
        dr_state.pkh,
        &report,
        reveal_txns.into_iter().map(|tx| tx.body.pkh).collect(),
        committers,
        collateral_minimum,
        tally_bytes_on_encode_error(),
        active_wips,
    );

    Ok((ta_tx, dr_state.clone()))
}

/// This will be the returned error if the tally serialization fails
pub fn tally_bytes_on_encode_error() -> Vec<u8> {
    let radon_report_unknown_error: RadonReport<RadonTypes> =
        RadonReport::from_result(Err(RadError::Unknown), &ReportContext::default());
    Vec::try_from(&radon_report_unknown_error).unwrap()
}

/// Return (number_of_lies, number_of_errors) in `TallyTransaction`
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
    active_wips: &ActiveWips,
) -> Result<(Vec<&'a ValueTransferOutput>, u64), failure::Error> {
    let (expected_ta_tx, dr_state) =
        create_expected_tally_transaction(ta_tx, dr_pool, collateral_minimum, active_wips)?;

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
    #[cfg(test)]
    println!(
        "expected_ta_tx.tally: {}",
        hex::encode(&expected_ta_tx.tally)
    );

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
    let is_after_second_hard_fork = active_wips.wips_0009_0011_0012();
    let (reward, tally_extra_fee) = if is_after_second_hard_fork {
        calculate_witness_reward(
            commits_count,
            liars_count,
            errors_count,
            dr_state.data_request.witness_reward,
            collateral,
        )
    } else {
        calculate_witness_reward_before_second_hard_fork(
            commits_count,
            reveals_count,
            liars_count,
            errors_count,
            dr_state.data_request.witness_reward,
            collateral,
        )
    };
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
            if is_after_second_hard_fork {
                if honests_count > 0 {
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
                } else {
                    // Make sure every rewarded address is a committer
                    if dr_state.info.commits.get(&output.pkh).is_none() {
                        return Err(TransactionError::CommitNotFound.into());
                    }
                    // Validation of the reward, must be equal to the collateral
                    if output.value != collateral {
                        return Err(TransactionError::InvalidReward {
                            value: output.value,
                            expected_value: collateral,
                        }
                        .into());
                    }
                }
            } else {
                // Old logic used before second hard fork
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
            }

            // Validation of a honest witness would not be rewarded more than once
            if pkh_rewarded.contains(&output.pkh) {
                return Err(TransactionError::MultipleRewards { pkh: output.pkh }.into());
            }
            pkh_rewarded.insert(output.pkh);
        }

        if is_after_second_hard_fork && output.time_lock != 0 {
            return Err(TransactionError::InvalidTimeLock {
                current: output.time_lock,
                expected: 0,
            }
            .into());
        }
        total_tally_value += output.value;
    }

    let expected_collateral_value = if commits_count > 0 {
        collateral * u64::from(dr_state.data_request.witnesses)
    } else {
        // In case of no commits, collateral does not affect
        0
    };
    let expected_dr_value = dr_state.data_request.checked_total_value()?;
    let found_dr_value = dr_state.info.commits.len() as u64
        * dr_state.data_request.commit_and_reveal_fee
        + dr_state.info.reveals.len() as u64 * dr_state.data_request.commit_and_reveal_fee
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

    Ok((ta_tx.outputs.iter().collect(), tally_extra_fee))
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
    utxo_diff: &UtxoDiff<'_>,
) -> Result<(), failure::Error> {
    let output = utxo_diff.get(input.output_pointer());
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
    utxo_set: &UtxoDiff<'_>,
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

/// Update UTXO diff with the provided inputs and outputs
pub fn update_utxo_diff(
    utxo_diff: &mut UtxoDiff<'_>,
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
    active_wips: &ActiveWips,
) -> Result<Diff, failure::Error> {
    let epoch = block.block_header.beacon.checkpoint;
    let is_genesis = block.hash() == consensus_constants.genesis_hash;
    let mut utxo_diff = UtxoDiff::new(utxo_set, block_number);

    // Init total fee
    let mut total_fee = 0;
    // When validating genesis block, keep track of total value created
    // The value created in the genesis block cannot be greater than 2^64 - the total block reward,
    // So the total amount is always representable by a u64
    let max_total_value_genesis = u64::max_value()
        - total_block_reward(
            consensus_constants.initial_block_reward,
            consensus_constants.halving_period,
        );
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

    // Validate commit transactions in a block
    let mut co_mt = ProgressiveMerkleTree::sha256();
    let mut commits_number = HashMap::new();
    let block_beacon = block.block_header.beacon;
    let mut commit_hs = HashSet::with_capacity(block.txns.commit_txns.len());
    for transaction in &block.txns.commit_txns {
        let (dr_pointer, dr_witnesses, fee) = validate_commit_transaction(
            transaction,
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
            consensus_constants.minimum_difficulty,
            active_wips,
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
        let fee = validate_reveal_transaction(transaction, dr_pool, signatures_to_verify)?;

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
    let mut expected_tally_ready_drs = dr_pool.get_tally_ready_drs();
    for transaction in &block.txns.tally_txns {
        let (outputs, fee) = validate_tally_transaction(
            transaction,
            dr_pool,
            consensus_constants.collateral_minimum,
            active_wips,
        )?;

        if !active_wips.wips_0009_0011_0012() && transaction.tally == tally_bytes_on_encode_error()
        {
            // Before the second hard fork, do not allow RadError::Unknown as tally result
            return Err(TransactionError::MismatchedConsensus {
                expected_tally: tally_bytes_on_encode_error(),
                miner_tally: tally_bytes_on_encode_error(),
            }
            .into());
        }

        // Remove tally created from expected
        expected_tally_ready_drs.remove(&transaction.dr_pointer);

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

    // All data requests for which we expected tally transactions should have been removed
    // upon creation of the tallies. If not, block is invalid due to missing expected tallies
    if !expected_tally_ready_drs.is_empty() {
        return Err(BlockError::MissingExpectedTallies {
            count: expected_tally_ready_drs.len(),
            block_hash: block.hash(),
        }
        .into());
    }

    let mut dr_weight: u32 = 0;
    if active_wips.wip_0008() {
        // Calculate data request not solved weight
        let mut dr_pointers: HashSet<Hash> = dr_pool
            .get_dr_output_pointers_by_epoch(epoch)
            .into_iter()
            .collect();
        for dr in commits_number.keys() {
            dr_pointers.remove(dr);
        }
        for dr in dr_pointers {
            let unsolved_dro = dr_pool.get_dr_output(&dr);
            if let Some(dro) = unsolved_dro {
                dr_weight = dr_weight
                    .saturating_add(dro.weight())
                    .saturating_add(dro.extra_weight());
            }
        }
    }

    // Validate data request transactions in a block
    let mut dr_mt = ProgressiveMerkleTree::sha256();
    for transaction in &block.txns.data_request_txns {
        let (inputs, outputs, fee) = validate_dr_transaction(
            transaction,
            &utxo_diff,
            epoch,
            epoch_constants,
            signatures_to_verify,
            consensus_constants.collateral_minimum,
            consensus_constants.max_dr_weight,
            active_wips,
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

    if !is_genesis {
        // Validate mint
        validate_mint_transaction(
            &block.txns.mint,
            total_fee,
            block_beacon.checkpoint,
            consensus_constants.initial_block_reward,
            consensus_constants.halving_period,
        )?;

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
    active_wips: &ActiveWips,
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
            block_epoch,
            consensus_constants.minimum_difficulty,
            consensus_constants.epochs_with_minimum_difficulty,
            active_wips,
        );

        add_block_vrf_signature_to_verify(
            signatures_to_verify,
            &block.block_header.proof,
            vrf_input,
            target_hash,
        );

        validate_block_signature(block, signatures_to_verify)
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
    minimum_reppoe_difficulty: u32,
    active_wips: &ActiveWips,
) -> Result<u64, failure::Error> {
    let utxo_diff = UtxoDiff::new(unspent_outputs_pool, block_number);

    match transaction {
        Transaction::ValueTransfer(tx) => validate_vt_transaction(
            tx,
            &utxo_diff,
            current_epoch,
            epoch_constants,
            signatures_to_verify,
            max_vt_weight,
        )
        .map(|(_, _, fee)| fee),

        Transaction::DataRequest(tx) => validate_dr_transaction(
            tx,
            &utxo_diff,
            current_epoch,
            epoch_constants,
            signatures_to_verify,
            collateral_minimum,
            max_dr_weight,
            active_wips,
        )
        .map(|(_, _, fee)| fee),
        Transaction::Commit(tx) => validate_commit_transaction(
            tx,
            data_request_pool,
            vrf_input,
            signatures_to_verify,
            reputation_engine,
            current_epoch,
            epoch_constants,
            &utxo_diff,
            collateral_minimum,
            collateral_age,
            block_number,
            minimum_reppoe_difficulty,
            active_wips,
        )
        .map(|(_, _, fee)| fee),
        Transaction::Reveal(tx) => {
            validate_reveal_transaction(tx, data_request_pool, signatures_to_verify)
        }
        _ => Err(TransactionError::NotValidTransaction.into()),
    }
}

/// Calculate the target hash needed to create a valid VRF proof of eligibility used for block
/// mining.
pub fn calculate_randpoe_threshold(
    total_identities: u32,
    replication_factor: u32,
    block_epoch: u32,
    minimum_difficulty: u32,
    epochs_with_minimum_difficulty: u32,
    active_wips: &ActiveWips,
) -> (Hash, f64) {
    let max = u64::max_value();
    let minimum_difficulty = std::cmp::max(1, minimum_difficulty);
    let target = if block_epoch <= epochs_with_minimum_difficulty {
        max / u64::from(minimum_difficulty)
    } else if active_wips.wips_0009_0011_0012() {
        let difficulty = std::cmp::max(total_identities, minimum_difficulty);
        (max / u64::from(difficulty)).saturating_mul(u64::from(replication_factor))
    } else {
        let difficulty = std::cmp::max(1, total_identities);
        (max / u64::from(difficulty)).saturating_mul(u64::from(replication_factor))
    };
    let target = u32::try_from(target >> 32).unwrap();

    let probability = f64::from(target) / f64::from(u32::try_from(max >> 32).unwrap());
    (Hash::with_first_u32(target), probability)
}

/// Calculate the target hash needed to create a valid VRF proof of eligibility used for data
/// request witnessing.
pub fn calculate_reppoe_threshold(
    rep_eng: &ReputationEngine,
    pkh: &PublicKeyHash,
    num_witnesses: u16,
    minimum_difficulty: u32,
    active_wips: &ActiveWips,
) -> (Hash, f64) {
    // Set minimum total_active_reputation to 1 to avoid division by zero
    let total_active_rep = std::cmp::max(rep_eng.total_active_reputation(), 1);
    // Add 1 to reputation because otherwise a node with 0 reputation would
    // never be eligible for a data request
    let my_eligibility = u64::from(rep_eng.get_eligibility(pkh)) + 1;

    let max = u64::max_value();
    // Compute target eligibility and hard-cap it if required
    let target = if active_wips.wip0016() {
        let factor = u64::from(num_witnesses);
        (max / std::cmp::max(total_active_rep, u64::from(minimum_difficulty)))
            .saturating_mul(my_eligibility)
            .saturating_mul(factor)
    } else if active_wips.third_hard_fork() {
        let factor = u64::from(rep_eng.threshold_factor(num_witnesses));
        // Eligibility must never be greater than (max/minimum_difficulty)
        std::cmp::min(
            max / u64::from(minimum_difficulty),
            (max / total_active_rep).saturating_mul(my_eligibility),
        )
        .saturating_mul(factor)
    } else {
        let factor = u64::from(rep_eng.threshold_factor(num_witnesses));
        // Check for overflow: when the probability is more than 100%, cap it to 100%
        (max / total_active_rep)
            .saturating_mul(my_eligibility)
            .saturating_mul(factor)
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
    /// Create new list of slots with the given target hashes.
    ///
    /// `target_hashes` must be sorted
    pub fn new(target_hashes: Vec<Hash>) -> Self {
        Self { target_hashes }
    }

    /// Create new list of slots with the given parameters
    pub fn from_rf(
        total_identities: u32,
        replication_factor: u32,
        backup_factor: u32,
        block_epoch: u32,
        minimum_difficulty: u32,
        epochs_with_minimum_difficulty: u32,
        active_wips: &ActiveWips,
    ) -> Self {
        Self::new(
            (replication_factor..=backup_factor)
                .map(|rf| {
                    calculate_randpoe_threshold(
                        total_identities,
                        rf,
                        block_epoch,
                        minimum_difficulty,
                        epochs_with_minimum_difficulty,
                        active_wips,
                    )
                    .0
                })
                .collect(),
        )
    }

    /// Return the slot number that contains the given hash
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

    /// Return the target hash for each slot
    pub fn target_hashes(&self) -> &[Hash] {
        &self.target_hashes
    }
}

#[allow(clippy::many_single_char_names)]
fn internal_calculate_mining_probability(
    rf: u32,
    n: f64,
    k: u32, // k: iterative rf until reach bf
    m: i32, // M: nodes with reputation greater than me
    l: i32, // L: nodes with reputation equal than me
    r: i32, // R: nodes with reputation less than me
) -> f64 {
    if k == rf {
        let rf = f64::from(rf);
        // Prob to mine is the probability that a node with the same reputation than me mine,
        // divided by all the nodes with the same reputation:
        // 1/L * (1 - ((N-RF)/N)^L)
        let prob_to_mine = (1.0 / f64::from(l)) * (1.0 - ((n - rf) / n).powi(l));
        // Prob that a node with more reputation than me mine is:
        // ((N-RF)/N)^M
        let prob_greater_neg = ((n - rf) / n).powi(m);

        prob_to_mine * prob_greater_neg
    } else {
        let k = f64::from(k);
        // Here we take into account that rf = 1 because is only a new slot
        let prob_to_mine = (1.0 / f64::from(l)) * (1.0 - ((n - 1.0) / n).powi(l));
        // The same equation than before
        let prob_bigger_neg = ((n - k) / n).powi(m);
        // Prob that a node with less or equal reputation than me mine with a lower slot is:
        // ((N+1-RF)/N)^(L+R-1)
        let prob_lower_slot_neg = ((n + 1.0 - k) / n).powi(l + r - 1);

        prob_to_mine * prob_bigger_neg * prob_lower_slot_neg
    }
}

/// Calculate the probability that the block candidate proposed by this identity will be the
/// consolidated block selected by the network.
pub fn calculate_mining_probability(
    rep_engine: &ReputationEngine,
    own_pkh: PublicKeyHash,
    rf: u32,
    bf: u32,
) -> f64 {
    let n = u32::try_from(rep_engine.ars().active_identities_number()).unwrap();

    // In case of any active node, the probability is maximum
    if n == 0 {
        return 1.0;
    }

    // First we need to know how many nodes have more or equal reputation than us
    let own_rep = rep_engine.trs().get(&own_pkh);
    let is_active_node = rep_engine.ars().contains(&own_pkh);
    let mut greater = 0;
    let mut equal = 0;
    let mut less = 0;
    for &active_id in rep_engine.ars().active_identities() {
        let rep = rep_engine.trs().get(&active_id);
        match (rep.0 > 0, own_rep.0 > 0) {
            (true, false) => greater += 1,
            (false, true) => less += 1,
            _ => equal += 1,
        }
    }
    // In case of not being active, the equal value is plus 1.
    if !is_active_node {
        equal += 1;
    }

    if rf > n && greater == 0 {
        // In case of replication factor exceed the active node number and being the most reputed
        // we obtain the maximum probability divided in the nodes we share the same reputation
        1.0 / f64::from(equal)
    } else if rf > n && greater > 0 {
        // In case of replication factor exceed the active node number and not being the most reputed
        // we obtain the minimum probability
        0.0
    } else {
        let mut aux =
            internal_calculate_mining_probability(rf, f64::from(n), rf, greater, equal, less);
        let mut k = rf + 1;
        while k <= bf && k <= n {
            aux += internal_calculate_mining_probability(rf, f64::from(n), k, greater, equal, less);
            k += 1;
        }
        aux
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
pub const NANOWITS_PER_WIT: u64 = 1_000_000_000;
// 10 ^ WIT_DECIMAL_PLACES
/// Number of decimal places used in the string representation of wit value.
pub const WIT_DECIMAL_PLACES: u8 = 9;

/// Unit of value
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord)]
pub struct Wit(u64);

impl Wit {
    /// Create from wits
    pub fn from_wits(wits: u64) -> Self {
        Self(wits.checked_mul(NANOWITS_PER_WIT).expect("overflow"))
    }
    /// Create from nanowits
    pub fn from_nanowits(nanowits: u64) -> Self {
        Self(nanowits)
    }
    /// Return integer and fractional part, useful for pretty printing
    pub fn wits_and_nanowits(self) -> (u64, u64) {
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

/// Calculate the block mining reward.
/// Returns nanowits.
pub fn block_reward(epoch: Epoch, initial_block_reward: u64, halving_period: u32) -> u64 {
    let initial_reward: u64 = initial_block_reward;
    let halvings = epoch / halving_period;
    if halvings < 64 {
        initial_reward >> halvings
    } else {
        0
    }
}

/// Calculate the total amount of wits that will be rewarded to miners.
pub fn total_block_reward(initial_block_reward: u64, halving_period: u32) -> u64 {
    let mut total_reward = 0u64;
    let mut base_reward = initial_block_reward;
    while base_reward != 0 {
        let new_reward = base_reward
            .checked_mul(u64::from(halving_period))
            .expect("overflow");
        total_reward = total_reward.checked_add(new_reward).expect("overflow");
        base_reward >>= 1;
    }

    total_reward
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
#[allow(clippy::too_many_arguments)]
pub fn compare_block_candidates(
    b1_hash: Hash,
    b1_rep: Reputation,
    b1_vrf_hash: Hash,
    b1_is_active: bool,
    b2_hash: Hash,
    b2_rep: Reputation,
    b2_vrf_hash: Hash,
    b2_is_active: bool,
    s: &VrfSlots,
) -> Ordering {
    let section1 = s.slot(&b1_vrf_hash);
    let section2 = s.slot(&b2_vrf_hash);
    // Bigger section implies worse block candidate
    section1
        .cmp(&section2)
        .reverse()
        // Blocks created with nodes with reputation are better candidates than the others
        .then({
            match (b1_rep.0 > 0, b2_rep.0 > 0) {
                (true, false) => Ordering::Greater,
                (false, true) => Ordering::Less,
                _ => Ordering::Equal,
            }
        })
        // Blocks created with active nodes are better candidates than the others
        .then({
            match (b1_is_active, b2_is_active) {
                (true, false) => Ordering::Greater,
                (false, true) => Ordering::Less,
                _ => Ordering::Equal,
            }
        })
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
    use super::*;

    const INITIAL_BLOCK_REWARD: u64 = 250 * 1_000_000_000;
    const HALVING_PERIOD: u32 = 3_500_000;

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
        let reward = |epoch| block_reward(epoch, INITIAL_BLOCK_REWARD, HALVING_PERIOD);

        assert_eq!(reward(0), 250 * wit);
        assert_eq!(reward(1), 250 * wit);
        assert_eq!(reward(3_500_000 - 1), 250 * wit);
        assert_eq!(reward(3_500_000), 125 * wit);
        assert_eq!(reward((3_500_000 * 2) - 1), 125 * wit);
        assert_eq!(reward(3_500_000 * 2), (62.5 * wit as f64).floor() as u64);
        assert_eq!(reward(3_500_000 * 36), 3);
        assert_eq!(reward(3_500_000 * 37), 1);
        assert_eq!(reward(3_500_000 * 38), 0);
        assert_eq!(reward(3_500_000 * 63), 0);
        assert_eq!(reward(3_500_000 * 64), 0);
        assert_eq!(reward(3_500_000 * 65), 0);
        assert_eq!(reward(3_500_000 * 100), 0);
    }
}
