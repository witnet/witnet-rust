use std::{
    collections::{HashMap, HashSet},
    convert::TryFrom,
    ops::AddAssign,
};

use serde::{Deserialize, Serialize};

use crate::transaction::UnstakeTransactionBody;
use crate::{
    chain::{
        DataRequestOutput, Epoch, EpochConstants, Input, OutputPointer, PublicKeyHash, StakeOutput,
        ValueTransferOutput,
    },
    error::TransactionError,
    fee::{AbsoluteFee, Fee},
    transaction::{DRTransactionBody, StakeTransactionBody, VTTransactionBody, INPUT_SIZE},
    utxo_pool::{
        NodeUtxos, NodeUtxosRef, OwnUnspentOutputsPool, UnspentOutputsPool, UtxoDiff,
        UtxoSelectionStrategy,
    },
    wit::Wit,
};

#[derive(Clone, Debug, Default)]
pub struct CollectedOutputs {
    pub pointers: Vec<OutputPointer>,
    pub resolved: Vec<ValueTransferOutput>,
    pub total_value: u64,
}

/// Structure that resumes the information needed to create a Transaction
#[derive(Clone, Debug)]
pub struct TransactionInfo {
    pub fee: AbsoluteFee,
    pub inputs: CollectedOutputs,
    pub output_value: u64,
    pub outputs: Vec<ValueTransferOutput>,
}

// Structure that includes the confirmed and pending balance of a node
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(untagged)]
pub enum NodeBalance {
    One {
        /// Total amount of a node's funds after last confirmed superblock
        confirmed: Option<u64>,
        /// Total amount of node's funds after last block
        total: u64,
    },
    Many(HashMap<PublicKeyHash, NodeBalance>),
}

impl NodeBalance {
    pub fn add_value(&mut self, value: u64) {
        match self {
            NodeBalance::One { total, .. } => {
                total.add_assign(value);
            }
            NodeBalance::Many(_) => {}
        }
    }

    pub fn get_total(&self) -> Option<Wit> {
        match self {
            NodeBalance::One { total, .. } => Some(Wit::from_nanowits(*total)),
            NodeBalance::Many(_) => None,
        }
    }

    pub fn get_confirmed(&self) -> Option<Wit> {
        match self {
            NodeBalance::One { confirmed, .. } => confirmed.map(Wit::from_nanowits),
            NodeBalance::Many(_) => None,
        }
    }
}

#[derive(Clone, Debug)]
pub enum TransactionOutputs {
    DataRequest((DataRequestOutput, Option<ValueTransferOutput>)),
    Stake((StakeOutput, Option<ValueTransferOutput>)),
    ValueTransfer(Vec<ValueTransferOutput>),
}

impl From<TransactionOutputs> for Vec<ValueTransferOutput> {
    fn from(value: TransactionOutputs) -> Self {
        match value {
            TransactionOutputs::DataRequest((_, change)) => change.into_iter().collect(),
            TransactionOutputs::Stake((_, change)) => change.into_iter().collect(),
            TransactionOutputs::ValueTransfer(outputs) => outputs,
        }
    }
}

/// Abstraction that facilitates the creation of new transactions from a set of unspent outputs.
/// Transaction factories are expected to operate on this trait so that their business logic
/// can be applied on many heterogeneous data structures that may implement it.
pub trait OutputsCollection {
    fn sort_by(&self, strategy: &UtxoSelectionStrategy) -> Vec<OutputPointer>;
    fn get(&self, outptr: &OutputPointer) -> Option<ValueTransferOutput>;
    fn get_usage_timeout(&self, outptr: &OutputPointer) -> Option<u64>;
    fn get_value(&self, outptr: &OutputPointer) -> Option<u64>;
    fn get_included_block_number(&self, outptr: &OutputPointer) -> Option<Epoch>;
    fn set_used_output_pointer(&mut self, outptrs: impl Iterator<Item = Input>, ts: u64);

    /// Select enough UTXOs to sum up to `amount`.
    ///
    /// On success, return a list of output pointers and their sum.
    /// On error, return the total sum of the output pointers in `own_utxos`.
    fn take_enough_utxos(
        &mut self,
        amount: u64,
        timestamp: u64,
        // The block number must be lower than this limit
        block_number_limit: Option<u32>,
        utxo_strategy: &UtxoSelectionStrategy,
    ) -> Result<CollectedOutputs, TransactionError> {
        // FIXME: this is a very naive utxo selection algorithm
        if amount == 0 {
            // Transactions with no inputs make no sense
            return Err(TransactionError::ZeroAmount);
        }

        let mut total_value = 0;
        let mut total: u64 = 0;
        let mut outputs = vec![];
        let mut pointers = vec![];

        let utxo_iter = self.sort_by(utxo_strategy);

        for pointer in utxo_iter.iter() {
            let output: ValueTransferOutput = self.get(pointer).unwrap();
            total = total
                .checked_add(output.value)
                .ok_or(TransactionError::OutputValueOverflow)?;

            if let Some(usage_timeout) = self.get_usage_timeout(pointer) {
                if usage_timeout > timestamp {
                    continue;
                }
            }

            if let Some(block_number_limit) = block_number_limit {
                // Ignore all outputs created after `block_number_limit`.
                // Outputs from the genesis block will never be ignored because `block_number_limit`
                // can't go lower than `0`.
                if let Some(limit) = self.get_included_block_number(pointer) {
                    if limit > block_number_limit {
                        continue;
                    }
                } else {
                    continue;
                }
            }

            total_value += output.value;
            pointers.push(*pointer);
            outputs.push(output);

            if total_value >= amount {
                break;
            }
        }

        if total_value >= amount {
            Ok(CollectedOutputs {
                resolved: outputs,
                pointers,
                total_value,
            })
        } else {
            Err(TransactionError::NoMoney {
                total_balance: total,
                available_balance: total_value,
                transaction_value: amount,
            })
        }
    }

    /// Generic inputs/outputs builder: can be used to build any kind of transaction.
    #[allow(clippy::too_many_arguments)]
    fn generic_transaction_factory(
        &mut self,
        outputs: TransactionOutputs,
        fee: Fee,
        timestamp: u64,
        block_number_limit: Option<u32>,
        utxo_strategy: &UtxoSelectionStrategy,
        max_weight: u32,
    ) -> Result<TransactionInfo, TransactionError> {
        let output_value;
        let mut current_weight;
        let inputs = vec![Input::default()];

        // For the first estimation: 1 input and 1 output more for the change address
        match outputs.clone() {
            TransactionOutputs::DataRequest((dr_output, change)) => {
                let body = DRTransactionBody::new(inputs, dr_output, change.into_iter().collect());
                output_value = body.value()?;
                current_weight = body.weight();
            }
            TransactionOutputs::Stake((stake_output, change)) => {
                let body = StakeTransactionBody::new(inputs, stake_output, change);
                output_value = body.value();
                current_weight = body.weight();
            }
            TransactionOutputs::ValueTransfer(outputs) => {
                let body = VTTransactionBody::new(inputs, outputs);
                output_value = body.value();
                current_weight = body.weight();
            }
        };

        match fee {
            Fee::Absolute(absolute_fee) => {
                let amount = output_value
                    .checked_add(absolute_fee.as_nanowits())
                    .ok_or(TransactionError::FeeOverflow)?;

                let inputs =
                    self.take_enough_utxos(amount, timestamp, block_number_limit, utxo_strategy)?;

                Ok(TransactionInfo {
                    fee: absolute_fee,
                    inputs,
                    output_value,
                    outputs: outputs.into(),
                })
            }
            Fee::Relative(priority) => {
                let absolute_fee = priority.into_absolute(current_weight);

                let max_iterations = 1 + ((max_weight - current_weight) / INPUT_SIZE);
                for _i in 0..max_iterations {
                    let amount = output_value
                        .checked_add(absolute_fee.as_nanowits())
                        .ok_or(TransactionError::FeeOverflow)?;

                    let collected_outputs = self.take_enough_utxos(
                        amount,
                        timestamp,
                        block_number_limit,
                        utxo_strategy,
                    )?;
                    let inputs = collected_outputs
                        .pointers
                        .iter()
                        .cloned()
                        .map(Input::new)
                        .collect();

                    let new_weight = match outputs.clone() {
                        TransactionOutputs::DataRequest((dr_output, change)) => {
                            let body = DRTransactionBody::new(
                                inputs,
                                dr_output,
                                change.into_iter().collect(),
                            );

                            body.weight()
                        }
                        TransactionOutputs::Stake((stake_output, change)) => {
                            let body = StakeTransactionBody::new(inputs, stake_output, change);

                            body.weight()
                        }
                        TransactionOutputs::ValueTransfer(outputs) => {
                            let body = VTTransactionBody::new(inputs, outputs);

                            body.weight()
                        }
                    };

                    if new_weight == current_weight {
                        return Ok(TransactionInfo {
                            fee: absolute_fee,
                            inputs: collected_outputs,
                            output_value,
                            outputs: outputs.into(),
                        });
                    } else {
                        current_weight = new_weight;
                    }
                }

                unreachable!("Unexpected exit in build_inputs_outputs method");
            }
        }
    }

