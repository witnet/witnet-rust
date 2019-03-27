use witnet_crypto::{hash::Sha256, merkle::merkle_tree_root as crypto_merkle_tree_root};

use std::collections::HashMap;
use witnet_data_structures::{
    chain::{
        Block, BlockError, BlockInChain, CheckpointBeacon, DataRequestOutput, Epoch, Hash,
        Hashable, Input, KeyedSignature, Output, OutputPointer, RADRequest, Signature, Transaction,
        TransactionBody, TransactionError, TransactionType, TransactionsPool, UnspentOutputsPool,
    },
    data_request::DataRequestPool,
    serializers::decoders::{TryFrom, TryInto},
};

use witnet_rad::{run_consensus, script::unpack_radon_script, types::RadonTypes};
use witnet_wallet::signature::verify;

/// Calculate the sum of the values of the outputs pointed by the
/// inputs of a transaction. If an input pointed-output is not
/// found in `pool`, then an error is returned instead indicating
/// it. If a Signature is invalid an error is returned too
pub fn transaction_inputs_sum(
    tx: &TransactionBody,
    pool: &UnspentOutputsPool,
) -> Result<u64, failure::Error> {
    let mut total_value = 0;

    for input in &tx.inputs {
        let pointed_value = pool
            .get(&input.output_pointer())
            .ok_or_else(|| TransactionError::OutputNotFound {
                output: input.output_pointer(),
            })?
            .value();
        total_value += pointed_value;
    }

    Ok(total_value)
}

/// Calculate the sum of the values of the outputs of a transaction.
pub fn transaction_outputs_sum(tx: &TransactionBody) -> u64 {
    tx.outputs.iter().map(Output::value).sum()
}

/// Returns the fee of a transaction.
///
/// The fee is the difference between the outputs and the inputs
/// of the transaction. The pool parameter is used to find the
/// outputs pointed by the inputs and that contain the actual
/// their value.
pub fn transaction_fee(
    tx: &TransactionBody,
    pool: &UnspentOutputsPool,
) -> Result<u64, failure::Error> {
    let in_value = transaction_inputs_sum(tx, pool)?;
    let out_value = transaction_outputs_sum(tx);

    if out_value > in_value {
        Err(TransactionError::NegativeFee)?
    } else {
        Ok(in_value - out_value)
    }
}

/// Returns `true` if the transaction classifies as a _mint
/// transaction_.  A mint transaction is one that has no inputs,
/// only outputs, thus, is allowed to create new wits.
pub fn transaction_is_mint(tx: &TransactionBody) -> bool {
    tx.inputs.is_empty()
}

