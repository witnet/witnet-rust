use crate::signature_mngr;
use futures::Future;
use std::collections::HashMap;
use witnet_data_structures::{
    chain::{
        DataRequestOutput, Hashable, Input, KeyedSignature, OutputPointer, PublicKeyHash,
        UnspentOutputsPool, ValueTransferOutput,
    },
    error::TransactionError,
    transaction::{DRTransactionBody, MemoizedHashable, VTTransactionBody},
};
use witnet_validations::validations::transaction_outputs_sum;

/// Select enough UTXOs to sum up to `amount`.
///
/// On success, return a list of output pointers and their sum.
/// On error, return the total sum of the output pointers in `own_utxos`.
pub fn take_enough_utxos<S: std::hash::BuildHasher>(
    own_utxos: &mut HashMap<OutputPointer, u64, S>,
    all_utxos: &UnspentOutputsPool,
    amount: u64,
    timestamp: u64,
    tx_pending_timeout: u64,
) -> Result<(Vec<OutputPointer>, u64), failure::Error> {
    // FIXME: this is a very naive utxo selection algorithm
    if amount == 0 {
        // Transactions with no inputs make no sense
        return Err(TransactionError::ZeroAmount.into());
    }

    let mut acc = 0;
    let mut total = 0;
    let mut list = vec![];

    for (op, ts) in own_utxos.iter_mut() {
        let value = all_utxos.get(op).unwrap().value;
        total += value;
        if all_utxos.get(op).unwrap().time_lock > timestamp {
            continue;
        }

        if timestamp < *ts + tx_pending_timeout {
            continue;
        }

        acc += value;
        list.push(op.clone());

        if acc >= amount {
            break;
        }
    }

    if acc >= amount {
        // Mark UTXOs as used so we don't double spend
        for elem in &list {
            let ts = own_utxos.get_mut(elem).unwrap();
            *ts = timestamp;
        }
        Ok((list, acc))
    } else {
        Err(TransactionError::NoMoney {
            total_balance: total,
            available_balance: acc,
            transaction_value: amount,
        }
        .into())
    }
}