    /// Generic inputs/outputs builder: can be used to build
    /// value transfer transactions and data request transactions.
    #[allow(clippy::too_many_arguments)]
    fn build_inputs_outputs(
        &mut self,
        outputs: Vec<ValueTransferOutput>,
        dr_output: Option<&DataRequestOutput>,
        fee: Fee,
        timestamp: u64,
        // The block number must be lower than this limit
        block_number_limit: Option<u32>,
        utxo_strategy: &UtxoSelectionStrategy,
        max_weight: u32,
    ) -> Result<TransactionInfo, TransactionError> {
        // On error just assume the value is u64::MAX, hoping that it is
        // impossible to pay for this transaction
        let output_value: u64 = transaction_outputs_sum(&outputs)
            .unwrap_or(u64::MAX)
            .checked_add(
                dr_output
                    .map(|o| o.checked_total_value().unwrap_or(u64::MAX))
                    .unwrap_or_default(),
            )
            .ok_or(TransactionError::OutputValueOverflow)?;

        // For the first estimation: 1 input and 1 output more for the change address
        let mut current_weight = calculate_weight(1, outputs.len() + 1, dr_output, max_weight)?;

        match fee {
            Fee::Absolute(absolute_fee) => {
                let amount = output_value
                    .checked_add(absolute_fee.as_nanowits())
                    .ok_or(TransactionError::FeeOverflow)?;

                let inputs =
                    self.take_enough_utxos(amount, timestamp, block_number_limit, utxo_strategy)?;

                Ok(TransactionInfo {
                    fee: absolute_fee,
                    inputs,
                    output_value,
                    outputs,
                })
            }
            Fee::Relative(priority) => {
                let max_iterations = 1 + ((max_weight - current_weight) / INPUT_SIZE);
                for _i in 0..max_iterations {
                    let absolute_fee = priority.into_absolute(current_weight);
                    let amount = output_value
                        .checked_add(absolute_fee.as_nanowits())
                        .ok_or(TransactionError::FeeOverflow)?;

                    let inputs = self.take_enough_utxos(
                        amount,
                        timestamp,
                        block_number_limit,
                        utxo_strategy,
                    )?;

                    let new_weight = calculate_weight(
                        inputs.pointers.len(),
                        outputs.len() + 1,
                        dr_output,
                        max_weight,
                    )?;
                    if new_weight == current_weight {
                        return Ok(TransactionInfo {
                            inputs,
                            outputs,
                            output_value,
                            fee: absolute_fee,
                        });
                    } else {
                        current_weight = new_weight;
                    }
                }

                unreachable!("Unexpected exit in build_inputs_outputs method");
            }
        }
    }
}

/// Calculate weight from inputs and outputs information
pub fn calculate_weight(
    inputs_count: usize,
    outputs_count: usize,
    dro: Option<&DataRequestOutput>,
    max_weight: u32,
) -> Result<u32, TransactionError> {
    let inputs = vec![Input::default(); inputs_count];
    let outputs = vec![ValueTransferOutput::default(); outputs_count];

    let weight = if let Some(dr_output) = dro {
        let drt = DRTransactionBody::new(inputs, dr_output.clone(), outputs);
        let dr_weight = drt.weight();
        if dr_weight > max_weight {
            return Err(TransactionError::DataRequestWeightLimitExceeded {
                weight: dr_weight,
                max_weight,
                dr_output: Box::new(dr_output.clone()),
            });
        } else {
            dr_weight
        }
    } else {
        let vtt = VTTransactionBody::new(inputs, outputs);
        let vt_weight = vtt.weight();
        if vt_weight > max_weight {
            return Err(TransactionError::ValueTransferWeightLimitExceeded {
                weight: vt_weight,
                max_weight,
            });
        } else {
            vt_weight
        }
    };

    Ok(weight)
}

/// Get total balance
pub fn get_total_balance(
    all_utxos: &UnspentOutputsPool,
    pkh: PublicKeyHash,
    simple: bool,
) -> NodeBalance {
    // FIXME: this does not scale, we need to be able to get UTXOs by PKH
    // Get the balance of the current utxo set
    let mut confirmed = 0;
    let mut total = 0;
    all_utxos.visit_with_pkh(
        pkh,
        |x| {
            let vto = &x.1 .0;
            confirmed += vto.value;
        },
        |x| {
            let vto = &x.1 .0;
            total += vto.value;
        },
    );

    if simple {
        NodeBalance::One {
            confirmed: None,
            total,
        }
    } else {
        NodeBalance::One {
            confirmed: Some(confirmed),
            total,
        }
    }
}

/// If the change_amount is greater than 0, insert a change output using the supplied `pkh`.
pub fn insert_change_output(
    outputs: &mut Vec<ValueTransferOutput>,
    own_pkh: PublicKeyHash,
    change_amount: u64,
) {
    if change_amount > 0 {
        // Create change output
        outputs.push(ValueTransferOutput {
            pkh: own_pkh,
            value: change_amount,
            time_lock: 0,
        });
    }
}

/// Build value transfer transaction with the given outputs and fee.
#[allow(clippy::too_many_arguments)]
pub fn build_vtt(
    outputs: Vec<ValueTransferOutput>,
    fee: Fee,
    own_utxos: &mut OwnUnspentOutputsPool,
    own_pkh: PublicKeyHash,
    all_utxos: &UnspentOutputsPool,
    timestamp: u64,
    tx_pending_timeout: u64,
    utxo_strategy: &UtxoSelectionStrategy,
    max_weight: u32,
    dry_run: bool,
) -> Result<VTTransactionBody, TransactionError> {
    let mut utxos = NodeUtxos {
        all_utxos,
        own_utxos,
        pkh: own_pkh,
    };

    let tx_info = utxos.build_inputs_outputs(
        outputs,
        None,
        fee,
        timestamp,
        None,
        utxo_strategy,
        max_weight,
    )?;

    let used_pointers = tx_info.inputs.pointers.iter().cloned().map(Input::new);

    // Mark UTXOs as used so we don't double spend
    // Save the timestamp after which the transaction will be considered timed out
    // and the output will become available for spending it again
    if !dry_run {
        utxos.set_used_output_pointer(used_pointers.clone(), timestamp + tx_pending_timeout);
    }

    let mut outputs = tx_info.outputs;
    insert_change_output(
        &mut outputs,
        own_pkh,
        tx_info.inputs.total_value - tx_info.output_value - tx_info.fee.as_nanowits(),
    );

    Ok(VTTransactionBody::new(
        used_pointers.collect::<Vec<_>>(),
        outputs,
    ))
}

/// Build data request transaction with the given outputs and fee.
#[allow(clippy::too_many_arguments)]
pub fn build_drt(
    dr_output: DataRequestOutput,
    fee: Fee,
    own_utxos: &mut OwnUnspentOutputsPool,
    own_pkh: PublicKeyHash,
    all_utxos: &UnspentOutputsPool,
    timestamp: u64,
    tx_pending_timeout: u64,
    max_weight: u32,
    dry_run: bool,
) -> Result<DRTransactionBody, TransactionError> {
    let mut utxos = NodeUtxos {
        all_utxos,
        own_utxos,
        pkh: own_pkh,
    };

    let tx_info = utxos.build_inputs_outputs(
        vec![],
        Some(&dr_output),
        fee,
        timestamp,
        None,
        &UtxoSelectionStrategy::Random { from: None },
        max_weight,
    )?;

    let used_pointers = tx_info.inputs.pointers.iter().cloned().map(Input::new);

    // Mark UTXOs as used so we don't double spend
    // Save the timestamp after which the transaction will be considered timed out
    // and the output will become available for spending it again
    if !dry_run {
        utxos.set_used_output_pointer(used_pointers.clone(), timestamp + tx_pending_timeout);
    }

    let mut outputs = tx_info.outputs;
    insert_change_output(
        &mut outputs,
        own_pkh,
        tx_info.inputs.total_value - tx_info.output_value - tx_info.fee.as_nanowits(),
    );

    Ok(DRTransactionBody::new(
        used_pointers.collect::<Vec<_>>(),
        dr_output,
        outputs,
    ))
}

