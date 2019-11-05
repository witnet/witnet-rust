use std::{
    cmp::Ordering,
    collections::{HashMap, HashSet},
    convert::{TryFrom, TryInto},
};

use witnet_crypto::{
    hash::Sha256,
    merkle::{merkle_tree_root as crypto_merkle_tree_root, ProgressiveMerkleTree},
    signature::verify,
};
use witnet_data_structures::chain::EpochConstants;
use witnet_data_structures::{
    chain::{
        Block, BlockMerkleRoots, CheckpointBeacon, DataRequestOutput, DataRequestStage,
        DataRequestState, Epoch, Hash, Hashable, Input, KeyedSignature, OutputPointer,
        PublicKeyHash, RADRequest, RADTally, Reputation, ReputationEngine, UnspentOutputsPool,
        ValueTransferOutput,
    },
    data_request::{calculate_dr_vt_reward, true_revealer, DataRequestPool},
    error::{BlockError, DataRequestError, TransactionError},
    transaction::{
        CommitTransaction, DRTransaction, MintTransaction, RevealTransaction, TallyTransaction,
        VTTransaction,
    },
    vrf::{BlockEligibilityClaim, DataRequestEligibilityClaim, VrfCtx},
};
use witnet_rad::{run_tally, script::unpack_radon_script, types::RadonTypes};

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
    let mut total_value = 0;

    for input in inputs {
        let vt_output = utxo_diff.get(&input.output_pointer()).ok_or_else(|| {
            TransactionError::OutputNotFound {
                output: input.output_pointer().clone(),
            }
        })?;

        // Verify that commits are only accepted after the time lock expired
        let epoch_timestamp = epoch_constants.epoch_timestamp(epoch)?;
        let vt_time_lock = vt_output.time_lock as i64;
        if vt_time_lock > epoch_timestamp {
            return Err(TransactionError::TimeLock {
                expected: vt_time_lock,
                current: epoch_timestamp,
            }
            .into());
        } else {
            total_value += vt_output.value;
        }
    }

    Ok(total_value)
}

