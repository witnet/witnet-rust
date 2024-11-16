use std::{
    cmp::Ordering,
    collections::{HashMap, HashSet},
    convert::{TryFrom, TryInto},
    panic,
};

use itertools::Itertools;

use witnet_config::defaults::PSEUDO_CONSENSUS_CONSTANTS_WIP0022_REWARD_COLLATERAL_RATIO;
use witnet_crypto::{
    hash::{calculate_sha256, Sha256},
    merkle::{merkle_tree_root as crypto_merkle_tree_root, ProgressiveMerkleTree},
    signature::{verify, PublicKey, Signature},
};
use witnet_data_structures::{
    chain::{
        tapi::ActiveWips, Block, BlockMerkleRoots, CheckpointBeacon, CheckpointVRF,
        ConsensusConstants, ConsensusConstantsWit2, DataRequestOutput, DataRequestStage,
        DataRequestState, Epoch, EpochConstants, Hash, Hashable, Input, KeyedSignature,
        OutputPointer, PublicKeyHash, RADRequest, RADTally, RADType, Reputation, ReputationEngine,
        SignaturesToVerify, StakeOutput, ValueTransferOutput,
    },
    data_request::{
        calculate_reward_collateral_ratio, calculate_tally_change, calculate_witness_reward,
        calculate_witness_reward_before_second_hard_fork, create_tally,
        data_request_has_too_many_witnesses, DataRequestPool,
    },
    error::{BlockError, DataRequestError, TransactionError},
    get_protocol_version,
    proto::versioning::{ProtocolVersion, VersionedHashable},
    radon_report::{RadonReport, ReportContext},
    staking::prelude::{Power, QueryStakesKey, StakeKey, StakesTracker},
    transaction::{
        CommitTransaction, DRTransaction, MintTransaction, RevealTransaction, StakeTransaction,
        TallyTransaction, Transaction, UnstakeTransaction, VTTransaction,
    },
    transaction_factory::{transaction_inputs_sum, transaction_outputs_sum},
    types::visitor::Visitor,
    utxo_pool::{Diff, UnspentOutputsPool, UtxoDiff},
    vrf::{BlockEligibilityClaim, DataRequestEligibilityClaim, VrfCtx},
    wit::NANOWITS_PER_WIT,
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

use crate::eligibility::{
    current::{
        Eligibility, Eligible,
        IneligibilityReason::{InsufficientPower, NotStaking},
    },
    legacy::*,
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

/// Returns the fee of a stake transaction.
///
/// The fee is the difference between the outputs and the inputs of the transaction.
pub fn st_transaction_fee(
    st_tx: &StakeTransaction,
    utxo_diff: &UtxoDiff<'_>,
    epoch: Epoch,
    epoch_constants: EpochConstants,
) -> Result<u64, failure::Error> {
    let in_value = transaction_inputs_sum(&st_tx.body.inputs, utxo_diff, epoch, epoch_constants)?;
    let out_value = st_tx.body.output.value;
    let change_value = match &st_tx.body.change {
        Some(change) => change.value,
        None => 0,
    };

    if out_value + change_value > in_value {
        Err(TransactionError::NegativeFee.into())
    } else {
        Ok(in_value - out_value - change_value)
    }
}

/// Returns the fee of a unstake transaction.
///
/// The fee is suplied as part of the unstake transaction
/// We check that the staked amount is greater than the
/// requested unstake amount plus the fee
pub fn ut_transaction_fee(
    ut_tx: &UnstakeTransaction,
    staked_amount: u64,
) -> Result<u64, failure::Error> {
    let out_value = ut_tx.body.value();
    let fee_value = ut_tx.body.fee;

    if out_value + fee_value > staked_amount {
        Err(TransactionError::NegativeFee.into())
    } else {
        Ok(fee_value)
    }
}

/// Returns the fee of a data request transaction.
///
/// The fee is the difference between the outputs (with the data request value)
/// and the inputs of the transaction. The pool parameter is used to find the
/// outputs pointed by the inputs and that contain the actual
/// their value.
#[allow(clippy::too_many_arguments)]
pub fn validate_commit_collateral(
    co_tx: &CommitTransaction,
    utxo_diff: &UtxoDiff<'_>,
    epoch: Epoch,
    epoch_constants: EpochConstants,
    required_collateral: u64,
    block_number: u32,
    collateral_age: u32,
    superblock_period: u16,
    protocol_version: ProtocolVersion,
    stakes: &StakesTracker,
    min_stake: u64,
) -> Result<(), failure::Error> {
    let block_number_limit = block_number.saturating_sub(collateral_age);
    let commit_pkh = co_tx.body.proof.proof.pkh();
    let mut in_value: u64 = 0;
    let mut seen_output_pointers = HashSet::with_capacity(co_tx.body.collateral.len());
    let qualification_requirement = 100 * NANOWITS_PER_WIT;

    // Validate commit collateral value in wit/2
    if protocol_version >= ProtocolVersion::V2_0 {
        // TODO: modify this to enable delegated staking with multiple withdrawer addresses on a single validator
        let validator_balance: u64 = stakes
            .query_stakes(QueryStakesKey::Validator(commit_pkh))
            .unwrap_or_default()
            .first()
            .map(|stake| stake.value.coins)
            .unwrap()
            .into();
        if validator_balance < min_stake + required_collateral {
            return Err(TransactionError::CollateralBelowMinimumStake {
                collateral: required_collateral,
                validator: commit_pkh,
            }
            .into());
        }
    }

    for input in &co_tx.body.collateral {
        let vt_output = utxo_diff.get(input.output_pointer()).ok_or_else(|| {
            TransactionError::OutputNotFound {
                output: *input.output_pointer(),
            }
        })?;

        // Special requirement for facilitating the 2.0 transition.
        // Every committer is required to have a total balance of at least 100 wits.
        // This works independently from the minimum collateral requirement.
        if protocol_version < ProtocolVersion::V2_0 && epoch > 2_245_000 {
            let committer = vt_output.pkh;
            let mut balance = 0;
            utxo_diff.get_utxo_set().visit_with_pkh(
                committer,
                |_| (),
                |(output_pointer, (vto, _))| {
                    if let Some(utxo_block_number) =
                        utxo_diff.included_in_block_number(output_pointer)
                    {
                        if utxo_block_number
                            < block_number.saturating_sub((2 * superblock_period).into())
                        {
                            balance += vto.value
                        }
                    }
                },
            );

            if balance < qualification_requirement {
                return Err(TransactionError::UnqualifiedCommitter {
                    committer,
                    required: qualification_requirement,
                    current: balance,
                }
                .into());
            }
        }

        // The inputs used as collateral do not need any additional signatures
        // as long as the commit transaction is signed by the same public key
        // So check that the public key matches
        if vt_output.pkh != commit_pkh {
            return Err(TransactionError::CollateralPkhMismatch {
                output: *input.output_pointer(),
                output_pkh: vt_output.pkh,
                proof_pkh: commit_pkh,
            }
            .into());
        }

        // Verify that commits are only accepted after the time lock expired
        let (epoch_timestamp, _) = epoch_constants.epoch_timestamp(epoch)?;
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
                output: *input.output_pointer(),
                must_be_older_than: collateral_age,
                found: block_number - included_in_block_number,
            }
            .into());
        }

        if !seen_output_pointers.insert(input.output_pointer()) {
            // If the set already contained this output pointer
            return Err(TransactionError::OutputNotFound {
                output: *input.output_pointer(),
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
        if protocol_version != ProtocolVersion::V2_0 {
            Err(TransactionError::IncorrectCollateral {
                expected: required_collateral,
                found: found_collateral,
            }
            .into())
        } else if found_collateral == 0 {
            Ok(())
        } else {
            Err(TransactionError::IncorrectCollateral {
                expected: 0,
                found: found_collateral,
            }
            .into())
        }
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

    if ProtocolVersion::from_epoch(block_epoch) != ProtocolVersion::V2_0 {
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
    } else {
        let mut valid_mint_tx = MintTransaction::default();
        valid_mint_tx.epoch = block_epoch;
        if *mint_tx != valid_mint_tx {
            return Err(BlockError::InvalidMintTransaction.into());
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
    protocol_version: Option<ProtocolVersion>,
) -> Result<(Vec<&'a Input>, Vec<&'a ValueTransferOutput>, u64), failure::Error> {
    let protocol_version = protocol_version.unwrap_or_default();
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
        vt_tx.versioned_hash(protocol_version),
        utxo_diff,
        signatures_to_verify,
    )?;

    // A value transfer transaction must have at least one input
    if vt_tx.body.inputs.is_empty() {
        return Err(TransactionError::NoInputs {
            tx_hash: vt_tx.versioned_hash(protocol_version),
        }
        .into());
    }

    // A value transfer output cannot have zero value
    for (idx, output) in vt_tx.body.outputs.iter().enumerate() {
        if output.value == 0 {
            return Err(TransactionError::ZeroValueOutput {
                tx_hash: vt_tx.versioned_hash(protocol_version),
                output_id: idx,
            }
            .into());
        }
    }

    let fee = vt_transaction_fee(vt_tx, utxo_diff, epoch, epoch_constants)?;

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
    required_reward_collateral_ratio: u64,
    active_wips: &ActiveWips,
    protocol_version: Option<ProtocolVersion>,
) -> Result<(Vec<&'a Input>, Vec<&'a ValueTransferOutput>, u64), failure::Error> {
    if dr_tx.weight() > max_dr_weight {
        return Err(TransactionError::DataRequestWeightLimitExceeded {
            weight: dr_tx.weight(),
            max_weight: max_dr_weight,
            dr_output: Box::new(dr_tx.body.dr_output.clone()),
        }
        .into());
    }
    let protocol_version = protocol_version.unwrap_or_default();

    validate_transaction_signature(
        &dr_tx.signatures,
        &dr_tx.body.inputs,
        dr_tx.versioned_hash(protocol_version),
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

    if let Some(dr_output) = dr_tx.body.outputs.first() {
        // A value transfer output cannot have zero value
        if dr_output.value == 0 {
            return Err(TransactionError::ZeroValueOutput {
                tx_hash: dr_tx.versioned_hash(protocol_version),
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

    validate_data_request_output(
        &dr_tx.body.dr_output,
        collateral_minimum,
        required_reward_collateral_ratio,
        active_wips,
    )?;

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
///  - The reward to collateral ratio is greater than 1/125
pub fn validate_data_request_output(
    request: &DataRequestOutput,
    collateral_minimum: u64,
    required_reward_collateral_ratio: u64,
    active_wips: &ActiveWips,
) -> Result<(), TransactionError> {
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

    if active_wips.wip0022() {
        let reward_collateral_ratio = calculate_reward_collateral_ratio(
            request.collateral,
            collateral_minimum,
            request.witness_reward,
        );
        if reward_collateral_ratio > required_reward_collateral_ratio {
            return Err(TransactionError::RewardTooLow {
                reward_collateral_ratio,
                required_reward_collateral_ratio,
            });
        }
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
    superblock_period: u16,
    protocol_version: ProtocolVersion,
    stakes: &StakesTracker,
    max_rounds: u16,
    min_stake: u64,
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

    let proof_pkh = co_tx.body.proof.proof.pkh();

    // Check if the commit transaction is from an eligible validator
    let target_hash_wit2 = if protocol_version >= ProtocolVersion::V2_0 {
        match stakes.witnessing_eligibility(
            proof_pkh,
            epoch,
            dr_state.data_request.witnesses,
            dr_state.info.current_commit_round,
            max_rounds,
        ) {
            Ok((eligibility, target_hash, _)) => {
                if matches!(eligibility, Eligible::No(_)) {
                    return Err(TransactionError::ValidatorNotEligible {
                        validator: proof_pkh,
                    }
                    .into());
                }

                target_hash
            }
            Err(e) => return Err(e.into()),
        }
    } else {
        Hash::min()
    };

    // Commitment's output is only for change propose, so it only has to be one output and the
    // address has to be the same than the address which creates the commitment
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
        superblock_period,
        protocol_version,
        stakes,
        min_stake,
    )?;

    // commit time_lock was disabled in the first hard fork
    if !active_wips.wip_0008() {
        // Verify that commits are only accepted after the time lock expired
        let (epoch_timestamp, _) = epoch_constants.epoch_timestamp(epoch)?;
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
    let target_hash = if protocol_version < ProtocolVersion::V2_0 {
        let (target_hash, _) = calculate_reppoe_threshold(
            rep_eng,
            &pkh,
            num_witnesses,
            minimum_reppoe_difficulty,
            active_wips,
        );

        target_hash
    } else {
        target_hash_wit2
    };
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
    too_many_witnesses: bool,
) -> RadonReport<RadonTypes> {
    let unwind_fn = || {
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
            active_wips,
        );

        run_tally(
            results,
            tally,
            non_error_min,
            commits_count,
            active_wips,
            too_many_witnesses,
        )
    };

    match panic::catch_unwind(unwind_fn) {
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
    too_many_witnesses: bool,
) -> RadonReport<RadonTypes> {
    let results_len = results.len();
    let clause_result = evaluate_tally_precondition_clause(
        results,
        non_error_min,
        commits_count,
        active_wips,
        too_many_witnesses,
    );
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
    too_many_witnesses: bool,
    epoch: Option<Epoch>,
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
        too_many_witnesses,
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
        get_protocol_version(epoch),
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
    validator_count: Option<usize>,
    epoch: Option<Epoch>,
) -> Result<(Vec<&'a ValueTransferOutput>, u64), failure::Error> {
    let validator_count =
        validator_count.unwrap_or(witnet_data_structures::DEFAULT_VALIDATOR_COUNT_FOR_TESTS);
    let too_many_witnesses;
    if let Some(dr_state) = dr_pool.data_request_state(&ta_tx.dr_pointer) {
        too_many_witnesses =
            data_request_has_too_many_witnesses(&dr_state.data_request, validator_count, epoch);
    } else {
        return Err(TransactionError::DataRequestNotFound {
            hash: ta_tx.dr_pointer,
        }
        .into());
    }
    let (expected_ta_tx, dr_state) = create_expected_tally_transaction(
        ta_tx,
        dr_pool,
        collateral_minimum,
        active_wips,
        too_many_witnesses,
        epoch,
    )?;

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
    #[cfg(test)]
    println!("ta_tx.tally:          {}", hex::encode(&ta_tx.tally));

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
            active_wips.wip0023(),
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
                    if !dr_state.info.reveals.contains_key(&output.pkh) {
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
                    if !dr_state.info.commits.contains_key(&output.pkh) {
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
                    if !dr_state.info.reveals.contains_key(&output.pkh) {
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
        if active_wips.wip0023() {
            if honests_count > 0 {
                collateral * (u64::from(dr_state.data_request.witnesses) - liars_count as u64)
            } else {
                collateral * u64::from(dr_state.data_request.witnesses)
            }
        } else {
            collateral * u64::from(dr_state.data_request.witnesses)
        }
    } else {
        // In case of no commits, collateral does not affect
        0
    };
    // TODO: should we somehow validate that the total data request reward is correctly refunded + added to the staked balance?
    if get_protocol_version(epoch) < ProtocolVersion::V2_0 {
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
    }

    Ok((ta_tx.outputs.iter().collect(), tally_extra_fee))
}

/// A type alias for the very complex return type of `fn validate_stake_transaction`.
pub type ValidatedStakeTransaction<'a> = (
    Vec<&'a Input>,
    &'a StakeOutput,
    u64,
    u32,
    &'a Option<ValueTransferOutput>,
);

/// Function to validate a stake transaction.
#[allow(clippy::too_many_arguments)]
pub fn validate_stake_transaction<'a>(
    st_tx: &'a StakeTransaction,
    utxo_diff: &UtxoDiff<'_>,
    epoch: Epoch,
    epoch_constants: EpochConstants,
    signatures_to_verify: &mut Vec<SignaturesToVerify>,
    stakes: &StakesTracker,
    min_stake_nanowits: u64,
    max_stake_nanowits: u64,
) -> Result<ValidatedStakeTransaction<'a>, failure::Error> {
    if get_protocol_version(Some(epoch)) == ProtocolVersion::V1_7 {
        return Err(TransactionError::NoStakeTransactionsAllowed.into());
    }

    // Check that the amount of coins to stake is equal or greater than the minimum allowed
    if st_tx.body.output.value < min_stake_nanowits {
        Err(TransactionError::StakeBelowMinimum {
            min_stake: min_stake_nanowits,
            stake: st_tx.body.output.value,
        })?;
    }

    // A stake transaction can only stake on an existing validator if the withdrawer address is the same
    match stakes.check_validator_withdrawer(
        st_tx.body.output.key.validator,
        st_tx.body.output.key.withdrawer,
    ) {
        Ok(_) => (),
        Err(_) => {
            return Err(TransactionError::NoStakeFound {
                validator: st_tx.body.output.key.validator,
                withdrawer: st_tx.body.output.key.withdrawer,
            }
            .into());
        }
    }

    // Check that the amount of coins to stake plus the alread staked amount is equal or smaller than the maximum allowed
    let stakes_key = QueryStakesKey::Key(StakeKey {
        validator: st_tx.body.output.key.validator,
        withdrawer: st_tx.body.output.key.withdrawer,
    });
    match stakes.query_stakes(stakes_key) {
        Ok(stake_entry) => {
            // TODO: modify this to enable delegated staking with multiple withdrawer addresses on a single validator
            let staked_amount: u64 = stake_entry
                .first()
                .map(|stake| stake.value.coins)
                .unwrap()
                .into();
            if staked_amount + st_tx.body.output.value > max_stake_nanowits {
                Err(TransactionError::StakeAboveMaximum {
                    max_stake: max_stake_nanowits,
                    stake: staked_amount + st_tx.body.output.value,
                })?;
            }
        }
        Err(_) => {
            // Check that the amount of coins to stake is equal or smaller than the maximum allowed
            if st_tx.body.output.value > max_stake_nanowits {
                Err(TransactionError::StakeAboveMaximum {
                    max_stake: max_stake_nanowits,
                    stake: st_tx.body.output.value,
                })?;
            }
        }
    };

    validate_transaction_signature(
        &st_tx.signatures,
        &st_tx.body.inputs,
        st_tx.hash(),
        utxo_diff,
        signatures_to_verify,
    )?;

    // A stake transaction must have at least one input
    if st_tx.body.inputs.is_empty() {
        Err(TransactionError::NoInputs {
            tx_hash: st_tx.hash(),
        })?;
    }

    let fee = st_transaction_fee(st_tx, utxo_diff, epoch, epoch_constants)?;

    Ok((
        st_tx.body.inputs.iter().collect(),
        &st_tx.body.output,
        fee,
        st_tx.weight(),
        &st_tx.body.change,
    ))
}

/// Function to validate a unstake transaction
pub fn validate_unstake_transaction<'a>(
    ut_tx: &'a UnstakeTransaction,
    epoch: Epoch,
    stakes: &StakesTracker,
    min_stake_nanowits: u64,
    unstake_delay: u64,
) -> Result<(u64, u32, Vec<&'a ValueTransferOutput>), failure::Error> {
    if get_protocol_version(Some(epoch)) <= ProtocolVersion::V1_8 {
        return Err(TransactionError::NoUnstakeTransactionsAllowed.into());
    }

    // Check if is unstaking more than the total stake
    let amount_to_unstake = ut_tx.body.value() + ut_tx.body.fee;

    let validator = ut_tx.body.operator;
    let withdrawer = ut_tx.signature.public_key.pkh();
    let stakes_key = QueryStakesKey::Key(StakeKey {
        validator,
        withdrawer,
    });
    let staked_amount = match stakes.query_stakes(stakes_key) {
        Ok(stake_entry) => {
            // TODO: modify this to enable delegated staking with multiple withdrawer addresses on a single validator
            let staked_amount = stake_entry
                .first()
                .map(|stake| stake.value.coins)
                .unwrap()
                .into();
            if amount_to_unstake > staked_amount {
                return Err(TransactionError::UnstakingMoreThanStaked {
                    unstake: amount_to_unstake,
                    stake: staked_amount,
                }
                .into());
            }

            // TODO: modify this to enable delegated staking with multiple withdrawer addresses on a single validator
            let nonce = stake_entry.first().map(|stake| stake.value.nonce).unwrap();
            if ut_tx.body.nonce != nonce {
                return Err(TransactionError::UnstakeInvalidNonce {
                    used: ut_tx.body.nonce,
                    current: nonce,
                }
                .into());
            }

            staked_amount
        }
        Err(_) => {
            return Err(TransactionError::NoStakeFound {
                validator,
                withdrawer,
            }
            .into());
        }
    };

    // Allowed unstake actions:
    // 1) Unstake the full balance (checked by the first condition)
    // 2) Unstake an amount such that the leftover staked amount is greater than the min allowed
    if staked_amount - amount_to_unstake > 0
        && staked_amount - amount_to_unstake < min_stake_nanowits
    {
        return Err(TransactionError::StakeBelowMinimum {
            min_stake: min_stake_nanowits,
            stake: staked_amount,
        }
        .into());
    }

    // Validate unstake timestamp
    validate_unstake_timelock(ut_tx, unstake_delay)?;

    // validate unstake_signature
    validate_unstake_signature(ut_tx, validator)?;

    let fee = ut_transaction_fee(ut_tx, staked_amount)?;
    let weight = ut_tx.weight();

    Ok((fee, weight, vec![&ut_tx.body.withdrawal]))
}

/// Validate unstake timelock
pub fn validate_unstake_timelock(
    ut_tx: &UnstakeTransaction,
    unstake_delay: u64,
) -> Result<(), failure::Error> {
    if ut_tx.body.withdrawal.time_lock < unstake_delay {
        return Err(TransactionError::InvalidUnstakeTimelock {
            time_lock: ut_tx.body.withdrawal.time_lock,
            unstaking_delay_seconds: unstake_delay,
        }
        .into());
    }

    Ok(())
}

/// Function to validate a unstake authorization
pub fn validate_unstake_signature(
    ut_tx: &UnstakeTransaction,
    operator: PublicKeyHash,
) -> Result<(), failure::Error> {
    let ut_tx_pkh = ut_tx.signature.public_key.pkh();
    if ut_tx_pkh != ut_tx.body.withdrawal.pkh {
        return Err(TransactionError::InvalidUnstakeSignature {
            signature: ut_tx_pkh,
            withdrawal: ut_tx.body.withdrawal.pkh,
            operator,
        }
        .into());
    }

    // Validate message body and signature
    let Hash::SHA256(message) = ut_tx.hash();

    let fte = |e: failure::Error| TransactionError::VerifyTransactionSignatureFail {
        hash: ut_tx.hash(),
        msg: e.to_string(),
    };

    let signature = ut_tx.signature.signature.clone().try_into().map_err(fte)?;
    let public_key = ut_tx.signature.public_key.clone().try_into().map_err(fte)?;

    verify(&public_key, message.as_ref(), &signature).map_err(|e| {
        TransactionError::VerifyTransactionSignatureFail {
            hash: {
                let mut sha256 = [0; 32];
                sha256.copy_from_slice(message.as_ref());
                Hash::SHA256(sha256)
            },
            msg: e.to_string(),
        }
    })?;

    Ok(())
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

    let Hash::SHA256(message) = block.versioned_hash(get_protocol_version(Some(
        block.block_header.beacon.checkpoint,
    )));

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
        .first()
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

/// Update UTXO diff with the provided inputs and outputs.
///
/// If all the inputs have the same PKH, the outputs with that PKH will keep the maximum
/// (most recent) block number of the inputs. Outputs with different PKH have the block number reset.
/// And in case one of the inputs has a different PKH from another input, the block number of all the outputs is reset.
pub fn update_utxo_diff<'a, IterInputs, IterOutputs>(
    utxo_diff: &mut UtxoDiff<'_>,
    inputs: IterInputs,
    outputs: IterOutputs,
    tx_hash: Hash,
    epoch: Epoch,
    checkpoint_zero_timestamp: i64,
) where
    IterInputs: IntoIterator<Item = &'a Input>,
    IterOutputs: IntoIterator<Item = &'a ValueTransferOutput>,
{
    let mut input_pkh = None;
    let mut block_number_input = 0;
    let mut first = true;

    for input in inputs {
        // Obtain the OutputPointer of each input and remove it from the utxo_diff
        let output_pointer = input.output_pointer();

        if !first && input_pkh.is_none() {
            // No need to check PKH and block number, there is more than one distinct input PKH
        } else if !first && input_pkh != utxo_diff.get(output_pointer).map(|vt| vt.pkh) {
            // PKH of this input is different from the previous input, no need to check block number
            input_pkh = None;
        } else {
            // All inputs up until this one have the same PKH
            if first {
                first = false;
                // Store the PKH of the first element
                input_pkh = utxo_diff.get(output_pointer).map(|vt| vt.pkh);
            }
            // Update block number to max of inputs
            let block_number = utxo_diff
                .included_in_block_number(output_pointer)
                .unwrap_or(0);
            block_number_input = std::cmp::max(block_number, block_number_input);
        }

        utxo_diff.remove_utxo(*output_pointer);
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

        #[allow(clippy::cast_sign_loss)]
        let output_to_insert = if get_protocol_version(Some(epoch)) >= ProtocolVersion::V2_0 {
            if output.time_lock < checkpoint_zero_timestamp.try_into().unwrap() {
                ValueTransferOutput {
                    pkh: output.pkh,
                    value: output.value,
                    time_lock: output.time_lock + checkpoint_zero_timestamp as u64,
                }
            } else {
                output.clone()
            }
        } else {
            output.clone()
        };

        utxo_diff.insert_utxo(output_pointer, output_to_insert, block_number);
    }
}

/// Function to validate transactions in a block and update a utxo_set and a `TransactionsPool`
///
/// This uses a `Visitor` that will visit each transaction as well as its fee and weight.
#[allow(clippy::too_many_arguments)]
pub fn validate_block_transactions(
    utxo_set: &UnspentOutputsPool,
    dr_pool: &mut DataRequestPool,
    block: &Block,
    vrf_input: CheckpointVRF,
    signatures_to_verify: &mut Vec<SignaturesToVerify>,
    rep_eng: &ReputationEngine,
    epoch_constants: EpochConstants,
    block_number: u32,
    consensus_constants: &ConsensusConstants,
    consensus_constants_wit2: &ConsensusConstantsWit2,
    active_wips: &ActiveWips,
    mut visitor: Option<&mut dyn Visitor<Visitable = (Transaction, u64, u32)>>,
    stakes: &StakesTracker,
    protocol_version: ProtocolVersion,
) -> Result<Diff, failure::Error> {
    let epoch = block.block_header.beacon.checkpoint;
    let is_genesis = block.is_genesis(&consensus_constants.genesis_hash);
    let mut utxo_diff = UtxoDiff::new(utxo_set, block_number);
    // Init total fee
    let mut total_fee = 0;
    // When validating genesis block, keep track of total value created
    // The value created in the genesis block cannot be greater than 2^64 - the total block reward,
    // So the total amount is always representable by a u64
    let max_total_value_genesis = u64::MAX
        - total_block_reward(
            consensus_constants.initial_block_reward,
            consensus_constants.halving_period,
        );
    let mut genesis_value_available = max_total_value_genesis;

    // Check stake transactions are added in V1_8 at the earliest
    if protocol_version == ProtocolVersion::V1_7 && !block.txns.stake_txns.is_empty() {
        return Err(TransactionError::NoStakeTransactionsAllowed.into());
    }
    // Check stake transactions are added in V2_0 at the earliest
    if protocol_version <= ProtocolVersion::V1_8 && !block.txns.unstake_txns.is_empty() {
        return Err(TransactionError::NoUnstakeTransactionsAllowed.into());
    }

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
                None,
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

        update_utxo_diff(
            &mut utxo_diff,
            inputs,
            outputs,
            transaction.versioned_hash(protocol_version),
            epoch,
            consensus_constants.checkpoint_zero_timestamp,
        );

        // Add new hash to merkle tree
        let txn_hash = transaction.versioned_hash(protocol_version);
        let Hash::SHA256(sha) = txn_hash;
        vt_mt.push(Sha256(sha));

        // Execute visitor
        if let Some(visitor) = &mut visitor {
            let transaction = Transaction::ValueTransfer(transaction.clone());
            visitor.visit(&(transaction, fee, weight));
        }
    }
    let vt_hash_merkle_root = vt_mt.root();

    // Validate commit transactions in a block
    let mut co_mt = ProgressiveMerkleTree::sha256();
    let mut commits_number = HashMap::new();
    let block_beacon = block.block_header.beacon;
    let mut commit_hs = HashSet::with_capacity(block.txns.commit_txns.len());
    let collateral_age = consensus_constants_wit2.get_collateral_age(active_wips);
    let min_stake = consensus_constants_wit2.get_validator_min_stake_nanowits(epoch);
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
            collateral_age,
            block_number,
            consensus_constants.minimum_difficulty,
            active_wips,
            consensus_constants.superblock_period,
            protocol_version,
            stakes,
            consensus_constants.extra_rounds + 1,
            min_stake,
        )?;

        // Validation for only one commit for pkh/data request in a block
        let pkh = transaction.body.proof.proof.pkh();
        if !commit_hs.insert((dr_pointer, pkh)) {
            return Err(TransactionError::DuplicatedCommit { pkh, dr_pointer }.into());
        }

        total_fee += fee;

        increment_witnesses_counter(&mut commits_number, &dr_pointer, u32::from(dr_witnesses));

        let (inputs, outputs) = (
            transaction.body.collateral.iter(),
            transaction.body.outputs.iter(),
        );
        update_utxo_diff(
            &mut utxo_diff,
            inputs,
            outputs,
            transaction.versioned_hash(protocol_version),
            epoch,
            consensus_constants.checkpoint_zero_timestamp,
        );

        // Add new hash to merkle tree
        let txn_hash = transaction.versioned_hash(protocol_version);
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
        let txn_hash = transaction.versioned_hash(protocol_version);
        let Hash::SHA256(sha) = txn_hash;
        re_mt.push(Sha256(sha));
    }
    let re_hash_merkle_root = re_mt.root();

    // Make sure that the block does not try to include data requests asking for too many witnesses
    for transaction in &block.txns.data_request_txns {
        let dr_tx_hash = transaction.versioned_hash(protocol_version);
        if !dr_pool.data_request_pool.contains_key(&dr_tx_hash)
            && data_request_has_too_many_witnesses(
                &transaction.body.dr_output,
                stakes.validator_count(),
                Some(epoch),
            )
        {
            log::debug!(
                "Temporarily adding data request {} to data request pool for validation purposes",
                transaction.versioned_hash(protocol_version)
            );
            if let Err(e) = dr_pool.process_data_request(
                transaction,
                epoch,
                &block.versioned_hash(protocol_version),
            ) {
                log::error!("Error adding data request to the data request pool: {}", e);
            }
            if let Some(dr_state) = dr_pool.data_request_state_mutable(&dr_tx_hash) {
                dr_state.update_stage(0, true);
            } else {
                log::error!("Could not find data request state");
            }
        }
    }

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
            Some(stakes.validator_count()),
            Some(epoch),
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

        update_utxo_diff(
            &mut utxo_diff,
            vec![],
            outputs,
            transaction.versioned_hash(protocol_version),
            epoch,
            consensus_constants.checkpoint_zero_timestamp,
        );

        // Add new hash to merkle tree
        let txn_hash = transaction.versioned_hash(protocol_version);
        let Hash::SHA256(sha) = txn_hash;
        ta_mt.push(Sha256(sha));
    }
    let ta_hash_merkle_root = ta_mt.root();

    // All data requests for which we expected tally transactions should have been removed
    // upon creation of the tallies. If not, block is invalid due to missing expected tallies
    if !expected_tally_ready_drs.is_empty() {
        return Err(BlockError::MissingExpectedTallies {
            count: expected_tally_ready_drs.len(),
            block_hash: block.versioned_hash(protocol_version),
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
        let required_reward_collateral_ratio =
            PSEUDO_CONSENSUS_CONSTANTS_WIP0022_REWARD_COLLATERAL_RATIO;
        let (inputs, outputs, fee) = validate_dr_transaction(
            transaction,
            &utxo_diff,
            epoch,
            epoch_constants,
            signatures_to_verify,
            consensus_constants.collateral_minimum,
            consensus_constants.max_dr_weight,
            required_reward_collateral_ratio,
            active_wips,
            Some(protocol_version),
        )?;
        total_fee += fee;

        update_utxo_diff(
            &mut utxo_diff,
            inputs,
            outputs,
            transaction.versioned_hash(protocol_version),
            epoch,
            consensus_constants.checkpoint_zero_timestamp,
        );

        // Add new hash to merkle tree
        let txn_hash = transaction.versioned_hash(protocol_version);
        let Hash::SHA256(sha) = txn_hash;
        dr_mt.push(Sha256(sha));

        // Update dr weight
        let weight = transaction.weight();
        let acc_weight = dr_weight.saturating_add(weight);
        if acc_weight > consensus_constants.max_dr_weight {
            return Err(BlockError::TotalDataRequestWeightLimitExceeded {
                weight: acc_weight,
                max_weight: consensus_constants.max_dr_weight,
            }
            .into());
        }
        dr_weight = acc_weight;

        // Execute visitor
        if let Some(visitor) = &mut visitor {
            let transaction = Transaction::DataRequest(transaction.clone());
            visitor.visit(&(transaction, fee, weight));
        }
    }
    let dr_hash_merkle_root = dr_mt.root();

    let st_root = if protocol_version >= ProtocolVersion::V1_8 {
        // validate stake transactions in a block
        let mut st_mt = ProgressiveMerkleTree::sha256();
        let mut st_weight: u32 = 0;

        // Check if the block contains more than one stake tx from the same operator
        let duplicate = block
            .txns
            .stake_txns
            .iter()
            .map(|stake_tx| &stake_tx.body.output.authorization.public_key)
            .duplicates()
            .next();

        if let Some(duplicate) = duplicate {
            return Err(BlockError::RepeatedStakeOperator {
                pkh: duplicate.pkh(),
            }
            .into());
        }

        let min_stake = consensus_constants_wit2.get_validator_min_stake_nanowits(epoch);
        let max_stake = consensus_constants_wit2.get_validator_max_stake_nanowits(epoch);
        for transaction in &block.txns.stake_txns {
            let (inputs, _output, fee, weight, change) = validate_stake_transaction(
                transaction,
                &utxo_diff,
                epoch,
                epoch_constants,
                signatures_to_verify,
                stakes,
                min_stake,
                max_stake,
            )?;

            total_fee += fee;

            // Update st weight
            let acc_weight = st_weight.saturating_add(weight);
            let max_stake_block_weight =
                consensus_constants_wit2.get_maximum_stake_block_weight(epoch);
            if acc_weight > max_stake_block_weight {
                return Err(BlockError::TotalStakeWeightLimitExceeded {
                    weight: acc_weight,
                    max_weight: max_stake_block_weight,
                }
                .into());
            }
            st_weight = acc_weight;

            let outputs = change.iter().collect_vec();
            update_utxo_diff(
                &mut utxo_diff,
                inputs,
                outputs,
                transaction.versioned_hash(protocol_version),
                epoch,
                consensus_constants.checkpoint_zero_timestamp,
            );

            // Add new hash to merkle tree
            st_mt.push(transaction.versioned_hash(protocol_version).into());

            // TODO: Move validations to a visitor
            // // Execute visitor
            // if let Some(visitor) = &mut visitor {
            //     let transaction = Transaction::ValueTransfer(transaction.clone());
            //     visitor.visit(&(transaction, fee, weight));
            // }
        }

        Hash::from(st_mt.root())
    } else {
        // Nullify stake merkle roots for the legacy protocol version
        Hash::default()
    };

    let ut_root = if protocol_version >= ProtocolVersion::V2_0 {
        let mut ut_mt = ProgressiveMerkleTree::sha256();
        let mut ut_weight: u32 = 0;

        for transaction in &block.txns.unstake_txns {
            let min_stake = consensus_constants_wit2.get_validator_min_stake_nanowits(epoch);
            let unstake_delay = consensus_constants_wit2.get_unstaking_delay_seconds(epoch);
            let (fee, weight, outputs) =
                validate_unstake_transaction(transaction, epoch, stakes, min_stake, unstake_delay)?;

            total_fee += fee;

            // Update ut weight
            let acc_weight = ut_weight.saturating_add(weight);
            let max_unstake_block_weight =
                consensus_constants_wit2.get_maximum_unstake_block_weight(epoch);
            if acc_weight > max_unstake_block_weight {
                return Err(BlockError::TotalUnstakeWeightLimitExceeded {
                    weight: acc_weight,
                    max_weight: max_unstake_block_weight,
                }
                .into());
            }
            ut_weight = acc_weight;

            update_utxo_diff(
                &mut utxo_diff,
                vec![],
                outputs,
                transaction.versioned_hash(protocol_version),
                epoch,
                consensus_constants.checkpoint_zero_timestamp,
            );

            // Add new hash to merkle tree
            ut_mt.push(transaction.versioned_hash(protocol_version).into());
        }

        Hash::from(ut_mt.root())
    } else {
        // Nullify unstake merkle roots for the legacy protocol version
        Hash::default()
    };

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
            block.txns.mint.outputs.iter(),
            block.txns.mint.versioned_hash(protocol_version),
            epoch,
            consensus_constants.checkpoint_zero_timestamp,
        );
    }

    // Validate Merkle Root
    let merkle_roots = BlockMerkleRoots {
        mint_hash: block.txns.mint.versioned_hash(protocol_version),
        vt_hash_merkle_root: Hash::from(vt_hash_merkle_root),
        dr_hash_merkle_root: Hash::from(dr_hash_merkle_root),
        commit_hash_merkle_root: Hash::from(co_hash_merkle_root),
        reveal_hash_merkle_root: Hash::from(re_hash_merkle_root),
        tally_hash_merkle_root: Hash::from(ta_hash_merkle_root),
        stake_hash_merkle_root: st_root,
        unstake_hash_merkle_root: ut_root,
    };

    if merkle_roots != block.block_header.merkle_roots {
        log::debug!(
            "{:?} vs {:?}",
            merkle_roots,
            block.block_header.merkle_roots
        );
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
    stakes: &StakesTracker,
    protocol_version: ProtocolVersion,
    replication_factor: u16,
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
        let target_hash = if protocol_version == ProtocolVersion::V2_0 {
            let validator = block.block_sig.public_key.pkh();
            let eligibility = stakes.mining_eligibility(validator, block_epoch, replication_factor);
            if eligibility == Ok(Eligible::No(InsufficientPower))
                || eligibility == Ok(Eligible::No(NotStaking))
            {
                return Err(BlockError::ValidatorNotEligible { validator }.into());
            }

            Hash::max()
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

            target_hash
        };

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
    required_reward_collateral_ratio: u64,
    active_wips: &ActiveWips,
    superblock_period: u16,
    stakes: &StakesTracker,
    protocol_version: ProtocolVersion,
    max_rounds: u16,
    consensus_constants_wit2: &ConsensusConstantsWit2,
) -> Result<u64, failure::Error> {
    let utxo_diff = UtxoDiff::new(unspent_outputs_pool, block_number);
    let min_stake = consensus_constants_wit2.get_validator_min_stake_nanowits(current_epoch);
    let max_stake = consensus_constants_wit2.get_validator_max_stake_nanowits(current_epoch);
    let unstake_delay = consensus_constants_wit2.get_unstaking_delay_seconds(current_epoch);

    match transaction {
        Transaction::ValueTransfer(tx) => validate_vt_transaction(
            tx,
            &utxo_diff,
            current_epoch,
            epoch_constants,
            signatures_to_verify,
            max_vt_weight,
            Some(protocol_version),
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
            required_reward_collateral_ratio,
            active_wips,
            None,
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
            superblock_period,
            protocol_version,
            stakes,
            max_rounds,
            min_stake,
        )
        .map(|(_, _, fee)| fee),
        Transaction::Reveal(tx) => {
            validate_reveal_transaction(tx, data_request_pool, signatures_to_verify)
        }
        Transaction::Stake(tx) => validate_stake_transaction(
            tx,
            &utxo_diff,
            current_epoch,
            epoch_constants,
            signatures_to_verify,
            stakes,
            min_stake,
            max_stake,
        )
        .map(|(_, _, fee, _, _)| fee),
        Transaction::Unstake(tx) => {
            validate_unstake_transaction(tx, current_epoch, stakes, min_stake, unstake_delay)
                .map(|(fee, _, _)| fee)
        }
        _ => Err(TransactionError::NotValidTransaction.into()),
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
        stake_hash_merkle_root: merkle_tree_root(&block.txns.stake_txns),
        unstake_hash_merkle_root: merkle_tree_root(&block.txns.unstake_txns),
    };

    merkle_roots == block.block_header.merkle_roots
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
    b1_power: Power,
    b2_hash: Hash,
    b2_rep: Reputation,
    b2_vrf_hash: Hash,
    b2_is_active: bool,
    b2_power: Power,
    s: &VrfSlots,
    protocol_version: ProtocolVersion,
) -> Ordering {
    if protocol_version == ProtocolVersion::V2_0 {
        match b1_power.cmp(&b2_power) {
            // Equal power, first compare VRF hash and finally the block hash
            Ordering::Equal => {
                b1_vrf_hash
                    .cmp(&b2_vrf_hash)
                    .reverse()
                    // Bigger block implies worse block candidate
                    .then(b1_hash.cmp(&b2_hash).reverse())
            }
            ord => ord,
        }
    } else {
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
}

/// Blocking process to verify signatures
pub fn verify_signatures(
    signatures_to_verify: Vec<SignaturesToVerify>,
    vrf: &mut VrfCtx,
) -> Result<Vec<Hash>, failure::Error> {
    let mut vrf_hashes = vec![];
    for signature in signatures_to_verify {
        match signature {
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
            } => verify(&public_key, &data, &signature).map_err(|e| {
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
            } => verify(&public_key, &data, &signature).map_err(|_e| {
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
                )?;
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
