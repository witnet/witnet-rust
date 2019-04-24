use std::collections::{HashMap, HashSet};

use witnet_crypto::{
    hash::Sha256,
    merkle::{merkle_tree_root as crypto_merkle_tree_root, ProgressiveMerkleTree},
    signature::verify,
};
use witnet_data_structures::{
    chain::{
        transaction_is_mint, transaction_tag, Block, CheckpointBeacon, Epoch, Hash, Hashable,
        Input, KeyedSignature, Output, OutputPointer, PublicKeyHash, RADRequest, Transaction,
        TransactionBody, TransactionType, UnspentOutputsPool,
    },
    data_request::DataRequestPool,
    error::{BlockError, TransactionError},
    serializers::decoders::{TryFrom, TryInto},
};
use witnet_rad::{run_consensus, script::unpack_radon_script, types::RadonTypes};

/// Calculate the sum of the values of the outputs pointed by the
/// inputs of a transaction. If an input pointed-output is not
/// found in `pool`, then an error is returned instead indicating
/// it. If a Signature is invalid an error is returned too
pub fn transaction_inputs_sum(
    tx: &TransactionBody,
    utxo_diff: &UtxoDiff,
) -> Result<u64, failure::Error> {
    let mut total_value = 0;

    match transaction_tag(tx) {
        TransactionType::Commit => {
            total_value = calculate_commit_input(&tx, utxo_diff)?;
        }
        TransactionType::Reveal => {
            total_value = calculate_reveal_input(&tx, utxo_diff)?;
        }
        _ => {
            for input in &tx.inputs {
                let pointed_value = utxo_diff
                    .get(&input.output_pointer())
                    .ok_or_else(|| TransactionError::OutputNotFound {
                        output: input.output_pointer(),
                    })?
                    .value();
                total_value += pointed_value;
            }
        }
    }

    Ok(total_value)
}

fn calculate_commit_input(
    tx: &TransactionBody,
    utxo_diff: &UtxoDiff,
) -> Result<u64, failure::Error> {
    let dr_input = &tx.inputs[0];
    // Get DataRequest information
    let dr_pointer = dr_input.output_pointer();
    let dr_output = utxo_diff
        .get(&dr_pointer)
        .ok_or(TransactionError::OutputNotFound {
            output: dr_pointer.clone(),
        })?;

    match dr_output {
        Output::DataRequest(dr_state) => Ok(dr_state.value / u64::from(dr_state.witnesses)),
        _ => Err(TransactionError::InvalidCommitTransaction)?,
    }
}

