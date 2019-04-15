use crate::signature_mngr;
use futures::Future;
use std::collections::HashSet;
use witnet_data_structures::chain::{
    DataRequestOutput, Input, Output, OutputPointer, PublicKeyHash, Transaction, TransactionBody,
    UnspentOutputsPool, ValueTransferInput, ValueTransferOutput,
};

/// Select enough UTXOs to sum up to `amount`.
///
/// On success, return a list of output pointers and their sum.
/// On error, return the total sum of the output pointers in `own_utxos`.
pub fn take_enough_utxos<S: std::hash::BuildHasher>(
    own_utxos: &HashSet<OutputPointer, S>,
    all_utxos: &UnspentOutputsPool,
    amount: u64,
) -> Result<(Vec<OutputPointer>, u64), u64> {
    // FIXME: this is a very naive utxo selection algorithm
    if amount == 0 {
        // Transactions with no inputs make no sense
        return Err(0);
    }
    let mut acc = 0;
    let mut list = vec![];

    for op in own_utxos {
        acc += all_utxos[op].value();
        list.push(op.clone());
        if acc >= amount {
            break;
        }
    }

    if acc >= amount {
        Ok((list, acc))
    } else {
        Err(acc)
    }
}

/// If the change_amount is greater than 0, insert a change output using the supplied `pkh`.
pub fn insert_change_output(outputs: &mut Vec<Output>, own_pkh: PublicKeyHash, change_amount: u64) {
    if change_amount > 0 {
        // Create change output
        outputs.push(Output::ValueTransfer(ValueTransferOutput {
            pkh: own_pkh,
            value: change_amount,
        }));
    }
}

/// Build value transfer transaction with the given outputs and fee.
pub fn build_vtt<S: std::hash::BuildHasher>(
    outputs: Vec<ValueTransferOutput>,
    fee: u64,
    own_utxos: &HashSet<OutputPointer, S>,
    own_pkh: PublicKeyHash,
    all_utxos: &UnspentOutputsPool,
) -> Result<TransactionBody, u64> {
    build_vtt_inner(outputs, None, fee, own_utxos, own_pkh, all_utxos)
}

/// Build data request transaction with the given outputs and fee.
pub fn build_drt<S: std::hash::BuildHasher>(
    dr: DataRequestOutput,
    fee: u64,
    own_utxos: &HashSet<OutputPointer, S>,
    own_pkh: PublicKeyHash,
    all_utxos: &UnspentOutputsPool,
) -> Result<TransactionBody, u64> {
    build_vtt_inner(vec![], Some(dr), fee, own_utxos, own_pkh, all_utxos)
}

/// Generic transaction builder: can build value transfer transactions and data
/// request transactions.
fn build_vtt_inner<S: std::hash::BuildHasher>(
    outputs: Vec<ValueTransferOutput>,
    dr: Option<DataRequestOutput>,
    fee: u64,
    own_utxos: &HashSet<OutputPointer, S>,
    own_pkh: PublicKeyHash,
    all_utxos: &UnspentOutputsPool,
) -> Result<TransactionBody, u64> {
    let output_value: u64 = outputs.iter().map(|x| x.value).sum::<u64>()
        + dr.as_ref()
            .map(DataRequestOutput::value)
            .unwrap_or_default();
    match take_enough_utxos(own_utxos, all_utxos, output_value + fee) {
        Err(sum_of_own_utxos) => Err(sum_of_own_utxos),
        Ok((output_pointers, input_value)) => {
            let inputs = output_pointers
                .into_iter()
                .map(|x| {
                    Input::ValueTransfer(ValueTransferInput {
                        transaction_id: x.transaction_id,
                        output_index: x.output_index,
                    })
                })
                .collect();
            let mut outputs = outputs.into_iter().map(Output::ValueTransfer).collect();
            insert_change_output(&mut outputs, own_pkh, input_value - output_value - fee);
            if let Some(dro) = dr {
                outputs.push(Output::DataRequest(dro));
            }
            let tx_body = TransactionBody::new(0, inputs, outputs);
            Ok(tx_body)
        }
    }
}