/// Check if there are enough collateral for a CommitTransaction
pub fn check_commit_collateral(
    collateral: Wit,
    own_utxos: &OwnUnspentOutputsPool,
    own_pkh: PublicKeyHash,
    all_utxos: &UnspentOutputsPool,
    timestamp: u64,
    // The block number must be lower than this limit
    block_number_limit: u32,
) -> bool {
    let fee = Fee::absolute_from_wit(collateral);
    let mut utxos = NodeUtxosRef {
        all_utxos,
        own_utxos,
        pkh: own_pkh,
    };
    let can_collateralize = utxos
        .build_inputs_outputs(
            vec![],
            None,
            fee,
            timestamp,
            Some(block_number_limit),
            &UtxoSelectionStrategy::SmallFirst { from: None },
            u32::MAX,
        )
        .is_ok();
    let can_qualify = utxos
        .build_inputs_outputs(
            vec![],
            None,
            Fee::absolute_from_wit(Wit::from_wits(100)),
            timestamp,
            None,
            &UtxoSelectionStrategy::SmallFirst { from: None },
            u32::MAX,
        )
        .is_ok();

    can_collateralize && can_qualify
}

/// Build inputs and outputs to be used as the collateral in a CommitTransaction
pub fn build_commit_collateral(
    collateral: Wit,
    own_utxos: &mut OwnUnspentOutputsPool,
    own_pkh: PublicKeyHash,
    all_utxos: &UnspentOutputsPool,
    timestamp: u64,
    tx_pending_timeout: u64,
    // The block number must be lower than this limit
    block_number_limit: u32,
) -> Result<(Vec<Input>, Vec<ValueTransferOutput>), TransactionError> {
    // The fee is the difference between input value and output value
    // In a CommitTransaction, the collateral is also the difference between the input value
    // and the output value
    let fee = Fee::absolute_from_wit(collateral);
    let mut utxos = NodeUtxos {
        all_utxos,
        own_utxos,
        pkh: own_pkh,
    };
    let tx_info = utxos.build_inputs_outputs(
        vec![],
        None,
        fee,
        timestamp,
        Some(block_number_limit),
        &UtxoSelectionStrategy::SmallFirst { from: None },
        u32::MAX,
    )?;

    let used_pointers = tx_info.inputs.pointers.iter().cloned().map(Input::new);

    // Mark UTXOs as used so we don't double spend
    // Save the timestamp after which the transaction will be considered timed out
    // and the output will become available for spending it again
    utxos.set_used_output_pointer(used_pointers.clone(), timestamp + tx_pending_timeout);

    let mut outputs = tx_info.outputs;
    insert_change_output(
        &mut outputs,
        own_pkh,
        tx_info.inputs.total_value - tx_info.output_value - tx_info.fee.as_nanowits(),
    );

    Ok((used_pointers.collect::<Vec<_>>(), outputs))
}

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
        let vt_output = utxo_diff.get(input.output_pointer()).ok_or_else(|| {
            TransactionError::OutputNotFound {
                output: *input.output_pointer(),
            }
        })?;

        // Verify that commits are only accepted after the time lock expired
        let (epoch_timestamp, _) = epoch_constants.epoch_timestamp(epoch)?;
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
                    output: *input.output_pointer(),
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

/// Build stake transaction from existing UTXOs without a need to specify inputs or change.
#[allow(clippy::too_many_arguments)]
pub fn build_st(
    output: StakeOutput,
    fee: Fee,
    own_utxos: &mut OwnUnspentOutputsPool,
    own_pkh: PublicKeyHash,
    all_utxos: &UnspentOutputsPool,
    timestamp: u64,
    tx_pending_timeout: u64,
    utxo_strategy: &UtxoSelectionStrategy,
    max_weight: u32,
    dry_run: bool,
) -> Result<StakeTransactionBody, TransactionError> {
    let mut utxos = NodeUtxos {
        all_utxos,
        own_utxos,
        pkh: own_pkh,
    };

    let tx_info = utxos.generic_transaction_factory(
        TransactionOutputs::Stake((output.clone(), None)),
        fee,
        timestamp,
        None,
        utxo_strategy,
        max_weight,
    )?;

    let used_pointers = tx_info.inputs.pointers.iter().cloned().map(Input::new);

    // Mark UTXOs as used so we don't double spend
    // Save the timestamp after which the transaction will be considered timed out
    // and the output will become available for spending it again
    if !dry_run {
        utxos.set_used_output_pointer(used_pointers.clone(), timestamp + tx_pending_timeout);
    }

    // Only use a change output if there is value inserted by inputs that is not consumed by outputs
    // or fees
    let change_value = tx_info
        .inputs
        .total_value
        .wrapping_sub(tx_info.output_value)
        .wrapping_sub(tx_info.fee.as_nanowits());
    let change = if change_value > 0 {
        Some(ValueTransferOutput {
            pkh: own_pkh,
            value: change_value,
            time_lock: 0,
        })
    } else {
        None
    };

    let inputs = used_pointers.collect::<Vec<_>>();
    let body = StakeTransactionBody::new(inputs, output, change);

    Ok(body)
}

/// Build unstake transaction.
pub fn build_ut(
    withdrawal: ValueTransferOutput,
    operator: PublicKeyHash,
) -> Result<UnstakeTransactionBody, TransactionError> {
    let body = UnstakeTransactionBody::new(operator, withdrawal);

    Ok(body)
}

#[cfg(test)]
mod tests {
    use std::{
        convert::TryFrom,
        sync::{
            atomic::{AtomicU32, Ordering},
            Arc,
        },
    };

    use crate::{
        chain::{Hash, Hashable, PublicKey},
        error::TransactionError,
        transaction::*,
        utxo_pool::UtxoDbWrapStorage,
    };

    use super::*;

    const MAX_VT_WEIGHT: u32 = 20000;
    const MAX_DR_WEIGHT: u32 = 80000;

    // Counter used to prevent creating two transactions with the same hash
    static TX_COUNTER: AtomicU32 = AtomicU32::new(0);

    fn update_utxo_inputs(utxo: &mut UnspentOutputsPool, inputs: &[Input]) {
        for input in inputs {
            // Obtain the OutputPointer of each input and remove it from the utxo_set
            let output_pointer = input.output_pointer();

            // This does check for missing inputs, so ignore "fake inputs" with hash 000000...
            if output_pointer.transaction_id != Hash::default() {
                utxo.remove(output_pointer);
            }
        }
    }

    fn update_utxo_outputs(
        utxo: &mut UnspentOutputsPool,
        outputs: &[ValueTransferOutput],
        txn_hash: Hash,
        block_number: u32,
    ) {
        for (index, output) in outputs.iter().enumerate() {
            // Add the new outputs to the utxo_set
            let output_pointer = OutputPointer {
                transaction_id: txn_hash,
                output_index: u32::try_from(index).unwrap(),
            };

            utxo.insert(output_pointer, output.clone(), block_number);
        }
    }

    /// Method to update the unspent outputs pool
    pub fn generate_unspent_outputs_pool(
        unspent_outputs_pool: &UnspentOutputsPool,
        transactions: &[Transaction],
        block_number: u32,
    ) -> UnspentOutputsPool {
        // Create a copy of the state "unspent_outputs_pool"
        let mut unspent_outputs = unspent_outputs_pool.clone();

        for transaction in transactions {
            let txn_hash = transaction.hash();
            match transaction {
                Transaction::ValueTransfer(vt_transaction) => {
                    update_utxo_inputs(&mut unspent_outputs, &vt_transaction.body.inputs);
                    update_utxo_outputs(
                        &mut unspent_outputs,
                        &vt_transaction.body.outputs,
                        txn_hash,
                        block_number,
                    );
                }
                Transaction::DataRequest(dr_transaction) => {
                    update_utxo_inputs(&mut unspent_outputs, &dr_transaction.body.inputs);
                    update_utxo_outputs(
                        &mut unspent_outputs,
                        &dr_transaction.body.outputs,
                        txn_hash,
                        block_number,
                    );
                }
                Transaction::Tally(tally_transaction) => {
                    update_utxo_outputs(
                        &mut unspent_outputs,
                        &tally_transaction.outputs,
                        txn_hash,
                        block_number,
                    );
                }
                Transaction::Mint(mint_transaction) => {
                    update_utxo_outputs(
                        &mut unspent_outputs,
                        &mint_transaction.outputs,
                        txn_hash,
                        block_number,
                    );
                }
                _ => {}
            }
        }

        unspent_outputs
    }