/// Function to validate a mint transaction
pub fn validate_mint_transaction(
    tx: &TransactionBody,
    total_fees: u64,
    block_reward: u64,
) -> Result<(), failure::Error> {
    let mint_value = transaction_outputs_sum(tx);

    if !transaction_is_mint(tx) {
        Err(TransactionError::InvalidMintTransaction)?
    } else if mint_value != total_fees + block_reward {
        Err(BlockError::MismatchedMintValue {
            mint_value,
            fees_value: total_fees,
            reward_value: block_reward,
        })?
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

    let consensus = &rad_request.consensus;
    unpack_radon_script(consensus.script.as_slice())?;

    Ok(())
}

/// Function to validate a tally consensus
pub fn validate_consensus(
    reveals: Vec<Vec<u8>>,
    miner_tally: Vec<u8>,
    tally_stage: Vec<u8>,
) -> Result<(), failure::Error> {
    let radon_types_vec: Vec<RadonTypes> = reveals
        .iter()
        .filter_map(|input| RadonTypes::try_from(input.as_slice()).ok())
        .collect();

    let local_tally = run_consensus(radon_types_vec, tally_stage)?;

    if local_tally == miner_tally {
        Ok(())
    } else {
        Err(TransactionError::MismatchedConsensus {
            local_tally,
            miner_tally,
        })?
    }
}

/// Function to validate a data request transaction
pub fn validate_dr_transaction(tx: &TransactionBody) -> Result<(), failure::Error> {
    if tx.outputs.len() != 1 {
        Err(TransactionError::InvalidDataRequestTransaction)?
    }

    if let Output::DataRequest(dr_output) = &tx.outputs[0] {
        if dr_output.witnesses < 2 {
            Err(TransactionError::InsufficientWitnesses)?
        }

        let witnesses = i64::from(dr_output.witnesses);
        let dr_value = dr_output.value as i64;
        let commit_fee = dr_output.commit_fee as i64;
        let reveal_fee = dr_output.reveal_fee as i64;
        let tally_fee = dr_output.tally_fee as i64;

        let witness_reward = ((dr_value - tally_fee) / witnesses) - commit_fee - reveal_fee;
        if witness_reward <= 0 {
            Err(TransactionError::InvalidDataRequestReward {
                reward: witness_reward,
            })?
        }

        validate_rad_request(&dr_output.data_request)
    } else {
        Err(TransactionError::InvalidDataRequestTransaction)?
    }
}

// Add 1 in the number assigned to a DataRequestOutput
pub fn update_count<S: ::std::hash::BuildHasher>(
    mut hm: HashMap<DataRequestOutput, u32, S>,
    k: &DataRequestOutput,
) {
    match hm.get_mut(k) {
        Some(count) => {
            *count += 1;
        }
        None => {
            hm.insert(k.clone(), 1);
        }
    };
}

/// Function to validate a commit transaction
pub fn validate_commit_transaction<S: ::std::hash::BuildHasher>(
    tx: &TransactionBody,
    dr_pool: &DataRequestPool,
    block_commits: HashMap<DataRequestOutput, u32, S>,
    fee: u64,
) -> Result<(), failure::Error> {
    if (tx.inputs.len() != 1) || (tx.outputs.len() != 1) {
        Err(TransactionError::InvalidCommitTransaction)?
    }

    match &tx.inputs[0] {
        Input::DataRequest(dr_input) => {
            // TODO: Complete PoE validation
            let _poe = dr_input.poe;
            if !verify_poe_data_request() {
                Err(TransactionError::InvalidDataRequestPoe)?
            }

            // Get DataRequest information
            let dr_pointer = dr_input.output_pointer();
            let dr_state = dr_pool.data_request_pool.get(&dr_pointer).ok_or(
                TransactionError::OutputNotFound {
                    output: dr_pointer.clone(),
                },
            )?;

            // Validate fee
            let expected_commit_fee = dr_state.data_request.commit_fee;
            if fee != expected_commit_fee {
                Err(TransactionError::InvalidFee {
                    fee,
                    expected_fee: expected_commit_fee,
                })?
            }

            // Accumulate commits number
            update_count(block_commits, &dr_state.data_request);

            Ok(())
        }
        _ => Err(TransactionError::NotDataRequestInputInCommit)?,
    }
}

/// Function to validate a reveal transaction
pub fn validate_reveal_transaction<S: ::std::hash::BuildHasher>(
    tx: &TransactionBody,
    dr_pool: &DataRequestPool,
    block_reveals: HashMap<DataRequestOutput, u32, S>,
    fee: u64,
) -> Result<(), failure::Error> {
    if (tx.inputs.len() != 1) || (tx.outputs.len() != 1) {
        Err(TransactionError::InvalidRevealTransaction)?
    }

    match &tx.inputs[0] {
        Input::Commit(commit_input) => {
            // Get DataRequest information
            let commit_pointer = commit_input.output_pointer();
            let dr_pointer = dr_pool.dr_pointer_cache.get(&commit_pointer).ok_or(
                TransactionError::OutputNotFound {
                    output: commit_pointer.clone(),
                },
            )?;
            let dr_state = dr_pool.data_request_pool.get(&dr_pointer).ok_or(
                TransactionError::OutputNotFound {
                    output: dr_pointer.clone(),
                },
            )?;

            // Validate fee
            let expected_reveal_fee = dr_state.data_request.reveal_fee;
            if fee != expected_reveal_fee {
                Err(TransactionError::InvalidFee {
                    fee,
                    expected_fee: expected_reveal_fee,
                })?
            }

            // TODO: Validate commitment

            // Accumulate commits number
            update_count(block_reveals, &dr_state.data_request);

            Ok(())
        }
        _ => Err(TransactionError::NotCommitInputInReveal)?,
    }
}

/// Function to validate a tally transaction
pub fn validate_tally_transaction(
    tx: &TransactionBody,
    dr_pool: &DataRequestPool,
    utxo: &UnspentOutputsPool,
    fee: u64,
) -> Result<(()), failure::Error> {
    if (tx.outputs.len() - tx.inputs.len()) != 1 {
        Err(TransactionError::InvalidTallyTransaction)?
    }

    let mut reveals: Vec<Vec<u8>> = vec![];
    let mut dr_pointer_aux = &OutputPointer {
        transaction_id: Hash::default(),
        output_index: 0,
    };

    for input in &tx.inputs {
        match input {
            Input::Reveal(reveal_input) => {
                // Get DataRequest information
                let reveal_pointer = reveal_input.output_pointer();

                let dr_pointer = dr_pool.dr_pointer_cache.get(&reveal_pointer).ok_or(
                    TransactionError::OutputNotFound {
                        output: reveal_pointer.clone(),
                    },
                )?;

                if dr_pointer_aux.transaction_id == Hash::default() {
                    dr_pointer_aux = dr_pointer;
                } else if dr_pointer_aux != dr_pointer {
                    Err(TransactionError::RevealsFromDifferentDataRequest)?
                }

                match utxo.get(&reveal_pointer) {
                    Some(Output::Reveal(reveal_output)) => {
                        reveals.push(reveal_output.reveal.clone())
                    }
                    _ => Err(TransactionError::OutputNotFound {
                        output: reveal_pointer.clone(),
                    })?,
                }
            }

            _ => Err(TransactionError::NotRevealInputInTally)?,
        }
    }

    // Get DataRequestState
    let dr_state =
        dr_pool
            .data_request_pool
            .get(dr_pointer_aux)
            .ok_or(TransactionError::OutputNotFound {
                output: dr_pointer_aux.clone(),
            })?;

    // Validate fee
    let expected_tally_fee = dr_state.data_request.tally_fee;
    if fee != expected_tally_fee {
        Err(TransactionError::InvalidFee {
            fee,
            expected_fee: expected_tally_fee,
        })?
    }

    //TODO: Check Tally convergence

    // Validate tally result
    if let Some(Output::Tally(tally_output)) = tx.outputs.last() {
        let miner_tally = tally_output.result.clone();
        let tally_stage = dr_state.data_request.data_request.consensus.script.clone();

        validate_consensus(reveals, miner_tally, tally_stage)
    } else {
        Err(TransactionError::InvalidTallyTransaction)?
    }
}
/// Function to validate a signature
pub fn validate_transaction_signature(
    input: &Input,
    keyed_signature: &KeyedSignature,
) -> Result<(), failure::Error> {
    let tx_hash = match input {
        Input::Commit(i) => i.transaction_id,
        Input::Reveal(i) => i.transaction_id,
        Input::ValueTransfer(i) => i.transaction_id,
        Input::DataRequest(i) => i.transaction_id,
    };

    let Hash::SHA256(message) = tx_hash;

    let signature = match keyed_signature.signature.clone() {
        Signature::Secp256k1(s) => s.try_into()?,
    };
    let public_key = keyed_signature.public_key.clone().try_into()?;

    verify(public_key, &message, signature)
}

/// Function to validate signatures
pub fn validate_transaction_signatures(
    inputs: Vec<Input>,
    signatures: Vec<KeyedSignature>,
) -> Result<(), failure::Error> {
    if signatures.len() != inputs.len() {
        Err(TransactionError::MismatchingSignaturesNumber {
            signatures_n: signatures.len() as u8,
            inputs_n: inputs.len() as u8,
        })?
    }

    for (input, keyed_signature) in inputs.iter().zip(signatures.iter()) {
        validate_transaction_signature(input, keyed_signature)?
    }

    Ok(())
}

/// Function to validate a transaction
pub fn validate_transaction(
    _transaction: &Transaction,
    _utxo_set: &UnspentOutputsPool,
) -> Result<(), failure::Error> {
    //let _fee = transaction_fee(transaction, utxo_set)?;
    // TODO(#519) Validate any kind of transaction

    Ok(())
}

/// Function to validate transactions in a block and update a utxo_set and a `TransactionsPool`
// TODO: Add verifications related to data requests (e.g. enough commitment transactions for a data request)
pub fn validate_transactions(
    utxo_set: &UnspentOutputsPool,
    _txn_pool: &TransactionsPool,
    data_request_pool: &DataRequestPool,
    block: &Block,
) -> Result<BlockInChain, failure::Error> {
    // TODO: Add validate_mint function

    let mut utxo_set = utxo_set.clone();
    let mut data_request_pool = data_request_pool.clone();

    let transactions = block.txns.clone();

    let mut remove_later = vec![];

    // TODO: replace for loop with a try_fold
    for transaction in &transactions {
        match validate_transaction(&transaction, &utxo_set) {
            Ok(_) => {
                let txn_hash = transaction.hash();

                for input in &transaction.body.inputs {
                    // Obtain the OuputPointer of each input and remove it from the utxo_set
                    let output_pointer = input.output_pointer();
                    match input {
                        Input::DataRequest(..) => {
                            remove_later.push(output_pointer);
                        }
                        _ => {
                            utxo_set.remove(&output_pointer);
                        }
                    }
                }

                for (index, output) in transaction.body.outputs.iter().enumerate() {
                    // Add the new outputs to the utxo_set
                    let output_pointer = OutputPointer {
                        transaction_id: txn_hash,
                        output_index: index as u32,
                    };

                    utxo_set.insert(output_pointer, output.clone());
                }

                // Add DataRequests from the block into the data_request_pool
                data_request_pool.process_transaction(
                    transaction,
                    block.block_header.beacon.checkpoint,
                    &block.hash(),
                );
            }
            Err(e) => Err(e)?,
        }
    }

    for output_pointer in remove_later {
        utxo_set.remove(&output_pointer);
    }

    Ok(BlockInChain {
        block: block.clone(),
        utxo_set,
        data_request_pool,
    })
}

/// Function to validate a block
pub fn validate_block(
    block: &Block,
    current_epoch: Epoch,
    chain_beacon: CheckpointBeacon,
    genesis_block_hash: Hash,
    utxo_set: &UnspentOutputsPool,
    txn_pool: &TransactionsPool,
    data_request_pool: &DataRequestPool,
) -> Result<BlockInChain, failure::Error> {
    let block_epoch = block.block_header.beacon.checkpoint;
    let hash_prev_block = block.block_header.beacon.hash_prev_block;

    if !verify_poe_block() {
        Err(BlockError::NotValidPoe)?
    } else if !validate_merkle_tree(&block) {
        Err(BlockError::NotValidMerkleTree)?
    } else if block_epoch > current_epoch {
        Err(BlockError::BlockFromFuture {
            block_epoch,
            current_epoch,
        })?
    } else if chain_beacon.checkpoint > block_epoch {
        Err(BlockError::BlockOlderThanTip {
            chain_epoch: chain_beacon.checkpoint,
            block_epoch,
        })?
    } else if hash_prev_block != genesis_block_hash
        && chain_beacon.hash_prev_block != hash_prev_block
    {
        Err(BlockError::PreviousHashNotKnown {
            hash: hash_prev_block,
        })?
    } else {
        validate_transactions(&utxo_set, &txn_pool, &data_request_pool, &block)
    }
}

/// Function to validate a block candidate
pub fn validate_candidate(block: &Block, current_epoch: Epoch) -> Result<(), failure::Error> {
    let block_epoch = block.block_header.beacon.checkpoint;

    if !verify_poe_block() {
        Err(BlockError::NotValidPoe)?
    } else if block_epoch != current_epoch {
        Err(BlockError::CandidateFromDifferentEpoch {
            block_epoch,
            current_epoch,
        })?
    } else {
        Ok(())
    }
}

/// Function to assign tags to transactions
pub fn transaction_tag(tx: &TransactionBody) -> TransactionType {
    match tx.outputs.last() {
        Some(Output::DataRequest(_)) => TransactionType::DataRequest,
        Some(Output::ValueTransfer(_)) => {
            if transaction_is_mint(tx) {
                TransactionType::Mint
            } else {
                TransactionType::ValueTransfer
            }
        }
        Some(Output::Commit(_)) => TransactionType::Commit,
        Some(Output::Reveal(_)) => TransactionType::Reveal,
        Some(Output::Tally(_)) => TransactionType::Tally,
        None => TransactionType::InvalidType,
    }
}

/// Function to calculate a merkle tree from a transaction vector
pub fn merkle_tree_root<T>(transactions: &[T]) -> Hash
where
    T: std::convert::AsRef<Transaction> + Hashable,
{
    let transactions_hashes: Vec<Sha256> = transactions
        .iter()
        .map(|x| match x.hash() {
            Hash::SHA256(x) => Sha256(x),
        })
        .collect();

    Hash::from(crypto_merkle_tree_root(&transactions_hashes))
}

/// Function to validate block's merkle tree
pub fn validate_merkle_tree(block: &Block) -> bool {
    let merkle_tree = block.block_header.hash_merkle_root;
    let transactions = &block.txns;

    merkle_tree == merkle_tree_root(transactions)
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
// TODO: Implement logic for this function
pub fn verify_poe_block() -> bool {
    true
}

/// Function to check poe validation for data requests
// TODO: Implement logic for this function
pub fn verify_poe_data_request() -> bool {
    true
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
