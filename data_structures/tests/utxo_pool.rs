use std::convert::TryFrom;
use witnet_data_structures::chain::{Hash, Hashable, Input};
use witnet_data_structures::{
    chain::{OutputPointer, ValueTransferOutput},
    transaction::{Transaction, VTTransaction, VTTransactionBody},
    utxo_pool::{OwnUnspentOutputsPool, UnspentOutputsPool},
};

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

#[test]
fn utxo_set_coin_age() {
    let mut p = UnspentOutputsPool::default();
    let v = ValueTransferOutput::default;

    let k0: OutputPointer = "0222222222222222222222222222222222222222222222222222222222222222:0"
        .parse()
        .unwrap();
    p.insert(k0.clone(), v(), 0);
    assert_eq!(p.included_in_block_number(&k0), Some(0));

    let k1: OutputPointer = "1222222222222222222222222222222222222222222222222222222222222222:0"
        .parse()
        .unwrap();
    p.insert(k1.clone(), v(), 1);
    assert_eq!(p.included_in_block_number(&k1), Some(1));

    // k2 points to the same transaction as k1, so they must have the same coin age
    let k2: OutputPointer = "1222222222222222222222222222222222222222222222222222222222222222:1"
        .parse()
        .unwrap();
    p.insert(k2.clone(), v(), 1);
    assert_eq!(p.included_in_block_number(&k2), Some(1));

    // Removing k2 should not affect k1
    p.remove(&k2);
    assert_eq!(p.included_in_block_number(&k2), None);
    assert_eq!(p.included_in_block_number(&k1), Some(1));
    assert_eq!(p.included_in_block_number(&k0), Some(0));

    p.remove(&k1);
    assert_eq!(p.included_in_block_number(&k2), None);
    assert_eq!(p.included_in_block_number(&k1), None);
    assert_eq!(p.included_in_block_number(&k0), Some(0));

    p.remove(&k0);
    assert_eq!(p.included_in_block_number(&k0), None);

    assert_eq!(p.iter().count(), 0);
}

#[test]
fn utxo_set_insert_twice() {
    // Inserting the same input twice into the UTXO set overwrites the transaction
    let mut p = UnspentOutputsPool::default();
    let v = ValueTransferOutput::default;

    let k0: OutputPointer = "0222222222222222222222222222222222222222222222222222222222222222:0"
        .parse()
        .unwrap();
    p.insert(k0.clone(), v(), 0);
    p.insert(k0.clone(), v(), 0);
    assert_eq!(p.included_in_block_number(&k0), Some(0));
    // Removing once is enough
    p.remove(&k0);
    assert_eq!(p.included_in_block_number(&k0), None);
}

#[test]
fn utxo_set_insert_same_transaction_different_epoch() {
    // Inserting the same transaction twice with different indexes means a different UTXO
    // so, each UTXO keeps their own block number
    let mut p = UnspentOutputsPool::default();
    let v = ValueTransferOutput::default;

    let k0: OutputPointer = "0222222222222222222222222222222222222222222222222222222222222222:0"
        .parse()
        .unwrap();
    p.insert(k0.clone(), v(), 0);
    assert_eq!(p.included_in_block_number(&k0), Some(0));
    let k1: OutputPointer = "0222222222222222222222222222222222222222222222222222222222222222:1"
        .parse()
        .unwrap();

    p.insert(k1.clone(), v(), 1);
    assert_eq!(p.included_in_block_number(&k1), Some(1));
}

#[test]
fn test_sort_own_utxos() {
    let vto1 = ValueTransferOutput {
        value: 100,
        ..ValueTransferOutput::default()
    };
    let vto2 = ValueTransferOutput {
        value: 500,
        ..ValueTransferOutput::default()
    };
    let vto3 = ValueTransferOutput {
        value: 200,
        ..ValueTransferOutput::default()
    };
    let vto4 = ValueTransferOutput {
        value: 300,
        ..ValueTransferOutput::default()
    };

    let vt = Transaction::ValueTransfer(VTTransaction::new(
        VTTransactionBody::new(vec![], vec![vto1, vto2, vto3, vto4]),
        vec![],
    ));

    let utxo_pool = generate_unspent_outputs_pool(&UnspentOutputsPool::default(), &[vt], 0);
    assert_eq!(utxo_pool.iter().count(), 4);

    let mut own_utxos = OwnUnspentOutputsPool::default();
    for (o, _) in utxo_pool.iter() {
        own_utxos.insert(o.clone(), 0);
    }
    assert_eq!(own_utxos.len(), 4);

    let sorted_bigger = own_utxos.sort(&utxo_pool, true);
    let mut aux = 1000;
    for o in sorted_bigger.iter() {
        let value = utxo_pool.get(o).unwrap().value;
        assert!(value < aux);
        aux = value;
    }

    let sorted_lower = own_utxos.sort(&utxo_pool, false);
    let mut aux = 0;
    for o in sorted_lower.iter() {
        let value = utxo_pool.get(o).unwrap().value;
        assert!(value > aux);
        aux = value;
    }
}