    fn my_pkh() -> PublicKeyHash {
        PublicKeyHash::from_public_key(&PublicKey {
            compressed: 2,
            bytes: [0x01; 32],
        })
    }

    fn bob_pkh() -> PublicKeyHash {
        PublicKeyHash::from_public_key(&PublicKey {
            compressed: 2,
            bytes: [0x04; 32],
        })
    }

    fn build_vtt_tx(
        outputs: Vec<ValueTransferOutput>,
        fee: Fee,
        own_utxos: &mut OwnUnspentOutputsPool,
        own_pkh: PublicKeyHash,
        all_utxos: &UnspentOutputsPool,
    ) -> Result<Transaction, TransactionError> {
        let timestamp = 777;
        let tx_pending_timeout = 100;
        let vtt_tx = build_vtt(
            outputs,
            fee,
            own_utxos,
            own_pkh,
            all_utxos,
            timestamp,
            tx_pending_timeout,
            &UtxoSelectionStrategy::Random { from: None },
            MAX_VT_WEIGHT,
            false,
        )?;

        Ok(Transaction::ValueTransfer(VTTransaction::new(
            vtt_tx,
            vec![],
        )))
    }

    fn build_vtt_tx_with_timestamp(
        outputs: Vec<ValueTransferOutput>,
        fee: Fee,
        own_utxos: &mut OwnUnspentOutputsPool,
        own_pkh: PublicKeyHash,
        all_utxos: &UnspentOutputsPool,
        timestamp: u64,
    ) -> Result<Transaction, TransactionError> {
        let tx_pending_timeout = 100;
        let vtt_tx = build_vtt(
            outputs,
            fee,
            own_utxos,
            own_pkh,
            all_utxos,
            timestamp,
            tx_pending_timeout,
            &UtxoSelectionStrategy::Random { from: None },
            MAX_VT_WEIGHT,
            false,
        )?;

        Ok(Transaction::ValueTransfer(VTTransaction::new(
            vtt_tx,
            vec![],
        )))
    }

    fn build_drt_tx(
        dr_output: DataRequestOutput,
        fee: Fee,
        own_utxos: &mut OwnUnspentOutputsPool,
        own_pkh: PublicKeyHash,
        all_utxos: &UnspentOutputsPool,
    ) -> Result<Transaction, TransactionError> {
        let timestamp = 777;
        let tx_pending_timeout = 100;
        let drt_tx = build_drt(
            dr_output,
            fee,
            own_utxos,
            own_pkh,
            all_utxos,
            timestamp,
            tx_pending_timeout,
            MAX_DR_WEIGHT,
            false,
        )?;

        Ok(Transaction::DataRequest(DRTransaction::new(drt_tx, vec![])))
    }

    fn build_utxo_set<T: Into<Option<(OwnUnspentOutputsPool, UnspentOutputsPool)>>>(
        outputs: Vec<ValueTransferOutput>,
        own_utxos_all_utxos: T,
        txns: Vec<Transaction>,
    ) -> (OwnUnspentOutputsPool, UnspentOutputsPool) {
        build_utxo_set_with_block_number(outputs, own_utxos_all_utxos, txns, 0)
    }

    fn build_utxo_set_with_block_number<
        T: Into<Option<(OwnUnspentOutputsPool, UnspentOutputsPool)>>,
    >(
        outputs: Vec<ValueTransferOutput>,
        own_utxos_all_utxos: T,
        txns: Vec<Transaction>,
        block_number: u32,
    ) -> (OwnUnspentOutputsPool, UnspentOutputsPool) {
        let own_pkh = my_pkh();
        // Add a fake input to avoid hash collisions
        let fake_input = Input::new(OutputPointer {
            output_index: TX_COUNTER.fetch_add(1, Ordering::SeqCst),
            ..OutputPointer::default()
        });
        let mut txns = txns;
        txns.push(Transaction::ValueTransfer(VTTransaction::new(
            VTTransactionBody::new(vec![fake_input], outputs),
            vec![],
        )));

        let (mut own_utxos, all_utxos) = own_utxos_all_utxos.into().unwrap_or_else(|| {
            (
                OwnUnspentOutputsPool::default(),
                // Use utxo set with in-memory database, to allow testing confirmed/unconfirmed UTXOs
                {
                    UnspentOutputsPool {
                        db: Some(Arc::new(UtxoDbWrapStorage(
                            witnet_storage::backends::hashmap::Backend::default(),
                        ))),
                        ..Default::default()
                    }
                },
            )
        });
        let all_utxos = generate_unspent_outputs_pool(&all_utxos, &txns, block_number);
        update_own_utxos(&mut own_utxos, own_pkh, &txns);

        (own_utxos, all_utxos)
    }

    fn update_own_utxos(
        own_utxos: &mut OwnUnspentOutputsPool,
        own_pkh: PublicKeyHash,
        txns: &[Transaction],
    ) {
        for transaction in txns {
            match transaction {
                Transaction::ValueTransfer(vt_tx) => {
                    // Remove spent inputs
                    for input in &vt_tx.body.inputs {
                        own_utxos.remove(input.output_pointer());
                    }
                    // Insert new outputs
                    for (i, output) in vt_tx.body.outputs.iter().enumerate() {
                        if output.pkh == own_pkh {
                            own_utxos.insert(
                                OutputPointer {
                                    transaction_id: transaction.hash(),
                                    output_index: u32::try_from(i).unwrap(),
                                },
                                0,
                            );
                        }
                    }
                }

                Transaction::DataRequest(dr_tx) => {
                    // Remove spent inputs
                    for input in &dr_tx.body.inputs {
                        own_utxos.remove(input.output_pointer());
                    }
                    // Insert new outputs
                    for (i, output) in dr_tx.body.outputs.iter().enumerate() {
                        if output.pkh == own_pkh {
                            own_utxos.insert(
                                OutputPointer {
                                    transaction_id: transaction.hash(),
                                    output_index: u32::try_from(i).unwrap(),
                                },
                                0,
                            );
                        }
                    }
                }
                _ => {}
            }
        }
    }

    fn outputs_sum(transaction: &Transaction) -> u64 {
        match transaction {
            Transaction::ValueTransfer(vt_tx) => {
                transaction_outputs_sum(&vt_tx.body.outputs).unwrap()
            }
            Transaction::DataRequest(dr_tx) => transaction_outputs_sum(&dr_tx.body.outputs)
                .unwrap()
                .checked_add(dr_tx.body.dr_output.checked_total_value().unwrap())
                .unwrap(),
            _ => 0,
        }
    }

    fn outputs_sum_not_mine(transaction: &Transaction) -> u64 {
        let pkh = my_pkh();
        match transaction {
            Transaction::ValueTransfer(tx) => tx
                .body
                .outputs
                .iter()
                .map(|vt| if vt.pkh != pkh { vt.value } else { 0 })
                .sum(),
            Transaction::DataRequest(tx) => tx
                .body
                .outputs
                .iter()
                .map(|vt| if vt.pkh != pkh { vt.value } else { 0 })
                .sum(),
            _ => 0,
        }
    }

    // build_drt should only return one change output, with the same pkh as the first input, or
    // no outputs if the value is zero
    fn check_one_output(transaction: &Transaction, pkh: &PublicKeyHash, value: u64) {
        match transaction {
            Transaction::ValueTransfer(vt_tx) => {
                if value == 0 {
                    assert_eq!(vt_tx.body.outputs.len(), 0);
                } else {
                    assert_eq!(vt_tx.body.outputs.len(), 1);
                    assert_eq!(vt_tx.body.outputs[0].value, value);
                    assert_eq!(&vt_tx.body.outputs[0].pkh, pkh);
                }
            }
            Transaction::DataRequest(dr_tx) => {
                if value == 0 {
                    assert_eq!(dr_tx.body.outputs.len(), 0);
                } else {
                    assert_eq!(dr_tx.body.outputs.len(), 1);
                    assert_eq!(dr_tx.body.outputs[0].value, value);
                    assert_eq!(&dr_tx.body.outputs[0].pkh, pkh);
                }
            }
            t => panic!("Unexpected transaction type: {:?}", t),
        }
    }

