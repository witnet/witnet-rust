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
    let output_value: u64 = outputs.iter().map(|x| x.value).sum();
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
            let tx_body = TransactionBody::new(0, inputs, outputs);
            Ok(tx_body)
        }
    }
}

/// Build data request transaction with the given outputs and fee.
pub fn build_drt<S: std::hash::BuildHasher>(
    dr: DataRequestOutput,
    fee: u64,
    own_utxos: &HashSet<OutputPointer, S>,
    own_pkh: PublicKeyHash,
    all_utxos: &UnspentOutputsPool,
) -> Result<TransactionBody, u64> {
    let dro = Output::DataRequest(dr);
    let output_value: u64 = dro.value();
    match take_enough_utxos(own_utxos, all_utxos, output_value + fee) {
        Err(sum_of_own_utxos) => Err(sum_of_own_utxos),
        Ok((output_pointers, input_value)) => {
            let mut outputs = vec![];
            insert_change_output(&mut outputs, own_pkh, input_value - output_value - fee);
            outputs.push(dro);

            let inputs = output_pointers
                .into_iter()
                .map(|x| {
                    Input::ValueTransfer(ValueTransferInput {
                        transaction_id: x.transaction_id,
                        output_index: x.output_index,
                    })
                })
                .collect();
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