/// Get total balance
pub fn get_total_balance(all_utxos: &UnspentOutputsPool, pkh: PublicKeyHash) -> u64 {
    // FIXME: this does not scale, we need to be able to get UTXOs by PKH
    all_utxos
        .iter()
        .filter_map(|(_output_pointer, vto)| {
            if vto.pkh == pkh {
                Some(vto.value)
            } else {
                None
            }
        })
        .sum()
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
pub fn build_vtt<S: std::hash::BuildHasher>(
    outputs: Vec<ValueTransferOutput>,
    fee: u64,
    own_utxos: &mut HashMap<OutputPointer, u64, S>,
    own_pkh: PublicKeyHash,
    all_utxos: &UnspentOutputsPool,
    timestamp: u64,
    tx_pending_timeout: u64,
) -> Result<VTTransactionBody, failure::Error> {
    let (inputs, outputs) = build_inputs_outputs_inner(
        outputs,
        None,
        fee,
        own_utxos,
        own_pkh,
        all_utxos,
        timestamp,
        tx_pending_timeout,
    )?;

    Ok(VTTransactionBody::new(inputs, outputs))
}

/// Build data request transaction with the given outputs and fee.
pub fn build_drt<S: std::hash::BuildHasher>(
    dr_output: DataRequestOutput,
    fee: u64,
    own_utxos: &mut HashMap<OutputPointer, u64, S>,
    own_pkh: PublicKeyHash,
    all_utxos: &UnspentOutputsPool,
    timestamp: u64,
    tx_pending_timeout: u64,
) -> Result<DRTransactionBody, failure::Error> {
    let (inputs, outputs) = build_inputs_outputs_inner(
        vec![],
        Some(&dr_output),
        fee,
        own_utxos,
        own_pkh,
        all_utxos,
        timestamp,
        tx_pending_timeout,
    )?;

    Ok(DRTransactionBody::new(inputs, outputs, dr_output))
}

/// Generic inputs/outputs builder: can be used to build
/// value transfer transactions and data request transactions.
#[allow(clippy::too_many_arguments)]
fn build_inputs_outputs_inner<S: std::hash::BuildHasher>(
    outputs: Vec<ValueTransferOutput>,
    dr_output: Option<&DataRequestOutput>,
    fee: u64,
    own_utxos: &mut HashMap<OutputPointer, u64, S>,
    own_pkh: PublicKeyHash,
    all_utxos: &UnspentOutputsPool,
    timestamp: u64,
    tx_pending_timeout: u64,
) -> Result<(Vec<Input>, Vec<ValueTransferOutput>), failure::Error> {
    // On error just assume the value is u64::max_value(), hoping that it is
    // impossible to pay for this transaction
    let output_value: u64 = transaction_outputs_sum(&outputs)
        .unwrap_or(u64::max_value())
        .saturating_add(
            dr_output
                .map(|o| o.checked_total_value().unwrap_or(u64::max_value()))
                .unwrap_or_default(),
        );

    let (output_pointers, input_value) = take_enough_utxos(
        own_utxos,
        all_utxos,
        output_value + fee,
        timestamp,
        tx_pending_timeout,
    )?;
    let inputs: Vec<Input> = output_pointers.into_iter().map(Input::new).collect();
    let mut outputs = outputs;
    insert_change_output(&mut outputs, own_pkh, input_value - output_value - fee);

    Ok((inputs, outputs))
}

/// Sign a transaction using this node's private key.
/// This function assumes that all the inputs have the same public key hash:
/// the hash of the public key of the node.
pub fn sign_transaction<T>(
    tx: &T,
    inputs_len: usize,
) -> impl Future<Item = Vec<KeyedSignature>, Error = failure::Error>
where
    T: MemoizedHashable + Hashable,
{
    // Assuming that all the inputs have the same pkh
    signature_mngr::sign(tx).map(move |signature| {
        // TODO: do we need to sign:
        // value transfer inputs,
        // data request inputs (for commits),
        // commit inputs (for reveals),
        //
        // We do not need to sign:
        // reveal inputs (for tallies)
        //
        // But currently we just sign everything, hoping that the validations
        // work
        vec![signature; inputs_len]
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        convert::TryFrom,
        sync::atomic::{AtomicU32, Ordering},
    };
    use witnet_data_structures::{
        chain::{generate_unspent_outputs_pool, Hashable, PublicKey},
        error::TransactionError,
        transaction::*,
    };

    // Counter used to prevent creating two transactions with the same hash
    static TX_COUNTER: AtomicU32 = AtomicU32::new(0);

    fn my_pkh() -> PublicKeyHash {
        PublicKeyHash::from_public_key(&PublicKey {
            compressed: 2,
            bytes: [0x01; 32],
        })
    }

    fn build_vtt_tx<S: std::hash::BuildHasher>(
        outputs: Vec<ValueTransferOutput>,
        fee: u64,
        own_utxos: &mut HashMap<OutputPointer, u64, S>,
        own_pkh: PublicKeyHash,
        all_utxos: &UnspentOutputsPool,
    ) -> Result<Transaction, failure::Error> {
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
        )?;

        Ok(Transaction::ValueTransfer(VTTransaction::new(
            vtt_tx,
            vec![],
        )))
    }

    fn build_vtt_tx_with_timestamp<S: std::hash::BuildHasher>(
        outputs: Vec<ValueTransferOutput>,
        fee: u64,
        own_utxos: &mut HashMap<OutputPointer, u64, S>,
        own_pkh: PublicKeyHash,
        all_utxos: &UnspentOutputsPool,
        timestamp: u64,
    ) -> Result<Transaction, failure::Error> {
        let tx_pending_timeout = 100;
        let vtt_tx = build_vtt(
            outputs,
            fee,
            own_utxos,
            own_pkh,
            all_utxos,
            timestamp,
            tx_pending_timeout,
        )?;

        Ok(Transaction::ValueTransfer(VTTransaction::new(
            vtt_tx,
            vec![],
        )))
    }

    fn build_drt_tx<S: std::hash::BuildHasher>(
        dr_output: DataRequestOutput,
        fee: u64,
        own_utxos: &mut HashMap<OutputPointer, u64, S>,
        own_pkh: PublicKeyHash,
        all_utxos: &UnspentOutputsPool,
    ) -> Result<Transaction, failure::Error> {
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
        )?;

        Ok(Transaction::DataRequest(DRTransaction::new(drt_tx, vec![])))
    }

    fn build_utxo_set<T: Into<Option<(HashMap<OutputPointer, u64>, UnspentOutputsPool)>>>(
        outputs: Vec<ValueTransferOutput>,
        own_utxos_all_utxos: T,
        txns: Vec<Transaction>,
    ) -> (HashMap<OutputPointer, u64>, UnspentOutputsPool) {
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
        let block_epoch = 0;

        let (mut own_utxos, all_utxos) = own_utxos_all_utxos.into().unwrap_or_default();
        let all_utxos = generate_unspent_outputs_pool(&all_utxos, &txns, block_epoch);
        update_own_utxos(&mut own_utxos, own_pkh, &txns);

        (own_utxos, all_utxos)
    }

    fn update_own_utxos<S: std::hash::BuildHasher>(
        own_utxos: &mut HashMap<OutputPointer, u64, S>,
        own_pkh: PublicKeyHash,
        txns: &[Transaction],
    ) {
        for transaction in txns {
            match transaction {
                Transaction::ValueTransfer(vt_tx) => {
                    // Remove spent inputs
                    for input in &vt_tx.body.inputs {
                        own_utxos.remove(&input.output_pointer());
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
                        own_utxos.remove(&input.output_pointer());
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
            build_vtt_tx(vec![], 0, &mut own_utxos, own_pkh, &all_utxos)
                .unwrap_err()
                .downcast::<TransactionError>()
                .unwrap(),
            TransactionError::ZeroAmount
        );

        // Building any transaction with an empty own_utxos returns an error
        assert_eq!(
            build_vtt_tx(vec![pay_bob(1000)], 0, &mut own_utxos, own_pkh, &all_utxos)
                .unwrap_err()
                .downcast::<TransactionError>()
                .unwrap(),
            TransactionError::NoMoney {
                total_balance: 0,
                available_balance: 0,
                transaction_value: 1000
            }
        );
        assert_eq!(
            build_vtt_tx(vec![], 50, &mut own_utxos, own_pkh, &all_utxos)
                .unwrap_err()
                .downcast::<TransactionError>()
                .unwrap(),
            TransactionError::NoMoney {
                total_balance: 0,
                available_balance: 0,
                transaction_value: 50
            }
        );
        assert_eq!(
            build_vtt_tx(
                vec![pay_me(0), pay_bob(0)],
                0,
                &mut own_utxos,
                own_pkh,
                &all_utxos
            )
            .unwrap_err()
            .downcast::<TransactionError>()
            .unwrap(),
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
            build_vtt_tx(vec![], 301, &mut own_utxos, own_pkh, &all_utxos)
                .unwrap_err()
                .downcast::<TransactionError>()
                .unwrap(),
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
            build_vtt_tx(vec![pay_bob(2000)], 0, &mut own_utxos, own_pkh, &all_utxos)
                .unwrap_err()
                .downcast::<TransactionError>()
                .unwrap(),
            TransactionError::NoMoney {
                total_balance: 1000,
                available_balance: 1000,
                transaction_value: 2000
            }
        );
        let outputs = vec![pay_me(1000)];
        let (mut own_utxos, all_utxos) = build_utxo_set(outputs, None, vec![]);
        assert_eq!(
            build_vtt_tx(vec![], 1001, &mut own_utxos, own_pkh, &all_utxos)
                .unwrap_err()
                .downcast::<TransactionError>()
                .unwrap(),
            TransactionError::NoMoney {
                total_balance: 1000,
                available_balance: 1000,
                transaction_value: 1001
            }
        );
        let outputs = vec![pay_me(1000)];
        let (mut own_utxos, all_utxos) = build_utxo_set(outputs, None, vec![]);
        assert_eq!(
            build_vtt_tx(vec![pay_bob(500)], 600, &mut own_utxos, own_pkh, &all_utxos)
                .unwrap_err()
                .downcast::<TransactionError>()
                .unwrap(),
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
                0,
                &mut own_utxos,
                own_pkh,
                &all_utxos,
                777
            )
            .unwrap_err()
            .downcast::<TransactionError>()
            .unwrap(),
            TransactionError::NoMoney {
                total_balance: 1000,
                available_balance: 0,
                transaction_value: 100
            }
        );

        assert_eq!(
            build_vtt_tx_with_timestamp(
                vec![pay_bob(100)],
                0,
                &mut own_utxos,
                own_pkh,
                &all_utxos,
                1_000_001
            )
            .is_ok(),
            true
        );
    }

    #[test]
    fn exact_change() {
        let own_pkh = my_pkh();
        let outputs = vec![pay_me(1000)];
        let (mut own_utxos1, all_utxos1) = build_utxo_set(outputs.clone(), None, vec![]);
        assert_eq!(own_utxos1.len(), 1);

        let t1 = build_vtt_tx(
            vec![pay_bob(1000)],
            0,
            &mut own_utxos1,
            own_pkh,
            &all_utxos1,
        )
        .unwrap();
        assert_eq!(outputs_sum(&t1), 1000);

        let (mut own_utxos, all_utxos) = build_utxo_set(outputs.clone(), None, vec![]);
        let t2 = build_vtt_tx(vec![pay_bob(990)], 10, &mut own_utxos, own_pkh, &all_utxos).unwrap();
        assert_eq!(outputs_sum(&t2), 990);

        let (mut own_utxos, all_utxos) = build_utxo_set(outputs.clone(), None, vec![]);
        let t3 = build_vtt_tx(vec![], 1000, &mut own_utxos, own_pkh, &all_utxos).unwrap();
        assert_eq!(outputs_sum(&t3), 0);

        let (mut own_utxos, all_utxos) = build_utxo_set(outputs, None, vec![]);
        let t4 = build_vtt_tx(
            vec![pay_bob(500), pay_me(500)],
            0,
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
            0,
            &mut own_utxos1,
            own_pkh,
            &all_utxos1,
        )
        .unwrap();
        assert_eq!(outputs_sum(&t1), 1_000_000);
        assert_eq!(outputs_sum_not_mine(&t1), 1000);

        let (mut own_utxos, all_utxos) = build_utxo_set(outputs.clone(), None, vec![]);
        let t2 = build_vtt_tx(vec![pay_bob(990)], 10, &mut own_utxos, own_pkh, &all_utxos).unwrap();
        assert_eq!(outputs_sum(&t2), 999_990);
        assert_eq!(outputs_sum_not_mine(&t2), 990);

        let (mut own_utxos, all_utxos) = build_utxo_set(outputs.clone(), None, vec![]);
        let t3 = build_vtt_tx(vec![], 1000, &mut own_utxos, own_pkh, &all_utxos).unwrap();
        assert_eq!(outputs_sum(&t3), 999_000);
        assert_eq!(outputs_sum_not_mine(&t3), 0);

        let (mut own_utxos, all_utxos) = build_utxo_set(outputs, None, vec![]);
        let t4 = build_vtt_tx(
            vec![pay_bob(500), pay_me(500)],
            0,
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
                (1_000_000 - 1_000) + 1,
                &mut own_utxos,
                own_pkh,
                &all_utxos
            )
            .unwrap_err()
            .downcast::<TransactionError>()
            .unwrap(),
            TransactionError::NoMoney {
                total_balance: 1_000_000 - 1_000,
                available_balance: 1_000_000 - 1_000,
                transaction_value: 1_000_000 - 1_000 + 1
            },
        );

        // Now we can spend that new utxo
        let t5 = build_vtt_tx(
            vec![],
            1_000_000 - 1_000,
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

        let t1 = build_vtt_tx(vec![pay_bob(1000)], 0, &mut own_utxos, own_pkh, &all_utxos).unwrap();
        assert_eq!(outputs_sum(&t1), 1000);
        assert_eq!(inputs_len(&t1), 1000);

        let (mut own_utxos, all_utxos) = build_utxo_set(outputs.clone(), None, vec![]);
        let t2 = build_vtt_tx(vec![pay_bob(990)], 10, &mut own_utxos, own_pkh, &all_utxos).unwrap();
        assert_eq!(outputs_sum(&t2), 990);
        assert_eq!(inputs_len(&t2), 1000);

        let (mut own_utxos, all_utxos) = build_utxo_set(outputs.clone(), None, vec![]);
        let t3 = build_vtt_tx(vec![], 1000, &mut own_utxos, own_pkh, &all_utxos).unwrap();
        assert_eq!(outputs_sum(&t3), 0);
        assert_eq!(inputs_len(&t3), 1000);

        let (mut own_utxos, all_utxos) = build_utxo_set(outputs, None, vec![]);
        let t4 = build_vtt_tx(vec![pay_bob(500)], 20, &mut own_utxos, own_pkh, &all_utxos).unwrap();
        assert_eq!(outputs_sum(&t4), 500);
        assert_eq!(inputs_len(&t4), 520);

        // Execute transaction t4
        // This will not create any change outputs because all our utxos have value 1
        let (mut own_utxos, all_utxos) = build_utxo_set(vec![], (own_utxos, all_utxos), vec![t4]);
        assert_eq!(own_utxos.len(), 480);

        assert_eq!(
            build_vtt_tx(vec![], 480 + 1, &mut own_utxos, own_pkh, &all_utxos)
                .unwrap_err()
                .downcast::<TransactionError>()
                .unwrap(),
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

        let t1 = build_vtt_tx(vec![pay_bob(1000)], 0, &mut own_utxos, own_pkh, &all_utxos).unwrap();
        assert_eq!(outputs_sum_not_mine(&t1), 1000);

        let (mut own_utxos, all_utxos) = build_utxo_set(outputs.clone(), None, vec![]);
        let t2 = build_vtt_tx(vec![pay_bob(990)], 10, &mut own_utxos, own_pkh, &all_utxos).unwrap();
        assert_eq!(outputs_sum_not_mine(&t2), 990);

        let (mut own_utxos, all_utxos) = build_utxo_set(outputs.clone(), None, vec![]);
        let t3 = build_vtt_tx(vec![], 1000, &mut own_utxos, own_pkh, &all_utxos).unwrap();
        assert_eq!(outputs_sum_not_mine(&t3), 0);

        let (mut own_utxos, all_utxos) = build_utxo_set(outputs, None, vec![]);
        let t4 = build_vtt_tx(vec![pay_bob(500)], 20, &mut own_utxos, own_pkh, &all_utxos).unwrap();
        assert_eq!(outputs_sum_not_mine(&t4), 500);

        // Execute transaction t4
        let (mut own_utxos, all_utxos) = build_utxo_set(vec![], (own_utxos, all_utxos), vec![t4]);
        // This will create a change output with an unknown value, but the total available will be 1000 - 520
        assert_eq!(
            build_vtt_tx(vec![], 480 + 1, &mut own_utxos, own_pkh, &all_utxos)
                .unwrap_err()
                .downcast::<TransactionError>()
                .unwrap(),
            TransactionError::NoMoney {
                total_balance: 480,
                available_balance: 480,
                transaction_value: 481
            }
        );

        // A transaction to ourselves with no fees will maintain our total balance
        let t5 = build_vtt_tx(vec![pay_me(480)], 0, &mut own_utxos, own_pkh, &all_utxos).unwrap();
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
            build_vtt_tx(vec![], 480 + 1, &mut own_utxos, own_pkh, &all_utxos)
                .unwrap_err()
                .downcast::<TransactionError>()
                .unwrap(),
            TransactionError::NoMoney {
                total_balance: 480,
                available_balance: 480,
                transaction_value: 480 + 1
            },
        );

        // Now spend everything
        let t6 = build_vtt_tx(vec![pay_bob(400)], 80, &mut own_utxos, own_pkh, &all_utxos).unwrap();
        // Execute transaction t6
        let (own_utxos, _all_utxos) = build_utxo_set(vec![], (own_utxos, all_utxos), vec![t6]);
        assert!(own_utxos.is_empty(), "{:?}", own_utxos);
    }

    #[test]
    fn exact_change_data_request() {
        let own_pkh = my_pkh();
        let outputs = vec![pay_me(3900)];
        let (mut own_utxos, all_utxos) = build_utxo_set(outputs.clone(), None, vec![]);
        assert_eq!(own_utxos.len(), 1);

        let t1 = build_drt_tx(
            DataRequestOutput {
                witness_reward: 3900 / 4,
                witnesses: 4,
                ..DataRequestOutput::default()
            },
            0,
            &mut own_utxos,
            own_pkh,
            &all_utxos,
        )
        .unwrap();
        assert_eq!(outputs_sum(&t1), 3900);

        let (mut own_utxos, all_utxos) = build_utxo_set(outputs, None, vec![]);
        let t2 = build_drt_tx(
            DataRequestOutput {
                witness_reward: 1000 / 4,
                witnesses: 4,
                commit_fee: 300,
                reveal_fee: 400,
                tally_fee: 100,
                ..DataRequestOutput::default()
            },
            0,
            &mut own_utxos,
            own_pkh,
            &all_utxos,
        )
        .unwrap();
        assert_eq!(outputs_sum(&t2), 3900);

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
            0,
            &mut own_utxos,
            own_pkh,
            &all_utxos,
        )
        .unwrap();
        assert_eq!(outputs_sum(&t1), 1_000_000);

        let (mut own_utxos, all_utxos) = build_utxo_set(outputs, None, vec![]);
        let t2 = build_drt_tx(
            DataRequestOutput {
                witness_reward: 1000 / 4,
                witnesses: 4,
                commit_fee: 300,
                reveal_fee: 400,
                tally_fee: 100,
                ..DataRequestOutput::default()
            },
            0,
            &mut own_utxos,
            own_pkh,
            &all_utxos,
        )
        .unwrap();
        assert_eq!(outputs_sum(&t2), 1_000_000);

        // Execute transaction t2
        let (mut own_utxos, all_utxos) = build_utxo_set(vec![], (own_utxos, all_utxos), vec![t2]);
        // This will create a change output with value 1_000_000 - 3_900
        assert_eq!(own_utxos.len(), 1);
        assert_eq!(
            all_utxos
                .get(own_utxos.iter().next().unwrap().0)
                .unwrap()
                .value,
            1_000_000 - 3_900
        );
        assert_eq!(
            build_vtt_tx(
                vec![],
                1_000_000 - 3_900 + 1,
                &mut own_utxos,
                own_pkh,
                &all_utxos
            )
            .unwrap_err()
            .downcast::<TransactionError>()
            .unwrap(),
            TransactionError::NoMoney {
                total_balance: 1_000_000 - 3_900,
                available_balance: 1_000_000 - 3_900,
                transaction_value: 1_000_000 - 3_900 + 1
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
            0,
            &mut own_utxos,
            own_pkh,
            &all_utxos,
        )
        .unwrap();
        assert_eq!(outputs_sum(&t1), 1_000_000);

        // Creating another transaction will fail because the old one is not confirmed yet
        // and this account only has 1 UTXO
        let t2 = build_drt_tx(
            DataRequestOutput {
                witness_reward: 1000 / 4,
                witnesses: 4,
                commit_fee: 300,
                reveal_fee: 400,
                tally_fee: 100,
                ..DataRequestOutput::default()
            },
            0,
            &mut own_utxos,
            own_pkh,
            &all_utxos,
        )
        .unwrap_err()
        .downcast::<TransactionError>()
        .unwrap();
        assert_eq!(
            t2,
            TransactionError::NoMoney {
                total_balance: 1_000_000,
                available_balance: 0,
                transaction_value: 3900
            }
        );
    }
}