/// Calculate the sum of the values of the outputs of a transaction.
pub fn transaction_outputs_sum(outputs: &[ValueTransferOutput]) -> u64 {
    outputs.iter().map(|o| o.value).sum()
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
    let out_value = transaction_outputs_sum(&vt_tx.body.outputs);

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
    let out_value = transaction_outputs_sum(&dr_tx.body.outputs) + dr_tx.body.dr_output.value;

    if out_value > in_value {
        Err(TransactionError::NegativeFee.into())
    } else {
        Ok(in_value - out_value)
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

    let mint_value = mint_tx.output.value;
    let block_reward_value = block_reward(mint_tx.epoch);
    // Mint value must be equal to block_reward + transaction fees
    if mint_value != total_fees + block_reward_value {
        Err(BlockError::MismatchedMintValue {
            mint_value,
            fees_value: total_fees,
            reward_value: block_reward_value,
        }
        .into())
    } else {
        Ok(())
    }
}

/// Function to validate a rad request
pub fn validate_rad_request(rad_request: &RADRequest) -> Result<(), failure::Error> {
    let retrieval_paths = &rad_request.retrieve;
    for path in retrieval_paths {
        unpack_radon_script(path.script.as_slice())?;
    }

    let aggregate = &rad_request.aggregate;
    unpack_radon_script(aggregate.script.as_slice())?;

    let consensus = &rad_request.tally;
    unpack_radon_script(consensus.script.as_slice())?;

    Ok(())
}

/// Function to validate a tally consensus
pub fn validate_consensus(
    reveals: &[&[u8]],
    miner_tally: &[u8],
    consensus: &RADTally,
) -> Result<(), failure::Error> {
    let radon_types_vec: Vec<RadonTypes> = reveals
        .iter()
        .filter_map(|&input| RadonTypes::try_from(input).ok())
        .collect();

    let local_tally = run_tally(radon_types_vec, consensus)?;

    if local_tally == miner_tally {
        Ok(())
    } else {
        Err(TransactionError::MismatchedConsensus {
            local_tally,
            miner_tally: miner_tally.to_vec(),
        }
        .into())
    }
}

/// Function to validate a value transfer transaction
pub fn validate_vt_transaction<'a>(
    vt_tx: &'a VTTransaction,
    utxo_diff: &UtxoDiff,
    epoch: Epoch,
    epoch_constants: EpochConstants,
) -> Result<(Vec<&'a Input>, Vec<&'a ValueTransferOutput>, u64), failure::Error> {
    validate_transaction_signature(
        &vt_tx.signatures,
        &vt_tx.body.inputs,
        vt_tx.hash(),
        utxo_diff,
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

/// Function to validate a data request transaction
pub fn validate_dr_transaction<'a>(
    dr_tx: &'a DRTransaction,
    utxo_diff: &UtxoDiff,
    epoch: Epoch,
    epoch_constants: EpochConstants,
) -> Result<(Vec<&'a Input>, Vec<&'a ValueTransferOutput>, u64), failure::Error> {
    validate_transaction_signature(
        &dr_tx.signatures,
        &dr_tx.body.inputs,
        dr_tx.hash(),
        utxo_diff,
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
/// - The sum of fees is strictly less than the data request value
/// - All witnesses receive exactly the same reward (value - total fees % witnesses == 0)
pub fn validate_data_request_output(request: &DataRequestOutput) -> Result<(), TransactionError> {
    if request.witnesses < 1 {
        return Err(TransactionError::InsufficientWitnesses);
    }

    let sum_fees = request
        .commit_fee
        .checked_add(request.reveal_fee)
        .and_then(|res| res.checked_mul(request.witnesses.into()))
        .and_then(|res| res.checked_add(request.tally_fee))
        .ok_or_else(|| TransactionError::FeeOverflow)?;

    // Calculate reward to be shared between all the witnesses, which must be greater than 0
    if request.value <= sum_fees {
        return Err(TransactionError::NoReward {
            value: request.value,
            fees: sum_fees,
        });
    }
    let total_witness_reward = request.value - sum_fees;

    // Must be divisible by the number of witnesses
    if (total_witness_reward % u64::from(request.witnesses)) != 0 {
        Err(TransactionError::NonUniformReward {
            reward: total_witness_reward,
            witnesses: request.witnesses,
        })
    } else {
        Ok(())
    }
}

/// Function to validate a commit transaction
pub fn validate_commit_transaction(
    co_tx: &CommitTransaction,
    dr_pool: &DataRequestPool,
    beacon: CheckpointBeacon,
    vrf: &mut VrfCtx,
    rep_eng: &ReputationEngine,
    epoch: Epoch,
    epoch_constants: EpochConstants,
) -> Result<(Hash, u16, u64), failure::Error> {
    // Get DataRequest information
    let dr_pointer = co_tx.body.dr_pointer;
    let dr_state = dr_pool
        .data_request_pool
        .get(&dr_pointer)
        .ok_or(TransactionError::DataRequestNotFound { hash: dr_pointer })?;
    if dr_state.stage != DataRequestStage::COMMIT {
        return Err(DataRequestError::NotCommitStage.into());
    }

    let dr_output = &dr_state.data_request;

    // Verify that commits are only accepted after the time lock expired
    let epoch_timestamp = epoch_constants.epoch_timestamp(epoch)?;
    let dr_time_lock = dr_output.data_request.time_lock as i64;
    if dr_time_lock > epoch_timestamp {
        return Err(TransactionError::TimeLock {
            expected: dr_time_lock,
            current: epoch_timestamp,
        }
        .into());
    }

    let commit_signature = validate_commit_reveal_signature(co_tx.hash(), &co_tx.signatures)?;

    let pkh = commit_signature.public_key.pkh();
    let pkh2 = co_tx.body.proof.proof.pkh();
    if pkh != pkh2 {
        return Err(TransactionError::PublicKeyHashMismatch {
            expected_pkh: pkh2,
            signature_pkh: pkh,
        }
        .into());
    }

    let pkh = co_tx.body.proof.proof.pkh();
    let my_reputation = rep_eng.trs.get(&pkh);
    let total_active_reputation = rep_eng.trs.get_sum(rep_eng.ars.active_identities());
    let num_witnesses = dr_output.witnesses + dr_output.backup_witnesses;
    let num_active_identities = rep_eng.ars.active_identities_number() as u32;
    let target_hash = calculate_reppoe_threshold(
        my_reputation,
        total_active_reputation,
        num_witnesses,
        num_active_identities,
    );
    verify_poe_data_request(
        vrf,
        &co_tx.body.proof,
        beacon,
        co_tx.body.dr_pointer,
        target_hash,
    )?;

    // The commit fee here is the fee to include one commit
    Ok((dr_pointer, dr_output.witnesses, dr_output.commit_fee))
}

/// Function to validate a reveal transaction
pub fn validate_reveal_transaction(
    re_tx: &RevealTransaction,
    dr_pool: &DataRequestPool,
) -> Result<u64, failure::Error> {
    // Get DataRequest information
    let dr_pointer = re_tx.body.dr_pointer;
    let dr_state = dr_pool
        .data_request_pool
        .get(&dr_pointer)
        .ok_or(TransactionError::DataRequestNotFound { hash: dr_pointer })?;

    if dr_state.stage != DataRequestStage::REVEAL {
        return Err(DataRequestError::NotRevealStage.into());
    }

    let reveal_signature = validate_commit_reveal_signature(re_tx.hash(), &re_tx.signatures)?;
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
        return Err(TransactionError::DuplicatedReveal { pkh }.into());
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

/// Function to validate a tally transaction
/// FIXME(#695): refactor tally validation
pub fn validate_tally_transaction<'a>(
    ta_tx: &'a TallyTransaction,
    dr_pool: &DataRequestPool,
) -> Result<(Vec<&'a ValueTransferOutput>, u64), failure::Error> {
    // Get DataRequestState
    let dr_pointer = ta_tx.dr_pointer;
    let dr_state = dr_pool
        .data_request_pool
        .get(&dr_pointer)
        .ok_or(TransactionError::DataRequestNotFound { hash: dr_pointer })?;

    if dr_state.stage != DataRequestStage::TALLY {
        return Err(DataRequestError::NotTallyStage.into());
    }

    let dr_output = &dr_state.data_request;

    // The unwrap is safe because we know that the data request exists
    let reveals: Vec<&[u8]> = dr_pool
        .get_reveals(&dr_pointer)
        .unwrap()
        .into_iter()
        .map(|reveal| reveal.body.reveal.as_slice())
        .collect();

    //TODO: Check Tally convergence

    // Validate tally result
    let miner_tally = ta_tx.tally.clone();
    let tally_stage = &dr_output.data_request.tally;

    validate_consensus(&reveals, &miner_tally, tally_stage)?;

    //TODO: Check dishonest reveals
    let n_dishonest_reveals = 0;
    validate_tally_outputs(&dr_state, &ta_tx, reveals.len(), n_dishonest_reveals)?;

    Ok((ta_tx.outputs.iter().collect(), dr_output.tally_fee))
}

pub fn validate_tally_outputs(
    dr_state: &DataRequestState,
    ta_tx: &TallyTransaction,
    n_reveals: usize,
    n_dishonest_reveals: usize,
) -> Result<(), failure::Error> {
    let witnesses = dr_state.data_request.witnesses as usize;
    let change_required = witnesses > n_reveals || n_dishonest_reveals > 0;

    if change_required && (ta_tx.outputs.len() != n_reveals + 1) {
        return Err(TransactionError::WrongNumberOutputs {
            outputs: ta_tx.outputs.len(),
            expected_outputs: n_reveals + 1,
        }
        .into());
    } else if !change_required && (ta_tx.outputs.len() != n_reveals) {
        return Err(TransactionError::WrongNumberOutputs {
            outputs: ta_tx.outputs.len(),
            expected_outputs: n_reveals,
        }
        .into());
    }

    let mut pkh_rewarded: HashSet<PublicKeyHash> = HashSet::new();
    let reveal_reward = calculate_dr_vt_reward(&dr_state.data_request);
    for (i, output) in ta_tx.outputs.iter().enumerate() {
        if change_required && i == ta_tx.outputs.len() - 1 && output.pkh == dr_state.pkh {
            // Expected honest witnesses is tally outputs - 1, which would be
            // the value transfer output related to the tally change.
            let honest_witnesses = ta_tx.outputs.len() - 1;

            let expected_tally_change = reveal_reward * (witnesses - honest_witnesses) as u64
                + dr_state.data_request.reveal_fee * (witnesses - n_reveals) as u64;
            if expected_tally_change != output.value {
                return Err(TransactionError::InvalidTallyChange {
                    change: output.value,
                    expected_change: expected_tally_change,
                }
                .into());
            }
        } else {
            if pkh_rewarded.contains(&output.pkh) {
                return Err(TransactionError::MultipleRewards { pkh: output.pkh }.into());
            }
            let reveal = dr_state.info.reveals.get(&output.pkh);

            match reveal {
                Some(r) => {
                    if !true_revealer(&r, &ta_tx.tally) {
                        return Err(TransactionError::DishonestReward.into());
                    }
                }

                None => return Err(TransactionError::RevealNotFound.into()),
            }
            pkh_rewarded.insert(output.pkh);
        }
    }

    Ok(())
}

/// Function to validate a block signature
pub fn validate_block_signature(block: &Block) -> Result<(), failure::Error> {
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

    verify(&public_key, &message, &signature)
        .map_err(|_| BlockError::VerifySignatureFail { hash: block.hash() }.into())
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
/// Function to validate a commit/reveal transaction signature
pub fn validate_commit_reveal_signature(
    tx_hash: Hash,
    signatures: &[KeyedSignature],
) -> Result<&KeyedSignature, failure::Error> {
    if let Some(tx_keyed_signature) = signatures.get(0) {
        let Hash::SHA256(message) = tx_hash;

        let fte = |e: failure::Error| TransactionError::VerifyTransactionSignatureFail {
            hash: tx_hash,
            index: 0,
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

        verify(&public_key, &message, &signature).map_err(fte)?;

        Ok(tx_keyed_signature)
    } else {
        Err(TransactionError::SignatureNotFound.into())
    }
}

/// Function to validate a transaction signature
pub fn validate_transaction_signature(
    signatures: &[KeyedSignature],
    inputs: &[Input],
    tx_hash: Hash,
    utxo_set: &UtxoDiff,
) -> Result<(), failure::Error> {
    if signatures.len() != inputs.len() {
        return Err(TransactionError::MismatchingSignaturesNumber {
            signatures_n: signatures.len() as u8,
            inputs_n: inputs.len() as u8,
        }
        .into());
    }

    let tx_hash_bytes = match tx_hash {
        Hash::SHA256(x) => x.to_vec(),
    };

    for (i, (input, keyed_signature)) in inputs.iter().zip(signatures.iter()).enumerate() {
        // Helper function to map errors to include transaction hash and input
        // index, as well as the error message.
        let fte = |e: failure::Error| TransactionError::VerifyTransactionSignatureFail {
            hash: tx_hash,
            index: i as u8,
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
        verify(&public_key, &tx_hash_bytes, &signature).map_err(fte)?;
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
    hm.entry(k.clone())
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
    for input in inputs {
        // Obtain the OuputPointer of each input and remove it from the utxo_diff
        let output_pointer = input.output_pointer();

        utxo_diff.remove_utxo(output_pointer.clone());
    }

    for (index, output) in outputs.into_iter().enumerate() {
        // Add the new outputs to the utxo_diff
        let output_pointer = OutputPointer {
            transaction_id: tx_hash,
            output_index: index as u32,
        };

        utxo_diff.insert_utxo(output_pointer, output.clone());
    }
}

/// Function to validate transactions in a block and update a utxo_set and a `TransactionsPool`
pub fn validate_block_transactions(
    utxo_set: &UnspentOutputsPool,
    dr_pool: &DataRequestPool,
    block: &Block,
    vrf: &mut VrfCtx,
    rep_eng: &ReputationEngine,
    epoch_constants: EpochConstants,
) -> Result<Diff, failure::Error> {
    let epoch = block.block_header.beacon.checkpoint;
    let mut utxo_diff = UtxoDiff::new(utxo_set);

    // Init total fee
    let mut total_fee = 0;

    // TODO: replace for loop with a try_fold
    // Validate value transfer transactions in a block
    let mut vt_mt = ProgressiveMerkleTree::sha256();
    for transaction in &block.txns.value_transfer_txns {
        let (inputs, outputs, fee) =
            validate_vt_transaction(transaction, &utxo_diff, epoch, epoch_constants)?;
        total_fee += fee;

        update_utxo_diff(&mut utxo_diff, inputs, outputs, transaction.hash());

        // Add new hash to merkle tree
        let txn_hash = transaction.hash();
        let Hash::SHA256(sha) = txn_hash;
        vt_mt.push(Sha256(sha));
    }
    let vt_hash_merkle_root = vt_mt.root();

    // Validate data request transactions in a block
    let mut dr_mt = ProgressiveMerkleTree::sha256();
    for transaction in &block.txns.data_request_txns {
        let (inputs, outputs, fee) =
            validate_dr_transaction(transaction, &utxo_diff, epoch, epoch_constants)?;
        total_fee += fee;

        update_utxo_diff(&mut utxo_diff, inputs, outputs, transaction.hash());

        // Add new hash to merkle tree
        let txn_hash = transaction.hash();
        let Hash::SHA256(sha) = txn_hash;
        dr_mt.push(Sha256(sha));
    }
    let dr_hash_merkle_root = dr_mt.root();

    // Validate commit transactions in a block
    let mut co_mt = ProgressiveMerkleTree::sha256();
    let mut commits_number = HashMap::new();
    let block_beacon = block.block_header.beacon;
    for transaction in &block.txns.commit_txns {
        let (dr_pointer, dr_witnesses, fee) = validate_commit_transaction(
            &transaction,
            dr_pool,
            block_beacon,
            vrf,
            rep_eng,
            epoch,
            epoch_constants,
        )?;
        total_fee += fee;

        increment_witnesses_counter(&mut commits_number, &dr_pointer, u32::from(dr_witnesses));

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
    for transaction in &block.txns.reveal_txns {
        let fee = validate_reveal_transaction(&transaction, dr_pool)?;
        total_fee += fee;

        // Add new hash to merkle tree
        let txn_hash = transaction.hash();
        let Hash::SHA256(sha) = txn_hash;
        re_mt.push(Sha256(sha));
    }
    let re_hash_merkle_root = re_mt.root();

    // Validate tally transactions in a block
    let mut ta_mt = ProgressiveMerkleTree::sha256();
    for transaction in &block.txns.tally_txns {
        let (outputs, fee) = validate_tally_transaction(transaction, dr_pool)?;
        total_fee += fee;

        update_utxo_diff(&mut utxo_diff, vec![], outputs, transaction.hash());

        // Add new hash to merkle tree
        let txn_hash = transaction.hash();
        let Hash::SHA256(sha) = txn_hash;
        ta_mt.push(Sha256(sha));
    }
    let ta_hash_merkle_root = ta_mt.root();

    // Validate mint
    validate_mint_transaction(&block.txns.mint, total_fee, block_beacon.checkpoint)?;

    // Insert mint in utxo
    update_utxo_diff(
        &mut utxo_diff,
        vec![],
        vec![&block.txns.mint.output],
        block.txns.mint.hash(),
    );

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
    chain_beacon: CheckpointBeacon,
    utxo_set: &UnspentOutputsPool,
    data_request_pool: &DataRequestPool,
    vrf: &mut VrfCtx,
    rep_eng: &ReputationEngine,
    epoch_constants: EpochConstants,
) -> Result<Diff, failure::Error> {
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
    } else {
        let total_identities = rep_eng.ars.active_identities_number() as u32;
        let target_hash = calculate_randpoe_threshold(total_identities);
        verify_poe_block(
            vrf,
            &block.block_header.proof,
            block.block_header.beacon,
            target_hash,
        )?;
        validate_block_signature(&block)?;

        // TODO: in the future, a block without any transactions may be invalid
        validate_block_transactions(
            &utxo_set,
            &data_request_pool,
            &block,
            vrf,
            rep_eng,
            epoch_constants,
        )
    }
}

/// Function to validate a block candidate
pub fn validate_candidate(
    block: &Block,
    current_epoch: Epoch,
    vrf: &mut VrfCtx,
    total_identities: u32,
) -> Result<(), BlockError> {
    let block_epoch = block.block_header.beacon.checkpoint;
    if block_epoch != current_epoch {
        return Err(BlockError::CandidateFromDifferentEpoch {
            block_epoch,
            current_epoch,
        });
    }

    let target_hash = calculate_randpoe_threshold(total_identities);
    verify_poe_block(
        vrf,
        &block.block_header.proof,
        block.block_header.beacon,
        target_hash,
    )
}

pub fn calculate_randpoe_threshold(total_identities: u32) -> Hash {
    let max = u32::max_value();
    let target = if total_identities == 0 {
        max
    } else {
        max / total_identities
    };

    Hash::with_first_u32(target)
}

pub fn calculate_reppoe_threshold(
    my_reputation: Reputation,
    total_active_reputation: Reputation,
    num_witnesses: u16,
    num_active_identities: u32,
) -> Hash {
    // Add 1 to reputation because otherwise a node with 0 reputation would
    // never be eligible for a data request
    let my_reputation = my_reputation.0 + 1;

    // Add N to the total active reputation to account for the +1 to my_reputation
    // This is equivalent to adding 1 reputation to every active identity
    let total_active_reputation = total_active_reputation.0 + num_active_identities;

    // The number of witnesses for the data request.
    // If num_witnesses is zero, it will be impossible to commit to this data request
    // However that is impossible because there is a data request validation that prevents it
    let num_witnesses = u32::from(num_witnesses);

    let max = u32::max_value();
    // Check for overflow: when the probability is more than 100%, cap it to 100%
    let target = if num_witnesses * my_reputation >= total_active_reputation {
        max
    } else {
        // First divide and then multiply. This introduces a small rounding error.
        // We could multiply first if we cast everything to u64.
        (max / total_active_reputation) * num_witnesses * my_reputation
    };

    Hash::with_first_u32(target)
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

/// 1 satowit is the minimal unit of value
/// 1 wit = 100_000_000 satowits
pub const SATOWITS_PER_WIT: u64 = 100_000_000;

/// Calculate the block mining reward.
/// Returns "satowits", where 1 wit = 100_000_000 satowits.
pub fn block_reward(epoch: Epoch) -> u64 {
    let initial_reward: u64 = 500 * SATOWITS_PER_WIT;
    let halvings = epoch / 1_750_000;
    if halvings < 64 {
        initial_reward >> halvings
    } else {
        0
    }
}

/// Function to check poe validation for blocks
pub fn verify_poe_block(
    vrf: &mut VrfCtx,
    proof: &BlockEligibilityClaim,
    beacon: CheckpointBeacon,
    target_hash: Hash,
) -> Result<(), BlockError> {
    let vrf_hash = proof
        .verify(vrf, beacon)
        .map_err(|_| BlockError::NotValidPoe)?;
    if vrf_hash > target_hash {
        Err(BlockError::BlockEligibilityDoesNotMeetTarget {
            vrf_hash,
            target_hash,
        })
    } else {
        Ok(())
    }
}

/// Function to check poe validation for data requests
pub fn verify_poe_data_request(
    vrf: &mut VrfCtx,
    proof: &DataRequestEligibilityClaim,
    beacon: CheckpointBeacon,
    dr_hash: Hash,
    target_hash: Hash,
) -> Result<(), TransactionError> {
    let vrf_hash = proof
        .verify(vrf, beacon, dr_hash)
        .map_err(|_| TransactionError::InvalidDataRequestPoe)?;
    if vrf_hash > target_hash {
        Err(TransactionError::DataRequestEligibilityDoesNotMeetTarget {
            vrf_hash,
            target_hash,
        })
    } else {
        Ok(())
    }
}

/// Diffs to apply to an utxo set. This type does not contains a
/// reference to the original utxo set.
#[derive(Debug, Default)]
pub struct Diff {
    utxos_to_add: UnspentOutputsPool,
    utxos_to_remove: HashSet<OutputPointer>,
    utxos_to_remove_dr: Vec<OutputPointer>,
}

impl Diff {
    pub fn apply(mut self, utxo_set: &mut UnspentOutputsPool) {
        for (output_pointer, output) in self.utxos_to_add.drain() {
            utxo_set.insert(output_pointer, output);
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
    /// let diff = Diff::default();
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
        for (output_pointer, output) in self.utxos_to_add.iter() {
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
    utxo_pool: &'a UnspentOutputsPool,
}

impl<'a> UtxoDiff<'a> {
    /// Create a new UtxoDiff without additional insertions or deletions
    pub fn new(utxo_pool: &'a UnspentOutputsPool) -> Self {
        UtxoDiff {
            utxo_pool,
            diff: Default::default(),
        }
    }

    /// Record an insertion to perform on the utxo set
    pub fn insert_utxo(&mut self, output_pointer: OutputPointer, output: ValueTransferOutput) {
        self.diff.utxos_to_add.insert(output_pointer, output);
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
        self.utxo_pool
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
}

pub fn compare_blocks(
    b1_hash: Hash,
    b1_rep: Reputation,
    b2_hash: Hash,
    b2_rep: Reputation,
) -> Ordering {
    // Greater implies than the block is a better choice
    // Bigger reputation implies better block candidate
    b1_rep
        .cmp(&b2_rep)
        // Bigger hash implies worse block candidate
        .then(b1_hash.cmp(&b2_hash).reverse())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_compare_block() {
        let hash_1s = Hash::SHA256([1; 32]);
        let hash_2s = Hash::SHA256([2; 32]);
        let rep_1 = Reputation(1);
        let rep_2 = Reputation(2);

        // Same hash different reputation
        assert_eq!(
            compare_blocks(hash_1s, rep_1, hash_1s, rep_2),
            Ordering::Less
        );
        assert_eq!(
            compare_blocks(hash_1s, rep_2, hash_1s, rep_1),
            Ordering::Greater
        );

        // Same reputation different hash
        assert_eq!(
            compare_blocks(hash_2s, rep_1, hash_1s, rep_1),
            Ordering::Less
        );
        assert_eq!(
            compare_blocks(hash_1s, rep_1, hash_2s, rep_1),
            Ordering::Greater
        );

        // Same reputation and hash
        assert_eq!(
            compare_blocks(hash_1s, rep_1, hash_1s, rep_1),
            Ordering::Equal
        );
    }

    #[test]
    fn test_block_reward() {
        // Satowits per wit
        let spw = 100_000_000;

        assert_eq!(block_reward(0), 500 * spw);
        assert_eq!(block_reward(1), 500 * spw);
        assert_eq!(block_reward(1_749_999), 500 * spw);
        assert_eq!(block_reward(1_750_000), 250 * spw);
        assert_eq!(block_reward(3_499_999), 250 * spw);
        assert_eq!(block_reward(3_500_000), 125 * spw);
        assert_eq!(block_reward(1_750_000 * 35), 1);
        assert_eq!(block_reward(1_750_000 * 36), 0);
        assert_eq!(block_reward(1_750_000 * 63), 0);
        assert_eq!(block_reward(1_750_000 * 64), 0);
        assert_eq!(block_reward(1_750_000 * 100), 0);
    }

    #[test]
    fn target_randpoe() {
        let max_hash = Hash::with_first_u32(0xFFFF_FFFF);
        let t00 = calculate_randpoe_threshold(0);
        let t01 = calculate_randpoe_threshold(1);
        assert_eq!(t00, max_hash);
        assert_eq!(t00, t01);
        let t02 = calculate_randpoe_threshold(2);
        assert_eq!(t02, Hash::with_first_u32(0x7FFF_FFFF));
        let t03 = calculate_randpoe_threshold(3);
        assert_eq!(t03, Hash::with_first_u32(0x5555_5555));
        let t04 = calculate_randpoe_threshold(4);
        assert_eq!(t04, Hash::with_first_u32(0x3FFF_FFFF));
        let t05 = calculate_randpoe_threshold(1024);
        assert_eq!(t05, Hash::with_first_u32(0x003F_FFFF));
        let t06 = calculate_randpoe_threshold(1024 * 1024);
        assert_eq!(t06, Hash::with_first_u32(0x0000_0FFF));
    }

    #[test]
    fn target_reppoe() {
        // 100% when we have all the reputation
        let t00 = calculate_reppoe_threshold(Reputation(50), Reputation(50), 1, 1);
        assert_eq!(t00, Hash::with_first_u32(0xFFFF_FFFF));

        // 50% when there are 2 nodes with 50% of the reputation each
        let t01 = calculate_reppoe_threshold(Reputation(1), Reputation(2), 1, 2);
        // Since the calculate_reppoe function first divides and later
        // multiplies, we get a rounding error here
        assert_eq!(t01, Hash::with_first_u32(0x7FFF_FFFE));

        // 10 identities with 100 total reputation but 10 witnesses for the data request:
        // 10 * (10 + 1) / (100 + 10) = 100%
        let t02 = calculate_reppoe_threshold(Reputation(10), Reputation(100), 10, 10);
        assert_eq!(t02, Hash::with_first_u32(0xFFFF_FFFF));

        // 10 identities with 100 total reputation but 10 witnesses for the data request:
        // 1 * (50 + 1) / (100 + 10) = 46%
        let t03 = calculate_reppoe_threshold(Reputation(50), Reputation(100), 1, 10);
        assert_eq!(t03, Hash::with_first_u32(0x76B0_DF5F));
    }

    #[test]
    fn target_reppoe_zero_reputation() {
        // Test the behavior of the algorithm when our node has 0 reputation

        // 100% when the total reputation is 0
        let t00 = calculate_reppoe_threshold(Reputation(0), Reputation(0), 1, 0);
        assert_eq!(t00, Hash::with_first_u32(0xFFFF_FFFF));
        let t01 = calculate_reppoe_threshold(Reputation(0), Reputation(0), 100, 0);
        assert_eq!(t01, Hash::with_first_u32(0xFFFF_FFFF));
        let t02 = calculate_reppoe_threshold(Reputation(0), Reputation(0), 1, 1);
        assert_eq!(t02, Hash::with_first_u32(0xFFFF_FFFF));

        // 50% when the total reputation is 1
        let t03 = calculate_reppoe_threshold(Reputation(0), Reputation(1), 1, 1);
        assert_eq!(t03, Hash::with_first_u32(0x7FFF_FFFF));

        // 33% when the total reputation is 1 but there are 2 active identities
        let t04 = calculate_reppoe_threshold(Reputation(0), Reputation(1), 1, 2);
        assert_eq!(t04, Hash::with_first_u32(0x5555_5555));

        // 10 identities with 100 total reputation: 1 / (100 + 10) = 0.9%
        let t05 = calculate_reppoe_threshold(Reputation(0), Reputation(100), 1, 10);
        assert_eq!(t05, Hash::with_first_u32(0x0253_C825));

        // 10 identities with 100 total reputation but 10 witnesses for the data request:
        // 10 * 1 / (100 + 10) = 9%
        let t06 = calculate_reppoe_threshold(Reputation(0), Reputation(100), 10, 10);
        assert_eq!(t06, Hash::with_first_u32(0x1745_D172));

        // 10_000 identities with 10_000 total reputation but 10 witnesses for the data request:
        // 10 * 1 / (10000 + 100) = 0.099%
        let t07 = calculate_reppoe_threshold(Reputation(0), Reputation(10_000), 10, 100);
        assert_eq!(t07, Hash::with_first_u32(0x0040_E318));
    }
}