    fn inputs_len(transaction: &Transaction) -> usize {
        match transaction {
            Transaction::ValueTransfer(tx) => tx.body.inputs.len(),
            Transaction::DataRequest(tx) => tx.body.inputs.len(),
            _ => 0,
        }
    }

    fn pay(pkh: PublicKeyHash, value: u64, time_lock: u64) -> ValueTransferOutput {
        ValueTransferOutput {
            pkh,
            value,
            time_lock,
        }
    }

    fn pay_me(value: u64) -> ValueTransferOutput {
        pay(my_pkh(), value, 0)
    }

    fn pay_alice(value: u64) -> ValueTransferOutput {
        let alice_pkh = PublicKeyHash::from_public_key(&PublicKey {
            compressed: 2,
            bytes: [0x03; 32],
        });

        pay(alice_pkh, value, 0)
    }

    fn pay_bob(value: u64) -> ValueTransferOutput {
        let bob_pkh = PublicKeyHash::from_public_key(&PublicKey {
            compressed: 2,
            bytes: [0x04; 32],
        });

        pay(bob_pkh, value, 0)
    }

    fn pay_me_later(value: u64, time_lock: u64) -> ValueTransferOutput {
        pay(my_pkh(), value, time_lock)
    }

    #[test]
    fn empty_utxo() {
        let own_pkh = my_pkh();
        let outputs = vec![];
        let (mut own_utxos, all_utxos) = build_utxo_set(outputs, None, vec![]);
        // Outputs was empty, so own_utxos is also empty
        assert!(own_utxos.is_empty(), "{:?}", own_utxos);

        // Building a zero value transaction returns an error
        assert_eq!(
            build_vtt_tx(vec![], Fee::default(), &mut own_utxos, own_pkh, &all_utxos).unwrap_err(),
            TransactionError::ZeroAmount
        );

        // Building any transaction with an empty own_utxos returns an error
        assert_eq!(
            build_vtt_tx(
                vec![pay_bob(1000)],
                Fee::default(),
                &mut own_utxos,
                own_pkh,
                &all_utxos
            )
            .unwrap_err(),
            TransactionError::NoMoney {
                total_balance: 0,
                available_balance: 0,
                transaction_value: 1000
            }
        );
        assert_eq!(
            build_vtt_tx(
                vec![],
                Fee::absolute_from_nanowits(50),
                &mut own_utxos,
                own_pkh,
                &all_utxos
            )
            .unwrap_err(),
            TransactionError::NoMoney {
                total_balance: 0,
                available_balance: 0,
                transaction_value: 50
            }
        );
        assert_eq!(
            build_vtt_tx(
                vec![pay_me(0), pay_bob(0)],
                Fee::default(),
                &mut own_utxos,
                own_pkh,
                &all_utxos
            )
            .unwrap_err(),
            TransactionError::ZeroAmount
        );
    }

    #[test]
    fn only_my_utxos() {
        let own_pkh = my_pkh();
        let outputs = vec![pay_alice(200), pay_bob(500), pay_bob(800)];
        let (own_utxos, all_utxos) = build_utxo_set(outputs, None, vec![]);
        // There were zero pay_me outputs
        assert!(own_utxos.is_empty(), "{:?}", own_utxos);

        let outputs = vec![pay_me(50), pay_me(100)];
        let (own_utxos, all_utxos) = build_utxo_set(outputs, (own_utxos, all_utxos), vec![]);
        // There are 2 pay_me in outputs, so there should be 2 outputs in own_utxos
        assert_eq!(own_utxos.len(), 2);

        let outputs = vec![pay_me(50), pay_me(100)];
        let (mut own_utxos, all_utxos) = build_utxo_set(outputs, (own_utxos, all_utxos), vec![]);
        // There are 2 pay_me in outputs, so there should be 2 new outputs in own_utxos
        assert_eq!(own_utxos.len(), 2 + 2);

        // The total value of own_utxos is 300, so trying to spend more than 300 will fail
        assert_eq!(
            build_vtt_tx(
                vec![],
                Fee::absolute_from_nanowits(301),
                &mut own_utxos,
                own_pkh,
                &all_utxos
            )
            .unwrap_err(),
            TransactionError::NoMoney {
                total_balance: 300,
                available_balance: 300,
                transaction_value: 301
            }
        );
    }

    #[test]
    fn poor_utxo() {
        let own_pkh = my_pkh();
        let outputs = vec![pay_me(1000)];
        let (mut own_utxos, all_utxos) = build_utxo_set(outputs, None, vec![]);
        // There is one pay_me in outputs, so there should be one output in own_utxos
        assert_eq!(own_utxos.len(), 1);

        assert_eq!(
            build_vtt_tx(
                vec![pay_bob(2000)],
                Fee::default(),
                &mut own_utxos,
                own_pkh,
                &all_utxos
            )
            .unwrap_err(),
            TransactionError::NoMoney {
                total_balance: 1000,
                available_balance: 1000,
                transaction_value: 2000
            }
        );
        let outputs = vec![pay_me(1000)];
        let (mut own_utxos, all_utxos) = build_utxo_set(outputs, None, vec![]);
        assert_eq!(
            build_vtt_tx(
                vec![],
                Fee::absolute_from_nanowits(1001),
                &mut own_utxos,
                own_pkh,
                &all_utxos
            )
            .unwrap_err(),
            TransactionError::NoMoney {
                total_balance: 1000,
                available_balance: 1000,
                transaction_value: 1001
            }
        );
        let outputs = vec![pay_me(1000)];
        let (mut own_utxos, all_utxos) = build_utxo_set(outputs, None, vec![]);
        assert_eq!(
            build_vtt_tx(
                vec![pay_bob(500)],
                Fee::absolute_from_nanowits(600),
                &mut own_utxos,
                own_pkh,
                &all_utxos
            )
            .unwrap_err(),
            TransactionError::NoMoney {
                total_balance: 1000,
                available_balance: 1000,
                transaction_value: 1100
            }
        );
    }

    #[test]
    fn time_locked_utxo() {
        let own_pkh = my_pkh();
        let outputs = vec![pay_me_later(1000, 1_000_000)];
        let (mut own_utxos, all_utxos) = build_utxo_set(outputs, None, vec![]);
        // There is one pay_me in outputs, so there should be one output in own_utxos
        assert_eq!(own_utxos.len(), 1);

        assert_eq!(
            build_vtt_tx_with_timestamp(
                vec![pay_bob(100)],
                Fee::default(),
                &mut own_utxos,
                own_pkh,
                &all_utxos,
                777
            )
            .unwrap_err(),
            TransactionError::NoMoney {
                total_balance: 1000,
                available_balance: 0,
                transaction_value: 100
            }
        );

        assert!(build_vtt_tx_with_timestamp(
            vec![pay_bob(100)],
            Fee::default(),
            &mut own_utxos,
            own_pkh,
            &all_utxos,
            1_000_001
        )
        .is_ok());
    }

    #[test]
    fn exact_change() {
        let own_pkh = my_pkh();
        let outputs = vec![pay_me(1000)];
        let (mut own_utxos1, all_utxos1) = build_utxo_set(outputs.clone(), None, vec![]);
        assert_eq!(own_utxos1.len(), 1);

        let t1 = build_vtt_tx(
            vec![pay_bob(1000)],
            Fee::default(),
            &mut own_utxos1,
            own_pkh,
            &all_utxos1,
        )
        .unwrap();
        assert_eq!(outputs_sum(&t1), 1000);

        let (mut own_utxos, all_utxos) = build_utxo_set(outputs.clone(), None, vec![]);
        let t2 = build_vtt_tx(
            vec![pay_bob(990)],
            Fee::absolute_from_nanowits(10),
            &mut own_utxos,
            own_pkh,
            &all_utxos,
        )
        .unwrap();
        assert_eq!(outputs_sum(&t2), 990);

        let (mut own_utxos, all_utxos) = build_utxo_set(outputs.clone(), None, vec![]);
        let t3 = build_vtt_tx(
            vec![],
            Fee::absolute_from_nanowits(1000),
            &mut own_utxos,
            own_pkh,
            &all_utxos,
        )
        .unwrap();
        assert_eq!(outputs_sum(&t3), 0);

        let (mut own_utxos, all_utxos) = build_utxo_set(outputs, None, vec![]);
        let t4 = build_vtt_tx(
            vec![pay_bob(500), pay_me(500)],
            Fee::default(),
            &mut own_utxos,
            own_pkh,
            &all_utxos,
        )
        .unwrap();
        assert_eq!(outputs_sum(&t4), 1000);
        assert_eq!(outputs_sum_not_mine(&t4), 500);

        // Execute transaction t1
        let (own_utxos, _all_utxos) = build_utxo_set(vec![], (own_utxos1, all_utxos1), vec![t1]);
        assert!(own_utxos.is_empty(), "{:?}", own_utxos);
    }