fn calculate_reveal_input(
    tx: &TransactionBody,
    utxo_diff: &UtxoDiff,
) -> Result<u64, failure::Error> {
    let dr_input = &tx.inputs[0];
    // Get DataRequest information
    let dr_pointer = dr_input.output_pointer();
    let dr_output = utxo_diff
        .get(&dr_pointer)
        .ok_or(TransactionError::OutputNotFound {
            output: dr_pointer.clone(),
        })?;

    match dr_output {
        Output::DataRequest(dr_state) => {
            Ok((dr_state.value / u64::from(dr_state.witnesses)) - dr_state.commit_fee)
        }
        _ => Err(TransactionError::InvalidCommitTransaction)?,
    }
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
pub fn transaction_fee(tx: &TransactionBody, utxo_diff: &UtxoDiff) -> Result<u64, failure::Error> {
    let in_value = transaction_inputs_sum(tx, utxo_diff)?;
    let out_value = transaction_outputs_sum(tx);

    if out_value > in_value {
        Err(TransactionError::NegativeFee)?
    } else {
        Ok(in_value - out_value)
    }
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

/// Function to validate a value transfer transaction
pub fn validate_vt_transaction(_tx: &TransactionBody) -> Result<(), failure::Error> {
    // TODO(#514): Implement value transfer transaction validation
    Ok(())
}

/// Function to validate a data request transaction
pub fn validate_dr_transaction(tx: &TransactionBody) -> Result<(), failure::Error> {
    if let Some(Output::DataRequest(dr_output)) = &tx.outputs.last() {
        if dr_output.witnesses < 1 {
            Err(TransactionError::InsufficientWitnesses)?
        }

        let witnesses = i64::from(dr_output.witnesses);
        let dr_value = dr_output.value as i64;
        let commit_fee = dr_output.commit_fee as i64;
        let reveal_fee = dr_output.reveal_fee as i64;
        let tally_fee = dr_output.tally_fee as i64;

        if ((dr_value - tally_fee) % witnesses) != 0 {
            Err(TransactionError::InvalidDataRequestValue {
                dr_value,
                witnesses,
            })?
        }

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

// Add 1 in the number assigned to a OutputPointer
pub fn increment_witnesses_counter<S: ::std::hash::BuildHasher>(
    hm: &mut WitnessesCounter<S>,
    k: &OutputPointer,
    rf: u32,
) {
    hm.entry(k.clone())
        .or_insert(WitnessesCount {
            current: 0,
            target: rf,
        })
        .current += 1;
}

/// HashMap to count commit transactions need for a Data Request
pub struct WitnessesCount {
    current: u32,
    target: u32,
}
pub type WitnessesCounter<S> = HashMap<OutputPointer, WitnessesCount, S>;

/// Function to validate a commit transaction
pub fn validate_commit_transaction<S: ::std::hash::BuildHasher>(
    tx: &TransactionBody,
    dr_pool: &DataRequestPool,
    block_commits: &mut WitnessesCounter<S>,
    fee: u64,
) -> Result<(), failure::Error> {
    if (tx.inputs.len() != 1) || (tx.outputs.len() != 1) {
        Err(TransactionError::InvalidCommitTransaction)?
    }

    let commit_input = &tx.inputs[0];

    // TODO: Complete PoE validation
    if !verify_poe_data_request() {
        Err(TransactionError::InvalidDataRequestPoe)?
    }

    // Get DataRequest information
    let dr_pointer = commit_input.output_pointer();
    let dr_state =
        dr_pool
            .data_request_pool
            .get(&dr_pointer)
            .ok_or(TransactionError::OutputNotFound {
                output: dr_pointer.clone(),
            })?;

    // Validate fee
    let expected_commit_fee = dr_state.data_request.commit_fee;
    if fee != expected_commit_fee {
        Err(TransactionError::InvalidFee {
            fee,
            expected_fee: expected_commit_fee,
        })?
    }

    // Accumulate commits number
    increment_witnesses_counter(
        block_commits,
        &dr_pointer,
        u32::from(dr_state.data_request.witnesses),
    );

    Ok(())
}

/// Function to validate a reveal transaction
pub fn validate_reveal_transaction(
    tx: &TransactionBody,
    dr_pool: &DataRequestPool,
    fee: u64,
) -> Result<(), failure::Error> {
    if (tx.inputs.len() != 1) || (tx.outputs.len() != 1) {
        Err(TransactionError::InvalidRevealTransaction)?
    }

    let reveal_input = &tx.inputs[0];
    // Get DataRequest information
    let dr_pointer = reveal_input.output_pointer();
    let dr_state =
        dr_pool
            .data_request_pool
            .get(&dr_pointer)
            .ok_or(TransactionError::OutputNotFound {
                output: dr_pointer.clone(),
            })?;

    // Validate fee
    let expected_reveal_fee = dr_state.data_request.reveal_fee;
    if fee != expected_reveal_fee {
        Err(TransactionError::InvalidFee {
            fee,
            expected_fee: expected_reveal_fee,
        })?
    }

    // TODO: Validate commitment

    Ok(())
}

/// Function to validate a tally transaction
pub fn validate_tally_transaction(
    tx: &TransactionBody,
    dr_pool: &DataRequestPool,
    fee: u64,
) -> Result<(()), failure::Error> {
    if tx.inputs.len() != 1 {
        Err(TransactionError::InvalidTallyTransaction)?
    }

    let mut reveals: Vec<Vec<u8>> = vec![];

    let dr_pointer = &tx.inputs[0].output_pointer();
    let all_reveals = dr_pool.get_reveals(dr_pointer);

    if let Some(all_reveals) = all_reveals {
        reveals.extend(all_reveals.iter().map(|reveal| reveal.reveal.clone()));
    } else {
        Err(TransactionError::InvalidTallyTransaction)?
    }

    // Get DataRequestState
    let dr_state =
        dr_pool
            .data_request_pool
            .get(dr_pointer)
            .ok_or(TransactionError::OutputNotFound {
                output: dr_pointer.clone(),
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

/// Function to validate a block signature
pub fn validate_block_signature(block: &Block) -> Result<(), failure::Error> {
    let keyed_signature = &block.proof.block_sig;

    let signature = keyed_signature.signature.clone().try_into()?;
    let public_key = keyed_signature.public_key.clone().try_into()?;

    let Hash::SHA256(message) = block.block_header.beacon.hash();

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
    // TODO: for now only validate value transfer outputs
    if let Some(Output::ValueTransfer(x)) = output {
        let signature_pkh = PublicKeyHash::from_public_key(&keyed_signature.public_key);
        let expected_pkh = x.pkh;
        if signature_pkh != expected_pkh {
            Err(TransactionError::PublicKeyHashMismatch {
                expected_pkh,
                signature_pkh,
            })?
        }
    }
    Ok(())
}

/// Function to validate transaction signatures
pub fn validate_transaction_signatures(
    transaction: &Transaction,
    utxo_set: &UtxoDiff,
) -> Result<(), failure::Error> {
    let signatures = &transaction.signatures;
    let inputs = &transaction.body.inputs;

    if signatures.len() != inputs.len() {
        Err(TransactionError::MismatchingSignaturesNumber {
            signatures_n: signatures.len() as u8,
            inputs_n: inputs.len() as u8,
        })?
    }

    // Validate transaction signature
    if let Some(tx_keyed_signature) = signatures.get(0) {
        let signature = tx_keyed_signature.signature.clone().try_into()?;
        let public_key = tx_keyed_signature.public_key.clone().try_into()?;
        let Hash::SHA256(message) = transaction.hash();

        verify(&public_key, &message, &signature).map_err(|_| {
            TransactionError::VerifyTransactionSignatureFail {
                hash: transaction.hash(),
                index: 0,
            }
        })?;
    } else {
        Err(TransactionError::SignatureNotFound)?
    }

    for (input, keyed_signature) in inputs.iter().zip(signatures.iter().next()) {
        validate_pkh_signature(input, keyed_signature, utxo_set)?
    }

    Ok(())
}

/// Function to validate a transaction
pub fn validate_transaction<S: ::std::hash::BuildHasher>(
    transaction: &Transaction,
    utxo_diff: &UtxoDiff,
    dr_pool: &DataRequestPool,
    block_commits: &mut WitnessesCounter<S>,
) -> Result<u64, failure::Error> {
    validate_transaction_signatures(&transaction, utxo_diff)?;

    match transaction_tag(&transaction.body) {
        TransactionType::Mint => Err(TransactionError::UnexpectedMint)?,
        TransactionType::InvalidType => Err(TransactionError::NotValidTransaction)?,
        TransactionType::ValueTransfer => {
            let fee = transaction_fee(&transaction.body, utxo_diff)?;

            validate_vt_transaction(&transaction.body)?;
            Ok(fee)
        }
        TransactionType::DataRequest => {
            let fee = transaction_fee(&transaction.body, utxo_diff)?;

            validate_dr_transaction(&transaction.body)?;
            Ok(fee)
        }
        TransactionType::Commit => {
            let fee = transaction_fee(&transaction.body, utxo_diff)?;

            validate_commit_transaction(&transaction.body, dr_pool, block_commits, fee)?;
            Ok(fee)
        }
        TransactionType::Reveal => {
            let fee = transaction_fee(&transaction.body, utxo_diff)?;

            validate_reveal_transaction(&transaction.body, dr_pool, fee)?;
            Ok(fee)
        }
        TransactionType::Tally => {
            let fee = transaction_fee(&transaction.body, utxo_diff)?;

            validate_tally_transaction(&transaction.body, dr_pool, fee)?;
            Ok(fee)
        }
    }
}

/// Function to validate transactions in a block and update a utxo_set and a `TransactionsPool`
pub fn validate_transactions(
    utxo_set: &UnspentOutputsPool,
    data_request_pool: &DataRequestPool,
    block: &Block,
) -> Result<Diff, failure::Error> {
    // Init Progressive merkle tree
    let mut mt = ProgressiveMerkleTree::sha256();

    match block.txns.get(0).map(|tx| {
        let Hash::SHA256(sha) = tx.hash();
        mt.push(Sha256(sha));
        transaction_tag(&tx.body)
    }) {
        Some(TransactionType::Mint) => (),
        _ => Err(BlockError::NoMint)?,
    }

    let mut utxo_diff = UtxoDiff::new(utxo_set);
    let mut commits_number: WitnessesCounter<_> = HashMap::new();

    // Init total fee
    let mut total_fee = 0;

    // TODO: replace for loop with a try_fold
    for transaction in &block.txns[1..] {
        match validate_transaction(
            &transaction,
            &utxo_diff,
            &data_request_pool,
            &mut commits_number,
        ) {
            Ok(fee) => {
                // Add transaction fee
                total_fee += fee;

                // Add new hash to merkle tree
                let txn_hash = transaction.hash();
                let Hash::SHA256(sha) = txn_hash;
                mt.push(Sha256(sha));

                for input in &transaction.body.inputs {
                    // Obtain the OuputPointer of each input and remove it from the utxo_diff
                    let output_pointer = input.output_pointer();
                    if let TransactionType::ValueTransfer
                    | TransactionType::DataRequest
                    | TransactionType::Tally = transaction_tag(&transaction.body)
                    {
                        utxo_diff.remove_utxo(output_pointer);
                    }
                }

                for (index, output) in transaction.body.outputs.iter().enumerate() {
                    // Add the new outputs to the utxo_diff
                    let output_pointer = OutputPointer {
                        transaction_id: txn_hash,
                        output_index: index as u32,
                    };

                    if let TransactionType::ValueTransfer
                    | TransactionType::DataRequest
                    | TransactionType::Tally = transaction_tag(&transaction.body)
                    {
                        utxo_diff.insert_utxo(output_pointer, output.clone());
                    }
                }
            }
            Err(e) => Err(e)?,
        }
    }

    // Validate mint
    validate_mint_transaction(
        &block.txns[0].body,
        total_fee,
        block_reward(block.block_header.beacon.checkpoint),
    )?;

    // Insert mint in utxo
    let mint_output_pointer = OutputPointer {
        transaction_id: block.txns[0].hash(),
        output_index: 0,
    };
    let mint_output = block.txns[0].body.outputs[0].clone();
    utxo_diff.insert_utxo(mint_output_pointer, mint_output);

    // Validate commits number
    for WitnessesCount { current, target } in commits_number.values() {
        if current != target {
            Err(BlockError::MismatchingCommitsNumber {
                commits: *current,
                rf: *target,
            })?
        }
    }

    // Validate Merkle Root
    let Hash::SHA256(mr) = block.block_header.hash_merkle_root;
    if mt.root() != Sha256(mr) {
        Err(BlockError::NotValidMerkleTree)?
    }

    Ok(utxo_diff.take_diff())
}

/// Function to validate a block
pub fn validate_block(
    block: &Block,
    current_epoch: Epoch,
    chain_beacon: CheckpointBeacon,
    genesis_block_hash: Hash,
    utxo_set: &UnspentOutputsPool,
    data_request_pool: &DataRequestPool,
) -> Result<Diff, failure::Error> {
    let block_epoch = block.block_header.beacon.checkpoint;
    let hash_prev_block = block.block_header.beacon.hash_prev_block;

    if block_epoch > current_epoch {
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
    } else if !verify_poe_block() {
        Err(BlockError::NotValidPoe)?
    } else {
        validate_block_signature(&block)?;

        validate_transactions(&utxo_set, &data_request_pool, &block)
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

/// Diffs to apply to an utxo set. This type does not contains a
/// reference to the original utxo set.
#[derive(Default)]
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
        F1: Fn(&mut A, &OutputPointer, &Output) -> (),
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
    pub fn insert_utxo(&mut self, output_pointer: OutputPointer, output: Output) {
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
    pub fn get(&self, output_pointer: &OutputPointer) -> Option<&Output> {
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
