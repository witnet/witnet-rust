use std::{
    collections::{HashMap, HashSet},
    convert::{TryFrom, TryInto},
};

use witnet_crypto::{
    hash::Sha256,
    merkle::{merkle_tree_root as crypto_merkle_tree_root, ProgressiveMerkleTree},
    signature::verify,
};
use witnet_data_structures::{
    chain::{
        Block, CheckpointBeacon, Epoch, Hash, Hashable, Input, KeyedSignature, OutputPointer,
        PublicKeyHash, RADConsensus, RADRequest, UnspentOutputsPool, ValueTransferOutput,
    },
    data_request::DataRequestPool,
    error::{BlockError, TransactionError},
    transaction::{
        mint, CommitTransactionBody, DRTransaction, DRTransactionBody, MintTransaction,
        RevealTransactionBody, TallyTransaction, Transaction, VTTransaction, VTTransactionBody,
    },
};
use witnet_rad::{run_consensus, script::unpack_radon_script, types::RadonTypes};

/// Calculate the sum of the values of the outputs pointed by the
/// inputs of a transaction. If an input pointed-output is not
/// found in `pool`, then an error is returned instead indicating
/// it. If a Signature is invalid an error is returned too
pub fn transaction_inputs_sum(
    inputs: &[Input],
    utxo_diff: &UtxoDiff,
) -> Result<u64, failure::Error> {
    let mut total_value = 0;

    for input in inputs {
        let vt_output = utxo_diff.get(&input.output_pointer()).ok_or_else(|| {
            TransactionError::OutputNotFound {
                output: input.output_pointer().clone(),
            }
        })?;
        total_value += vt_output.value;
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
) -> Result<u64, failure::Error> {
    let in_value = transaction_inputs_sum(&vt_tx.body.inputs, utxo_diff)?;
    let out_value = transaction_outputs_sum(&vt_tx.body.outputs);

    if out_value > in_value {
        Err(TransactionError::NegativeFee)?
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
) -> Result<u64, failure::Error> {
    let in_value = transaction_inputs_sum(&dr_tx.body.inputs, utxo_diff)?;
    let out_value = transaction_outputs_sum(&dr_tx.body.outputs) + dr_tx.body.dr_output.value;

    if out_value > in_value {
        Err(TransactionError::NegativeFee)?
    } else {
        Ok(in_value - out_value)
    }
}

/// Function to validate a mint transaction
pub fn validate_mint_transaction(
    mint_tx: &MintTransaction,
    total_fees: u64,
    block_reward: u64,
) -> Result<(), failure::Error> {
    let mint_value = transaction_outputs_sum(&mint_tx.outputs);

    if mint_value != total_fees + block_reward {
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

    let local_tally = run_consensus(
        radon_types_vec,
        &RADConsensus {
            script: tally_stage,
        },
    )?;

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
pub fn validate_vt_transaction(_tx: &VTTransactionBody) -> Result<(), failure::Error> {
    // TODO(#514): Implement value transfer transaction validation
    Ok(())
}

/// Function to validate a data request transaction
pub fn validate_dr_transaction(tx: &DRTransactionBody) -> Result<(), failure::Error> {
    let dr_output = &tx.dr_output;

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

    // TODO: Review after use a unique commit/reveal fee for group not for each
    let witness_reward = ((dr_value - tally_fee) / witnesses) - commit_fee - reveal_fee;
    if witness_reward <= 0 {
        Err(TransactionError::InvalidDataRequestReward {
            reward: witness_reward,
        })?
    }

    validate_rad_request(&dr_output.data_request)
}

/// Function to validate a commit transaction
pub fn validate_commit_transaction(
    tx: &CommitTransactionBody,
    dr_pool: &DataRequestPool,
) -> Result<(Hash, u16, u64), failure::Error> {
    // TODO: Complete PoE validation
    if !verify_poe_data_request() {
        Err(TransactionError::InvalidDataRequestPoe)?
    }

    // Get DataRequest information
    let dr_pointer = tx.dr_pointer;
    let dr_output = dr_pool
        .get_dr_output(&dr_pointer)
        .ok_or(TransactionError::DataRequestNotFound { hash: dr_pointer })?;

    Ok((dr_pointer, dr_output.witnesses, dr_output.commit_fee))
}

/// Function to validate a reveal transaction
pub fn validate_reveal_transaction(
    tx: &RevealTransactionBody,
    dr_pool: &DataRequestPool,
) -> Result<(Hash, u16, u64), failure::Error> {
    // Get DataRequest information
    let dr_pointer = tx.dr_pointer;
    let dr_output = dr_pool
        .get_dr_output(&dr_pointer)
        .ok_or(TransactionError::DataRequestNotFound { hash: dr_pointer })?;

    // TODO: Validate commitment

    Ok((dr_pointer, dr_output.witnesses, dr_output.reveal_fee))
}

/// Function to validate a tally transaction
pub fn validate_tally_transaction(
    tx: &TallyTransaction,
    dr_pool: &DataRequestPool,
) -> Result<(u64), failure::Error> {
    let mut reveals: Vec<Vec<u8>> = vec![];

    let dr_pointer = tx.dr_pointer;
    let all_reveals = dr_pool.get_reveals(&dr_pointer);

    if let Some(all_reveals) = all_reveals {
        reveals.extend(all_reveals.iter().map(|reveal| reveal.body.reveal.clone()));
    } else {
        Err(TransactionError::InvalidTallyTransaction)?
    }

    // Get DataRequestState
    let dr_output = dr_pool
        .get_dr_output(&dr_pointer)
        .ok_or(TransactionError::DataRequestNotFound { hash: dr_pointer })?;

    //TODO: Check Tally convergence

    // Validate tally result
    let miner_tally = tx.tally.clone();
    let tally_stage = dr_output.data_request.consensus.script.clone();

    validate_consensus(reveals, miner_tally, tally_stage)?;

    Ok(dr_output.tally_fee)
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
    if let Some(x) = output {
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
/// Function to validate a commit/reveal transaction signature
pub fn validate_commit_reveal_signature(
    tx_hash: Hash,
    signatures: &[KeyedSignature],
) -> Result<(), failure::Error> {
    if let Some(tx_keyed_signature) = signatures.get(0) {
        let signature = tx_keyed_signature.signature.clone().try_into()?;
        let public_key = tx_keyed_signature.public_key.clone().try_into()?;
        let Hash::SHA256(message) = tx_hash;

        verify(&public_key, &message, &signature).map_err(|_| {
            TransactionError::VerifyTransactionSignatureFail {
                hash: tx_hash,
                index: 0,
            }
        })?;
    } else {
        Err(TransactionError::SignatureNotFound)?
    }

    Ok(())
}

/// Function to validate a transaction signature
pub fn validate_transaction_signature(
    signatures: &[KeyedSignature],
    inputs: &[Input],
    utxo_set: &UtxoDiff,
) -> Result<(), failure::Error> {
    if signatures.len() != inputs.len() {
        Err(TransactionError::MismatchingSignaturesNumber {
            signatures_n: signatures.len() as u8,
            inputs_n: inputs.len() as u8,
        })?
    }

    for (input, keyed_signature) in inputs.iter().zip(signatures.iter()) {
        validate_pkh_signature(input, keyed_signature, utxo_set)?
    }

    Ok(())
}

/// Function to validate transaction signatures
pub fn validate_transaction_signatures(
    transaction: &Transaction,
    utxo_set: &UtxoDiff,
) -> Result<(), failure::Error> {
    match transaction {
        Transaction::ValueTransfer(tx) => {
            validate_transaction_signature(&tx.signatures, &tx.body.inputs, utxo_set)
        }
        Transaction::DataRequest(tx) => {
            validate_transaction_signature(&tx.signatures, &tx.body.inputs, utxo_set)
        }
        Transaction::Commit(tx) => {
            validate_commit_reveal_signature(transaction.hash(), &tx.signatures)
        }
        Transaction::Reveal(tx) => {
            validate_commit_reveal_signature(transaction.hash(), &tx.signatures)
        }
        _ => Ok(()),
    }
}
/// Function to validate a transaction
pub fn validate_transaction<'a>(
    transaction: &'a Transaction,
    utxo_diff: &UtxoDiff,
    dr_pool: &DataRequestPool,
) -> Result<(Vec<&'a Input>, Vec<&'a ValueTransferOutput>, u64), failure::Error> {
    validate_transaction_signatures(&transaction, &utxo_diff)?;

    let empty_inputs: Vec<&Input> = vec![];
    let empty_outputs: Vec<&ValueTransferOutput> = vec![];
    match transaction {
        Transaction::ValueTransfer(vt_tx) => {
            let fee = vt_transaction_fee(vt_tx, utxo_diff)?;
            validate_vt_transaction(&vt_tx.body)?;

            Ok((
                vt_tx.body.inputs.iter().collect(),
                vt_tx.body.outputs.iter().collect(),
                fee,
            ))
        }
        Transaction::DataRequest(dr_tx) => {
            let fee = dr_transaction_fee(dr_tx, utxo_diff)?;
            validate_dr_transaction(&dr_tx.body)?;

            Ok((
                dr_tx.body.inputs.iter().collect(),
                dr_tx.body.outputs.iter().collect(),
                fee,
            ))
        }
        Transaction::Commit(co_tx) => {
            validate_commit_transaction(&co_tx.body, dr_pool)?;

            Ok((empty_inputs, empty_outputs, 0))
        }
        Transaction::Reveal(re_tx) => {
            validate_reveal_transaction(&re_tx.body, dr_pool)?;

            Ok((empty_inputs, empty_outputs, 0))
        }
        Transaction::Tally(ta_tx) => {
            let fee = validate_tally_transaction(&ta_tx, dr_pool)?;

            Ok((empty_inputs, ta_tx.outputs.iter().collect(), fee))
        }
        Transaction::Mint(_) => Err(BlockError::DoubleMint)?,
    }
}

/// HashMap to count commit transactions need for a Data Request
struct WitnessesCount {
    current: u32,
    target: u32,
    fee: u64,
}
type WitnessesCounter<S> = HashMap<Hash, WitnessesCount, S>;

// Add 1 in the number assigned to a OutputPointer
fn increment_witnesses_counter<S: ::std::hash::BuildHasher>(
    hm: &mut WitnessesCounter<S>,
    k: &Hash,
    rf: u32,
    fee: u64,
) {
    hm.entry(k.clone())
        .or_insert(WitnessesCount {
            current: 0,
            target: rf,
            fee,
        })
        .current += 1;
}

/// Function to validate transactions in a block and update a utxo_set and a `TransactionsPool`
pub fn validate_block_transactions(
    utxo_set: &UnspentOutputsPool,
    dr_pool: &DataRequestPool,
    block: &Block,
) -> Result<Diff, failure::Error> {
    // Init Progressive merkle tree
    let mut mt = ProgressiveMerkleTree::sha256();

    let mint = block
        .txns
        .get(0)
        .map(|tx| {
            let Hash::SHA256(sha) = tx.hash();
            mt.push(Sha256(sha));

            tx
        })
        .and_then(|tx| mint(tx))
        .ok_or(BlockError::NoMint)?;

    let mut utxo_diff = UtxoDiff::new(utxo_set);

    // Init total fee
    let mut total_fee = 0;

    let mut commits_number = HashMap::new();
    let mut reveals_number = HashMap::new();

    // TODO: replace for loop with a try_fold
    for transaction in &block.txns[1..] {
        match transaction {
            Transaction::Commit(commit) => {
                let (dr_pointer, dr_witnesses, fee) =
                    validate_commit_transaction(&commit.body, dr_pool)?;

                increment_witnesses_counter(
                    &mut commits_number,
                    &dr_pointer,
                    u32::from(dr_witnesses),
                    fee,
                );
            }

            Transaction::Reveal(reveal) => {
                let (dr_pointer, dr_witnesses, fee) =
                    validate_reveal_transaction(&reveal.body, dr_pool)?;

                increment_witnesses_counter(
                    &mut reveals_number,
                    &dr_pointer,
                    u32::from(dr_witnesses),
                    fee,
                );
            }
            
            _ => {
                let (inputs, outputs, fee) = validate_transaction(transaction, &utxo_diff, dr_pool)?;
                total_fee += fee;

                for input in inputs {
                    // Obtain the OuputPointer of each input and remove it from the utxo_diff
                    let output_pointer = input.output_pointer();

                    utxo_diff.remove_utxo(output_pointer.clone());
                }

                for (index, output) in outputs.into_iter().enumerate() {
                    // Add the new outputs to the utxo_diff
                    let output_pointer = OutputPointer {
                        transaction_id: transaction.hash(),
                        output_index: index as u32,
                    };

                    utxo_diff.insert_utxo(output_pointer, output.clone());
                }
            }
        }

        // Add new hash to merkle tree
        let txn_hash = transaction.hash();
        let Hash::SHA256(sha) = txn_hash;
        mt.push(Sha256(sha));
    }

    // Validate commits number and add commit fees
    for WitnessesCount {
        current,
        target,
        fee,
    } in commits_number.values()
    {
        if current != target {
            Err(BlockError::MismatchingCommitsNumber {
                commits: *current,
                rf: *target,
            })?
        } else {
            total_fee += fee;
        }
    }

    // Add reveal fees
    for WitnessesCount { fee, .. } in reveals_number.values() {
        total_fee += fee;
    }

    // Validate mint
    validate_mint_transaction(
        mint,
        total_fee,
        block_reward(block.block_header.beacon.checkpoint),
    )?;

    // Insert mint in utxo
    let mint_output_pointer = OutputPointer {
        transaction_id: mint.hash(),
        output_index: 0,
    };
    utxo_diff.insert_utxo(mint_output_pointer, mint.outputs[0].clone());

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

        validate_block_transactions(&utxo_set, &data_request_pool, &block)
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