    #[test]
    fn one_big_utxo() {
        let own_pkh = my_pkh();
        let outputs = vec![pay_me(1_000_000)];
        let (mut own_utxos1, all_utxos1) = build_utxo_set(outputs.clone(), None, vec![]);
        assert_eq!(own_utxos1.len(), 1);

        let t1 = build_vtt_tx(
            vec![pay_bob(1000)],
            Fee::default(),
            &mut own_utxos1,
            own_pkh,
            &all_utxos1,
        )
        .unwrap();
        assert_eq!(outputs_sum(&t1), 1_000_000);
        assert_eq!(outputs_sum_not_mine(&t1), 1000);

        let (mut own_utxos, all_utxos) = build_utxo_set(outputs.clone(), None, vec![]);
        let t2 = build_vtt_tx(
            vec![pay_bob(990)],
            Fee::absolute_from_nanowits(10),
            &mut own_utxos,
            own_pkh,
            &all_utxos,
        )
        .unwrap();
        assert_eq!(outputs_sum(&t2), 999_990);
        assert_eq!(outputs_sum_not_mine(&t2), 990);

        let (mut own_utxos, all_utxos) = build_utxo_set(outputs.clone(), None, vec![]);
        let t3 = build_vtt_tx(
            vec![],
            Fee::absolute_from_nanowits(1000),
            &mut own_utxos,
            own_pkh,
            &all_utxos,
        )
        .unwrap();
        assert_eq!(outputs_sum(&t3), 999_000);
        assert_eq!(outputs_sum_not_mine(&t3), 0);

        let (mut own_utxos, all_utxos) = build_utxo_set(outputs, None, vec![]);
        let t4 = build_vtt_tx(
            vec![pay_bob(500), pay_me(500)],
            Fee::default(),
            &mut own_utxos,
            own_pkh,
            &all_utxos,
        )
        .unwrap();
        assert_eq!(outputs_sum_not_mine(&t4), 500);

        // Execute transaction t1
        // This should create a change output with value 1_000_000 - 1_000
        assert_eq!(own_utxos.len(), 1);
        let (mut own_utxos, all_utxos) = build_utxo_set(vec![], (own_utxos1, all_utxos1), vec![t1]);
        assert_eq!(own_utxos.len(), 1);
        assert_eq!(
            all_utxos
                .get(own_utxos.iter().next().unwrap().0)
                .unwrap()
                .value,
            1_000_000 - 1_000
        );
        assert_eq!(
            build_vtt_tx(
                vec![],
                Fee::absolute_from_nanowits(1_000_000 - 1_000 + 1),
                &mut own_utxos,
                own_pkh,
                &all_utxos
            )
            .unwrap_err(),
            TransactionError::NoMoney {
                total_balance: 1_000_000 - 1_000,
                available_balance: 1_000_000 - 1_000,
                transaction_value: 1_000_000 - 1_000 + 1
            },
        );

        // Now we can spend that new utxo
        let t5 = build_vtt_tx(
            vec![],
            Fee::absolute_from_nanowits(1_000_000 - 1_000),
            &mut own_utxos,
            own_pkh,
            &all_utxos,
        )
        .unwrap();
        // Execute transaction t5
        let (own_utxos, _all_utxos) = build_utxo_set(vec![], (own_utxos, all_utxos), vec![t5]);
        assert!(own_utxos.is_empty(), "{:?}", own_utxos);
    }

    #[test]
    fn many_small_utxos() {
        let own_pkh = my_pkh();
        // 1000 utxos with 1 value each
        let outputs = vec![pay_me(1); 1000];
        let (mut own_utxos, all_utxos) = build_utxo_set(outputs.clone(), None, vec![]);
        assert_eq!(own_utxos.len(), 1000);

        let t1 = build_vtt_tx(
            vec![pay_bob(1000)],
            Fee::default(),
            &mut own_utxos,
            own_pkh,
            &all_utxos,
        )
        .unwrap();
        assert_eq!(outputs_sum(&t1), 1000);
        assert_eq!(inputs_len(&t1), 1000);

        let (mut own_utxos, all_utxos) = build_utxo_set(outputs.clone(), None, vec![]);
        let t2 = build_vtt_tx(
            vec![pay_bob(990)],
            Fee::absolute_from_nanowits(10),
            &mut own_utxos,
            own_pkh,
            &all_utxos,
        )
        .unwrap();
        assert_eq!(outputs_sum(&t2), 990);
        assert_eq!(inputs_len(&t2), 1000);

        let (mut own_utxos, all_utxos) = build_utxo_set(outputs.clone(), None, vec![]);
        let t3 = build_vtt_tx(
            vec![],
            Fee::absolute_from_nanowits(1000),
            &mut own_utxos,
            own_pkh,
            &all_utxos,
        )
        .unwrap();
        assert_eq!(outputs_sum(&t3), 0);
        assert_eq!(inputs_len(&t3), 1000);

        let (mut own_utxos, all_utxos) = build_utxo_set(outputs, None, vec![]);
        let t4 = build_vtt_tx(
            vec![pay_bob(500)],
            Fee::absolute_from_nanowits(20),
            &mut own_utxos,
            own_pkh,
            &all_utxos,
        )
        .unwrap();
        assert_eq!(outputs_sum(&t4), 500);
        assert_eq!(inputs_len(&t4), 520);

        // Execute transaction t4
        // This will not create any change outputs because all our utxos have value 1
        let (mut own_utxos, all_utxos) = build_utxo_set(vec![], (own_utxos, all_utxos), vec![t4]);
        assert_eq!(own_utxos.len(), 480);

        assert_eq!(
            build_vtt_tx(
                vec![],
                Fee::absolute_from_nanowits(480 + 1),
                &mut own_utxos,
                own_pkh,
                &all_utxos
            )
            .unwrap_err(),
            TransactionError::NoMoney {
                total_balance: 480,
                available_balance: 480,
                transaction_value: 480 + 1
            },
        );
    }