/// Sign a transaction using this node's private key.
/// This function assumes that all the inputs have the same public key hash:
/// the hash of the public key of the node.
pub fn sign_transaction(
    tx_body: TransactionBody,
) -> impl Future<Item = Transaction, Error = failure::Error> {
    // Assuming that all the inputs have the same pkh
    signature_mngr::sign(&tx_body).map(move |signature| {
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
        let num_inputs = tx_body.inputs.len();
        let signatures = vec![signature; num_inputs];

        Transaction::new(tx_body, signatures)
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU32, Ordering};
    use witnet_data_structures::chain::{
        generate_unspent_outputs_pool, Hashable, PublicKey, RADRequest, TransactionType,
    };
    use witnet_validations::validations::transaction_tag;

    // Counter used to prevent creating two transactions with the same hash
    static TX_COUNTER: AtomicU32 = AtomicU32::new(0);

    fn my_pkh() -> PublicKeyHash {
        PublicKeyHash::from_public_key(&PublicKey {
            compressed: 2,
            bytes: [0x01; 32],
        })
    }

    fn build_utxo_set<T: Into<Option<(HashSet<OutputPointer>, UnspentOutputsPool)>>>(
        outputs: Vec<ValueTransferOutput>,
        own_utxos_all_utxos: T,
        txns: Vec<TransactionBody>,
    ) -> (HashSet<OutputPointer>, UnspentOutputsPool) {
        let own_pkh = my_pkh();
        let outputs = outputs
            .into_iter()
            .map(Output::ValueTransfer)
            .collect::<Vec<_>>();
        // Add a fake input to avoid hash collisions
        let fake_input = Input::ValueTransfer(ValueTransferInput {
            output_index: TX_COUNTER.fetch_add(1, Ordering::SeqCst),
            ..ValueTransferInput::default()
        });
        let mut txns: Vec<Transaction> = txns
            .into_iter()
            .map(|tx_body| Transaction::new(tx_body, vec![]))
            .collect();
        txns.push(Transaction::new(
            TransactionBody::new(0, vec![fake_input], outputs),
            vec![],
        ));

        let (mut own_utxos, all_utxos) = own_utxos_all_utxos.into().unwrap_or_default();
        let all_utxos = generate_unspent_outputs_pool(&all_utxos, &txns);
        update_own_utxos(&mut own_utxos, own_pkh, &txns);

        (own_utxos, all_utxos)
    }

    fn update_own_utxos(
        own_utxos: &mut HashSet<OutputPointer>,
        own_pkh: PublicKeyHash,
        txns: &[Transaction],
    ) {
        for transaction in txns {
            // Remove spent inputs
            for input in &transaction.body.inputs {
                own_utxos.remove(&input.output_pointer());
            }
            // Insert new outputs
            for (i, output) in transaction.body.outputs.iter().enumerate() {
                if let Output::ValueTransfer(x) = output {
                    if x.pkh == own_pkh {
                        own_utxos.insert(OutputPointer {
                            transaction_id: transaction.hash(),
                            output_index: i as u32,
                        });
                    }
                }
            }
        }
    }

    fn outputs_sum(outputs: &[Output]) -> u64 {
        outputs.iter().map(Output::value).sum()
    }
    fn outputs_sum_not_mine(outputs: &[Output]) -> u64 {
        let pkh = my_pkh();
        outputs
            .iter()
            .map(|x| match x {
                Output::ValueTransfer(vt) if vt.pkh != pkh => x.value(),
                // Data requests do not count, because we cannot spent them, unlike vtts
                // Output::DataRequest(dr) if dr.pkh != pkh => x.value(),
                _ => 0,
            })
            .sum()
    }

    fn pay(pkh: PublicKeyHash, value: u64) -> ValueTransferOutput {
        ValueTransferOutput { pkh, value }
    }

    fn pay_me(value: u64) -> ValueTransferOutput {
        pay(my_pkh(), value)
    }

    fn pay_alice(value: u64) -> ValueTransferOutput {
        let alice_pkh = PublicKeyHash::from_public_key(&PublicKey {
            compressed: 2,
            bytes: [0x03; 32],
        });

        pay(alice_pkh, value)
    }

    fn pay_bob(value: u64) -> ValueTransferOutput {
        let bob_pkh = PublicKeyHash::from_public_key(&PublicKey {
            compressed: 2,
            bytes: [0x04; 32],
        });

        pay(bob_pkh, value)
    }

    #[test]
    fn empty_utxo() {
        let own_pkh = my_pkh();
        let outputs = vec![];
        let (own_utxos, all_utxos) = build_utxo_set(outputs, None, vec![]);
        // Outputs was empty, so own_utxos is also empty
        assert!(own_utxos.is_empty(), "{:?}", own_utxos);

        // Building a zero value transaction returns an error
        assert_eq!(
            build_vtt(vec![], 0, &own_utxos, own_pkh, &all_utxos),
            Err(0)
        );

        // Building any transaction with an empty own_utxos returns an error
        assert_eq!(
            build_vtt(vec![pay_bob(1000)], 0, &own_utxos, own_pkh, &all_utxos),
            Err(0)
        );
        assert_eq!(
            build_vtt(vec![], 50, &own_utxos, own_pkh, &all_utxos),
            Err(0)
        );
        assert_eq!(
            build_vtt(
                vec![pay_me(0), pay_bob(0)],
                0,
                &own_utxos,
                own_pkh,
                &all_utxos
            ),
            Err(0)
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
        let (own_utxos, all_utxos) = build_utxo_set(outputs, (own_utxos, all_utxos), vec![]);
        // There are 2 pay_me in outputs, so there should be 2 new outputs in own_utxos
        assert_eq!(own_utxos.len(), 2 + 2);

        // The total value of own_utxos is 300, so trying to spend more than 300 will fail
        assert_eq!(
            build_vtt(vec![], 301, &own_utxos, own_pkh, &all_utxos),
            Err(300)
        );
    }

    #[test]
    fn poor_utxo() {
        let own_pkh = my_pkh();
        let outputs = vec![pay_me(1000)];
        let (own_utxos, all_utxos) = build_utxo_set(outputs, None, vec![]);
        // There is one pay_me in outputs, so there should be one output in own_utxos
        assert_eq!(own_utxos.len(), 1);

        assert_eq!(
            build_vtt(vec![pay_bob(2000)], 0, &own_utxos, own_pkh, &all_utxos),
            Err(1000)
        );
        assert_eq!(
            build_vtt(vec![], 1001, &own_utxos, own_pkh, &all_utxos),
            Err(1000)
        );
        assert_eq!(
            build_vtt(vec![pay_bob(500)], 600, &own_utxos, own_pkh, &all_utxos),
            Err(1000)
        );
        assert_eq!(
            build_vtt(vec![pay_bob(500)], 600, &own_utxos, own_pkh, &all_utxos),
            Err(1000)
        );
    }

    #[test]
    fn exact_change() {
        let own_pkh = my_pkh();
        let outputs = vec![pay_me(1000)];
        let (own_utxos, all_utxos) = build_utxo_set(outputs, None, vec![]);
        assert_eq!(own_utxos.len(), 1);

        let t1 = build_vtt(vec![pay_bob(1000)], 0, &own_utxos, own_pkh, &all_utxos).unwrap();
        assert_eq!(outputs_sum(&t1.outputs), 1000);
        assert_eq!(transaction_tag(&t1), TransactionType::ValueTransfer);

        let t2 = build_vtt(vec![pay_bob(990)], 10, &own_utxos, own_pkh, &all_utxos).unwrap();
        assert_eq!(outputs_sum(&t2.outputs), 990);
        assert_eq!(transaction_tag(&t2), TransactionType::ValueTransfer);

        let t3 = build_vtt(vec![], 1000, &own_utxos, own_pkh, &all_utxos).unwrap();
        assert_eq!(outputs_sum(&t3.outputs), 0);
        assert_eq!(transaction_tag(&t3), TransactionType::ValueTransfer);

        let t4 = build_vtt(
            vec![pay_bob(500), pay_me(500)],
            0,
            &own_utxos,
            own_pkh,
            &all_utxos,
        )
        .unwrap();
        assert_eq!(outputs_sum(&t4.outputs), 1000);
        assert_eq!(outputs_sum_not_mine(&t4.outputs), 500);
        assert_eq!(transaction_tag(&t4), TransactionType::ValueTransfer);

        // Execute transaction t1
        let (own_utxos, _all_utxos) = build_utxo_set(vec![], (own_utxos, all_utxos), vec![t1]);
        assert!(own_utxos.is_empty(), "{:?}", own_utxos);
    }

    #[test]
    fn one_big_utxo() {
        let own_pkh = my_pkh();
        let outputs = vec![pay_me(1_000_000)];
        let (own_utxos, all_utxos) = build_utxo_set(outputs, None, vec![]);
        assert_eq!(own_utxos.len(), 1);

        let t1 = build_vtt(vec![pay_bob(1000)], 0, &own_utxos, own_pkh, &all_utxos).unwrap();
        assert_eq!(outputs_sum(&t1.outputs), 1_000_000);
        assert_eq!(outputs_sum_not_mine(&t1.outputs), 1000);

        let t2 = build_vtt(vec![pay_bob(990)], 10, &own_utxos, own_pkh, &all_utxos).unwrap();
        assert_eq!(outputs_sum(&t2.outputs), 999_990);
        assert_eq!(outputs_sum_not_mine(&t2.outputs), 990);

        let t3 = build_vtt(vec![], 1000, &own_utxos, own_pkh, &all_utxos).unwrap();
        assert_eq!(outputs_sum(&t3.outputs), 999_000);
        assert_eq!(outputs_sum_not_mine(&t3.outputs), 0);

        let t4 = build_vtt(
            vec![pay_bob(500), pay_me(500)],
            0,
            &own_utxos,
            own_pkh,
            &all_utxos,
        )
        .unwrap();
        assert_eq!(outputs_sum_not_mine(&t4.outputs), 500);

        // Execute transaction t1
        // This should create a change output with value 1_000_000 - 1_000
        let (own_utxos, all_utxos) = build_utxo_set(vec![], (own_utxos, all_utxos), vec![t1]);
        assert_eq!(own_utxos.len(), 1);
        assert_eq!(
            all_utxos[own_utxos.iter().next().unwrap()].value(),
            1_000_000 - 1_000
        );
        assert_eq!(
            build_vtt(
                vec![],
                1_000_000 - 1_000 + 1,
                &own_utxos,
                own_pkh,
                &all_utxos
            ),
            Err(1_000_000 - 1_000)
        );

        // Now we can spend that new utxo
        let t5 = build_vtt(vec![], 1_000_000 - 1_000, &own_utxos, own_pkh, &all_utxos).unwrap();
        // Execute transaction t5
        let (own_utxos, _all_utxos) = build_utxo_set(vec![], (own_utxos, all_utxos), vec![t5]);
        assert!(own_utxos.is_empty(), "{:?}", own_utxos);
    }

    #[test]
    fn many_small_utxos() {
        let own_pkh = my_pkh();
        // 1000 utxos with 1 value each
        let outputs = vec![pay_me(1); 1000];
        let (own_utxos, all_utxos) = build_utxo_set(outputs, None, vec![]);
        assert_eq!(own_utxos.len(), 1000);

        let t1 = build_vtt(vec![pay_bob(1000)], 0, &own_utxos, own_pkh, &all_utxos).unwrap();
        assert_eq!(outputs_sum(&t1.outputs), 1000);
        assert_eq!(t1.inputs.len(), 1000);

        let t2 = build_vtt(vec![pay_bob(990)], 10, &own_utxos, own_pkh, &all_utxos).unwrap();

        assert_eq!(outputs_sum(&t2.outputs), 990);
        assert_eq!(t2.inputs.len(), 1000);

        let t3 = build_vtt(vec![], 1000, &own_utxos, own_pkh, &all_utxos).unwrap();
        assert_eq!(outputs_sum(&t3.outputs), 0);
        assert_eq!(t3.inputs.len(), 1000);

        let t4 = build_vtt(vec![pay_bob(500)], 20, &own_utxos, own_pkh, &all_utxos).unwrap();
        assert_eq!(outputs_sum(&t4.outputs), 500);
        assert_eq!(t4.inputs.len(), 520);

        // Execute transaction t4
        // This will not create any change outputs because all our utxos have value 1
        let (own_utxos, all_utxos) = build_utxo_set(vec![], (own_utxos, all_utxos), vec![t4]);
        assert_eq!(own_utxos.len(), 480);

        assert_eq!(
            build_vtt(vec![], 480 + 1, &own_utxos, own_pkh, &all_utxos),
            Err(480)
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
        let (own_utxos, all_utxos) = build_utxo_set(outputs, None, vec![]);
        assert_eq!(own_utxos.len(), 7);

        let t1 = build_vtt(vec![pay_bob(1000)], 0, &own_utxos, own_pkh, &all_utxos).unwrap();
        assert_eq!(outputs_sum_not_mine(&t1.outputs), 1000);

        let t2 = build_vtt(vec![pay_bob(990)], 10, &own_utxos, own_pkh, &all_utxos).unwrap();

        assert_eq!(outputs_sum_not_mine(&t2.outputs), 990);

        let t3 = build_vtt(vec![], 1000, &own_utxos, own_pkh, &all_utxos).unwrap();
        assert_eq!(outputs_sum_not_mine(&t3.outputs), 0);

        let t4 = build_vtt(vec![pay_bob(500)], 20, &own_utxos, own_pkh, &all_utxos).unwrap();
        assert_eq!(outputs_sum_not_mine(&t4.outputs), 500);

        // Execute transaction t4
        let (own_utxos, all_utxos) = build_utxo_set(vec![], (own_utxos, all_utxos), vec![t4]);
        // This will create a change output with an unknown value, but the total available will be 1000 - 520
        assert_eq!(
            build_vtt(vec![], 480 + 1, &own_utxos, own_pkh, &all_utxos),
            Err(480)
        );

        // A transaction to ourselves with no fees will maintain our total balance
        let t5 = build_vtt(vec![pay_me(480)], 0, &own_utxos, own_pkh, &all_utxos).unwrap();
        // Execute transaction t5
        let (own_utxos, all_utxos) = build_utxo_set(vec![], (own_utxos, all_utxos), vec![t5]);
        // Since we are spending everything, the result is merging all the unspent outputs into one
        assert_eq!(own_utxos.len(), 1);
        assert_eq!(all_utxos[own_utxos.iter().next().unwrap()].value(), 480);
        assert_eq!(
            build_vtt(vec![], 480 + 1, &own_utxos, own_pkh, &all_utxos),
            Err(480)
        );

        // Now spend everything
        let t6 = build_vtt(vec![pay_bob(400)], 80, &own_utxos, own_pkh, &all_utxos).unwrap();
        // Execute transaction t6
        let (own_utxos, _all_utxos) = build_utxo_set(vec![], (own_utxos, all_utxos), vec![t6]);
        assert!(own_utxos.is_empty(), "{:?}", own_utxos);
    }

    #[test]
    fn exact_change_data_request() {
        let own_pkh = my_pkh();
        let outputs = vec![pay_me(1000)];
        let (own_utxos, all_utxos) = build_utxo_set(outputs, None, vec![]);
        assert_eq!(own_utxos.len(), 1);

        let t1 = build_drt(
            DataRequestOutput {
                pkh: own_pkh,
                data_request: RADRequest::default(),
                value: 1000,
                witnesses: 4,
                backup_witnesses: 0,
                commit_fee: 0,
                reveal_fee: 0,
                tally_fee: 0,
                time_lock: 0,
            },
            0,
            &own_utxos,
            own_pkh,
            &all_utxos,
        )
        .unwrap();
        assert_eq!(transaction_tag(&t1), TransactionType::DataRequest);
        assert_eq!(outputs_sum(&t1.outputs), 1000);

        let t2 = build_drt(
            DataRequestOutput {
                pkh: own_pkh,
                data_request: RADRequest::default(),
                value: 200,
                witnesses: 4,
                backup_witnesses: 0,
                commit_fee: 300,
                reveal_fee: 400,
                tally_fee: 100,
                time_lock: 0,
            },
            0,
            &own_utxos,
            own_pkh,
            &all_utxos,
        )
        .unwrap();
        assert_eq!(outputs_sum(&t2.outputs), 1000);
        assert_eq!(transaction_tag(&t2), TransactionType::DataRequest);

        // Execute transaction t2
        let (own_utxos, _all_utxos) = build_utxo_set(vec![], (own_utxos, all_utxos), vec![t2]);
        assert!(own_utxos.is_empty(), "{:?}", own_utxos);
    }

    #[test]
    fn one_big_utxo_data_request() {
        let own_pkh = my_pkh();
        let outputs = vec![pay_me(1_000_000)];
        let (own_utxos, all_utxos) = build_utxo_set(outputs, None, vec![]);
        assert_eq!(own_utxos.len(), 1);

        let t1 = build_drt(
            DataRequestOutput {
                pkh: own_pkh,
                data_request: RADRequest::default(),
                value: 1000,
                witnesses: 4,
                backup_witnesses: 0,
                commit_fee: 0,
                reveal_fee: 0,
                tally_fee: 0,
                time_lock: 0,
            },
            0,
            &own_utxos,
            own_pkh,
            &all_utxos,
        )
        .unwrap();
        assert_eq!(outputs_sum(&t1.outputs), 1_000_000);
        assert_eq!(transaction_tag(&t1), TransactionType::DataRequest);

        let t2 = build_drt(
            DataRequestOutput {
                pkh: own_pkh,
                data_request: RADRequest::default(),
                value: 200,
                witnesses: 4,
                backup_witnesses: 0,
                commit_fee: 300,
                reveal_fee: 400,
                tally_fee: 100,
                time_lock: 0,
            },
            0,
            &own_utxos,
            own_pkh,
            &all_utxos,
        )
        .unwrap();
        assert_eq!(outputs_sum(&t2.outputs), 1_000_000);
        assert_eq!(transaction_tag(&t2), TransactionType::DataRequest);

        // Execute transaction t2
        let (own_utxos, all_utxos) = build_utxo_set(vec![], (own_utxos, all_utxos), vec![t2]);
        // This will create a change output with value 1_000_000 - 1_000
        assert_eq!(own_utxos.len(), 1);
        assert_eq!(
            all_utxos[own_utxos.iter().next().unwrap()].value(),
            1_000_000 - 1_000
        );
        assert_eq!(
            build_vtt(
                vec![],
                1_000_000 - 1_000 + 1,
                &own_utxos,
                own_pkh,
                &all_utxos
            ),
            Err(1_000_000 - 1_000)
        );
    }
}