    #[test]
    fn many_different_utxos() {
        let own_pkh = my_pkh();
        // Different outputs with total value: 1000
        let outputs = vec![
            pay_me(1),
            pay_me(5),
            pay_me(10),
            pay_me(50),
            pay_me(100),
            pay_me(500),
            pay_me(334),
        ];
        let (mut own_utxos, all_utxos) = build_utxo_set(outputs.clone(), None, vec![]);
        assert_eq!(own_utxos.len(), 7);

        let t1 = build_vtt_tx(
            vec![pay_bob(1000)],
            Fee::default(),
            &mut own_utxos,
            own_pkh,
            &all_utxos,
        )
        .unwrap();
        assert_eq!(outputs_sum_not_mine(&t1), 1000);

        let (mut own_utxos, all_utxos) = build_utxo_set(outputs.clone(), None, vec![]);
        let t2 = build_vtt_tx(
            vec![pay_bob(990)],
            Fee::absolute_from_nanowits(10),
            &mut own_utxos,
            own_pkh,
            &all_utxos,
        )
        .unwrap();
        assert_eq!(outputs_sum_not_mine(&t2), 990);

        let (mut own_utxos, all_utxos) = build_utxo_set(outputs.clone(), None, vec![]);
        let t3 = build_vtt_tx(
            vec![],
            Fee::absolute_from_nanowits(1000),
            &mut own_utxos,
            own_pkh,
            &all_utxos,
        )
        .unwrap();
        assert_eq!(outputs_sum_not_mine(&t3), 0);

        let (mut own_utxos, all_utxos) = build_utxo_set(outputs, None, vec![]);
        let t4 = build_vtt_tx(
            vec![pay_bob(500)],
            Fee::absolute_from_nanowits(20),
            &mut own_utxos,
            own_pkh,
            &all_utxos,
        )
        .unwrap();
        assert_eq!(outputs_sum_not_mine(&t4), 500);

        // Execute transaction t4
        let (mut own_utxos, all_utxos) = build_utxo_set(vec![], (own_utxos, all_utxos), vec![t4]);
        // This will create a change output with an unknown value, but the total available will be 1000 - 520
        assert_eq!(
            build_vtt_tx(
                vec![],
                Fee::absolute_from_nanowits(480 + 1),
                &mut own_utxos,
                own_pkh,
                &all_utxos
            )
            .unwrap_err(),
            TransactionError::NoMoney {
                total_balance: 480,
                available_balance: 480,
                transaction_value: 481
            }
        );

        // A transaction to ourselves with no fees will maintain our total balance
        let t5 = build_vtt_tx(
            vec![pay_me(480)],
            Fee::default(),
            &mut own_utxos,
            own_pkh,
            &all_utxos,
        )
        .unwrap();
        // Execute transaction t5
        let (mut own_utxos, all_utxos) = build_utxo_set(vec![], (own_utxos, all_utxos), vec![t5]);
        // Since we are spending everything, the result is merging all the unspent outputs into one
        assert_eq!(own_utxos.len(), 1);
        assert_eq!(
            all_utxos
                .get(own_utxos.iter().next().unwrap().0)
                .unwrap()
                .value,
            480
        );
        assert_eq!(
            build_vtt_tx(
                vec![],
                Fee::absolute_from_nanowits(480 + 1),
                &mut own_utxos,
                own_pkh,
                &all_utxos
            )
            .unwrap_err(),
            TransactionError::NoMoney {
                total_balance: 480,
                available_balance: 480,
                transaction_value: 480 + 1
            },
        );

        // Now spend everything
        let t6 = build_vtt_tx(
            vec![pay_bob(400)],
            Fee::absolute_from_nanowits(80),
            &mut own_utxos,
            own_pkh,
            &all_utxos,
        )
        .unwrap();
        // Execute transaction t6
        let (own_utxos, _all_utxos) = build_utxo_set(vec![], (own_utxos, all_utxos), vec![t6]);
        assert!(own_utxos.is_empty(), "{:?}", own_utxos);
    }

    #[test]
    fn test_get_total_balance() {
        let own_pkh = my_pkh();
        let bob_pkh = bob_pkh();
        // Different outputs with total value: 1000
        let outputs = vec![
            pay_me(1),
            pay_me(5),
            pay_me(10),
            pay_me(50),
            pay_me(100),
            pay_me(500),
            pay_me(334),
        ];
        let (mut own_utxos, mut all_utxos) = build_utxo_set(outputs, None, vec![]);
        // If the utxo set from the storage is None it should set the confirmed balance to 0
        assert_eq!(
            get_total_balance(&all_utxos, own_pkh, false),
            NodeBalance::One {
                confirmed: Some(0),
                total: 1000,
            }
        );
        // When using simple balance, both balances should be 1000
        assert_eq!(
            get_total_balance(&all_utxos, own_pkh, true),
            NodeBalance::One {
                confirmed: None,
                total: 1000,
            }
        );
        // Confirm pending UTXOs
        all_utxos.persist();
        // Assert the balance is 1000 when the superblock is confirmed
        assert_eq!(
            get_total_balance(&all_utxos, own_pkh, false),
            NodeBalance::One {
                confirmed: Some(1000),
                total: 1000,
            }
        );
        // Assert the balance is 1000 when the superblock is confirmed when using simple balance
        assert_eq!(
            get_total_balance(&all_utxos, own_pkh, true),
            NodeBalance::One {
                confirmed: None,
                total: 1000,
            }
        );

        let t2 = build_vtt_tx(
            vec![pay_bob(100)],
            Fee::default(),
            &mut own_utxos,
            own_pkh,
            &all_utxos,
        )
        .unwrap();
        let (own_utxos, mut all_utxos_2) = build_utxo_set(vec![], (own_utxos, all_utxos), vec![t2]);
        // Assert the balance is 900 after paying 100 to Bob
        assert_eq!(
            get_total_balance(&all_utxos_2, own_pkh, false),
            NodeBalance::One {
                confirmed: Some(1000),
                total: 900,
            }
        );
        // Assert both balances are 900 after paying 100 to Bob when using simple balance
        assert_eq!(
            get_total_balance(&all_utxos_2, own_pkh, true),
            NodeBalance::One {
                confirmed: None,
                total: 900,
            }
        );
        // Assert Bob's balance is 100
        assert_eq!(
            get_total_balance(&all_utxos_2, bob_pkh, false),
            NodeBalance::One {
                confirmed: Some(0),
                total: 100,
            }
        );
        // Assert both of Bob's balance are 100 when using simple balance
        assert_eq!(
            get_total_balance(&all_utxos_2, bob_pkh, true),
            NodeBalance::One {
                confirmed: None,
                total: 100,
            }
        );

        // Confirm pending UTXOs
        all_utxos_2.persist();
        let outputs3 = vec![pay_me(600)];
        let (mut _own_utxos, all_utxos_3) =
            build_utxo_set(outputs3, (own_utxos, all_utxos_2), vec![]);
        // Assert the balance is 1500 after receiving 600
        assert_eq!(
            get_total_balance(&all_utxos_3, own_pkh, false),
            NodeBalance::One {
                confirmed: Some(900),
                total: 1500,
            }
        );
        // Assert both balances are 1500 after receiving 600 when using simple balance
        assert_eq!(
            get_total_balance(&all_utxos_3, own_pkh, true),
            NodeBalance::One {
                confirmed: None,
                total: 1500,
            }
        );
    }

    #[test]
    fn exact_change_data_request() {
        let own_pkh = my_pkh();
        let outputs = vec![pay_me(3400)];
        let (mut own_utxos, all_utxos) = build_utxo_set(outputs.clone(), None, vec![]);
        assert_eq!(own_utxos.len(), 1);

        let t1 = build_drt_tx(
            DataRequestOutput {
                witness_reward: 3400 / 4,
                witnesses: 4,
                ..DataRequestOutput::default()
            },
            Fee::default(),
            &mut own_utxos,
            own_pkh,
            &all_utxos,
        )
        .unwrap();
        assert_eq!(outputs_sum(&t1), 3400);
        check_one_output(&t1, &own_pkh, 0);

        let (mut own_utxos, all_utxos) = build_utxo_set(outputs, None, vec![]);
        let t2 = build_drt_tx(
            DataRequestOutput {
                witness_reward: 1000 / 4,
                witnesses: 4,
                commit_and_reveal_fee: 300,
                ..DataRequestOutput::default()
            },
            Fee::default(),
            &mut own_utxos,
            own_pkh,
            &all_utxos,
        )
        .unwrap();
        assert_eq!(outputs_sum(&t2), 3400);
        check_one_output(&t2, &own_pkh, 0);

        // Execute transaction t2
        let (own_utxos, _all_utxos) = build_utxo_set(vec![], (own_utxos, all_utxos), vec![t2]);
        assert!(own_utxos.is_empty(), "{:?}", own_utxos);
    }

    #[test]
    fn one_big_utxo_data_request() {
        let own_pkh = my_pkh();
        let outputs = vec![pay_me(1_000_000)];
        let (mut own_utxos, all_utxos) = build_utxo_set(outputs.clone(), None, vec![]);
        assert_eq!(own_utxos.len(), 1);

        let t1 = build_drt_tx(
            DataRequestOutput {
                witness_reward: 1000 / 4,
                witnesses: 4,
                ..DataRequestOutput::default()
            },
            Fee::default(),
            &mut own_utxos,
            own_pkh,
            &all_utxos,
        )
        .unwrap();
        assert_eq!(outputs_sum(&t1), 1_000_000);
        check_one_output(&t1, &own_pkh, 1_000_000 - 1_000);

        let (mut own_utxos, all_utxos) = build_utxo_set(outputs, None, vec![]);
        let t2 = build_drt_tx(
            DataRequestOutput {
                witness_reward: 1000 / 4,
                witnesses: 4,
                commit_and_reveal_fee: 300,
                ..DataRequestOutput::default()
            },
            Fee::default(),
            &mut own_utxos,
            own_pkh,
            &all_utxos,
        )
        .unwrap();
        assert_eq!(outputs_sum(&t2), 1_000_000);
        check_one_output(&t2, &own_pkh, 1_000_000 - 3_400);

        // Execute transaction t2
        let (mut own_utxos, all_utxos) = build_utxo_set(vec![], (own_utxos, all_utxos), vec![t2]);
        // This will create a change output with value 1_000_000 - 3_900
        assert_eq!(own_utxos.len(), 1);
        assert_eq!(
            all_utxos
                .get(own_utxos.iter().next().unwrap().0)
                .unwrap()
                .value,
            1_000_000 - 3_400
        );
        assert_eq!(
            build_vtt_tx(
                vec![],
                Fee::absolute_from_nanowits(1_000_000 - 3_400 + 1),
                &mut own_utxos,
                own_pkh,
                &all_utxos
            )
            .unwrap_err(),
            TransactionError::NoMoney {
                total_balance: 1_000_000 - 3_400,
                available_balance: 1_000_000 - 3_400,
                transaction_value: 1_000_000 - 3_400 + 1
            }
        );
    }

    #[test]
    fn cannot_double_spend() {
        let own_pkh = my_pkh();
        let outputs = vec![pay_me(1_000_000)];
        let (mut own_utxos, all_utxos) = build_utxo_set(outputs, None, vec![]);
        assert_eq!(own_utxos.len(), 1);

        let t1 = build_drt_tx(
            DataRequestOutput {
                witness_reward: 1000 / 4,
                witnesses: 4,
                ..DataRequestOutput::default()
            },
            Fee::default(),
            &mut own_utxos,
            own_pkh,
            &all_utxos,
        )
        .unwrap();
        assert_eq!(outputs_sum(&t1), 1_000_000);
        check_one_output(&t1, &own_pkh, 1_000_000 - 1_000);

        // Creating another transaction will fail because the old one is not confirmed yet
        // and this account only has 1 UTXO
        let t2 = build_drt_tx(
            DataRequestOutput {
                witness_reward: 1000 / 4,
                witnesses: 4,
                commit_and_reveal_fee: 300,
                ..DataRequestOutput::default()
            },
            Fee::default(),
            &mut own_utxos,
            own_pkh,
            &all_utxos,
        )
        .unwrap_err();
        assert_eq!(
            t2,
            TransactionError::NoMoney {
                total_balance: 1_000_000,
                available_balance: 0,
                transaction_value: 3400
            }
        );
    }

    #[test]
    fn collateral_from_utxos_in_block_0() {
        let timestamp = 777;
        let tx_pending_timeout = 100;
        let own_pkh = my_pkh();
        let outputs = vec![pay_me(1000)];
        // This UTXOs are created in block number 0
        let (mut own_utxos, all_utxos) = build_utxo_set(outputs, None, vec![]);
        assert_eq!(own_utxos.len(), 1);

        let collateral = Wit::from_nanowits(1000);
        // A limit of block number 0 means that only UTXOs from block 0 can be valid
        let block_number_limit = 0;
        let (inputs, outputs) = build_commit_collateral(
            collateral,
            &mut own_utxos,
            own_pkh,
            &all_utxos,
            timestamp,
            tx_pending_timeout,
            block_number_limit,
        )
        .unwrap();
        assert_eq!(inputs.len(), 1);
        assert_eq!(outputs.len(), 0);
    }

    #[test]
    fn collateral_from_utxos_split_in_different_blocks() {
        let timestamp = 777;
        let tx_pending_timeout = 100;
        let own_pkh = my_pkh();
        let outputs = vec![pay_me(500)];
        // This UTXOs are created in block number 1
        let (own_utxos, all_utxos) = build_utxo_set_with_block_number(outputs, None, vec![], 1);
        assert_eq!(own_utxos.len(), 1);

        let outputs = vec![pay_me(400)];
        // This UTXOs are created in block number 2
        let (own_utxos, all_utxos) =
            build_utxo_set_with_block_number(outputs, (own_utxos, all_utxos), vec![], 2);
        assert_eq!(own_utxos.len(), 2);

        let outputs = vec![pay_me(200)];
        // This UTXOs are created in block number 3
        let (own_utxos, all_utxos) =
            build_utxo_set_with_block_number(outputs, (own_utxos, all_utxos), vec![], 3);
        assert_eq!(own_utxos.len(), 3);

        let outputs = vec![pay_me(1000)];
        // This UTXOs are created in block number 4
        let (mut own_utxos, all_utxos) =
            build_utxo_set_with_block_number(outputs, (own_utxos, all_utxos), vec![], 4);
        assert_eq!(own_utxos.len(), 4);

        let collateral = Wit::from_nanowits(1000);
        // A limit of block number 0 means that only UTXOs from block 0 can be valid
        let block_number_limit = 0;
        let t1 = build_commit_collateral(
            collateral,
            &mut own_utxos,
            own_pkh,
            &all_utxos,
            timestamp,
            tx_pending_timeout,
            block_number_limit,
        )
        .unwrap_err();
        assert_eq!(
            t1,
            TransactionError::NoMoney {
                total_balance: 2100,
                available_balance: 0,
                transaction_value: 1000,
            }
        );

        let collateral = Wit::from_nanowits(1000);
        // Only allow using UTXOs from block number <= 1
        let block_number_limit = 1;
        let t2 = build_commit_collateral(
            collateral,
            &mut own_utxos,
            own_pkh,
            &all_utxos,
            timestamp,
            tx_pending_timeout,
            block_number_limit,
        )
        .unwrap_err();
        assert_eq!(
            t2,
            TransactionError::NoMoney {
                total_balance: 2100,
                available_balance: 500,
                transaction_value: 1000,
            }
        );

        let collateral = Wit::from_nanowits(1000);
        let block_number_limit = 2;
        let t3 = build_commit_collateral(
            collateral,
            &mut own_utxos,
            own_pkh,
            &all_utxos,
            timestamp,
            tx_pending_timeout,
            block_number_limit,
        )
        .unwrap_err();
        assert_eq!(
            t3,
            TransactionError::NoMoney {
                total_balance: 2100,
                available_balance: 900,
                transaction_value: 1000,
            }
        );

        let collateral = Wit::from_nanowits(1000);
        let block_number_limit = 3;
        let (inputs, outputs) = build_commit_collateral(
            collateral,
            &mut own_utxos,
            own_pkh,
            &all_utxos,
            timestamp,
            tx_pending_timeout,
            block_number_limit,
        )
        .unwrap();
        assert_eq!(inputs.len(), 3);
        assert_eq!(outputs.len(), 1);
        assert_eq!(transaction_outputs_sum(&outputs).unwrap(), 100);
    }

    #[test]
    fn collateral_utxo_blocked_until_timeout() {
        let timestamp = 777;
        let tx_pending_timeout = 100;
        let own_pkh = my_pkh();
        let outputs = vec![pay_me(1000)];
        // This UTXOs are created in block number 0
        let (mut own_utxos, all_utxos) = build_utxo_set(outputs, None, vec![]);
        assert_eq!(own_utxos.len(), 1);

        let collateral = Wit::from_nanowits(1000);
        // A limit of block number 0 means that only UTXOs from block 0 can be valid
        let block_number_limit = 0;
        let (inputs, outputs) = build_commit_collateral(
            collateral,
            &mut own_utxos,
            own_pkh,
            &all_utxos,
            timestamp,
            tx_pending_timeout,
            block_number_limit,
        )
        .unwrap();
        assert_eq!(inputs.len(), 1);
        assert_eq!(outputs.len(), 0);

        let timestamp = 777 + tx_pending_timeout - 1;
        let res = build_commit_collateral(
            collateral,
            &mut own_utxos,
            own_pkh,
            &all_utxos,
            timestamp,
            tx_pending_timeout,
            block_number_limit,
        )
        .unwrap_err();
        assert_eq!(
            res,
            TransactionError::NoMoney {
                total_balance: 1000,
                available_balance: 0,
                transaction_value: 1000,
            }
        );

        let timestamp = 777 + tx_pending_timeout;
        let (inputs, outputs) = build_commit_collateral(
            collateral,
            &mut own_utxos,
            own_pkh,
            &all_utxos,
            timestamp,
            tx_pending_timeout,
            block_number_limit,
        )
        .unwrap();
        assert_eq!(inputs.len(), 1);
        assert_eq!(outputs.len(), 0);
    }
}
