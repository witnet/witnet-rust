use std::{collections::HashMap, iter::FromIterator as _, mem};

use witnet_data_structures::{
    chain::Hashable, transaction::VTTransaction, transaction_factory::calculate_weight,
};

use crate::{db::HashMapDb, repository::wallet::tests::factories::vtt_from_body, *};

use super::*;

mod factories;

#[test]
fn test_wallet_public_data() {
    let (wallet, _db) = factories::wallet(None);
    let data = wallet.public_data().unwrap();

    assert!(data.name.is_none());
    assert!(data.description.is_none());
    assert_eq!(0, data.balance.local);
    assert_eq!(0, data.balance.unconfirmed.available);
    assert_eq!(0, data.balance.unconfirmed.locked);
    assert_eq!(0, data.balance.confirmed.available);
    assert_eq!(0, data.balance.confirmed.locked);
    assert_eq!(0, data.current_account);
    assert_eq!(vec![0], data.available_accounts);
}

#[test]
fn test_gen_external_address() {
    let (wallet, _db) = factories::wallet(None);
    let label = "address label".to_string();
    let address = wallet.gen_external_address(Some(label.clone())).unwrap();

    assert!(address.address.starts_with("wit"));
    assert_eq!("m/3'/4919'/0'/0/0", &address.path);
    assert_eq!(Some(label), address.info.label);

    let address_no_label = wallet.gen_external_address(None).unwrap();

    assert_eq!(None, address_no_label.info.label);
}

#[test]
fn test_gen_external_address_creates_different_addresses() {
    let (wallet, _db) = factories::wallet(None);
    let address = wallet.gen_external_address(None).unwrap();

    assert_eq!("m/3'/4919'/0'/0/0", &address.path);
    assert_eq!(0, address.index);

    let new_address = wallet.gen_external_address(None).unwrap();

    assert_eq!("m/3'/4919'/0'/0/1", &new_address.path);
    assert_eq!(1, new_address.index);
}

#[test]
fn test_gen_external_address_stores_next_address_index_in_db() {
    let (wallet, db) = factories::wallet(None);
    let account = 0;
    let keychain = constants::EXTERNAL_KEYCHAIN;

    wallet.gen_external_address(None).unwrap();

    assert_eq!(
        1,
        db.get(&keys::account_next_index(account, keychain))
            .unwrap()
    );

    wallet.gen_external_address(None).unwrap();

    assert_eq!(
        2,
        db.get(&keys::account_next_index(account, keychain))
            .unwrap()
    );
}

#[test]
fn test_gen_external_address_saves_details_in_db() {
    let (wallet, db) = factories::wallet(None);
    let account = 0;
    let keychain = constants::EXTERNAL_KEYCHAIN;
    let index = 0;
    let label = "address label".to_string();
    let address = wallet.gen_external_address(Some(label.clone())).unwrap();

    assert_eq!(
        address.address,
        db.get(&keys::address(account, keychain, index)).unwrap()
    );
    assert_eq!(
        address.path,
        db.get(&keys::address_path(account, keychain, index))
            .unwrap()
    );
    assert_eq!(
        address.pkh,
        db.get(&keys::address_pkh(account, keychain, index))
            .unwrap()
    );

    let address_info: model::AddressInfo = db
        .get(&keys::address_info(account, keychain, index))
        .unwrap();

    assert_eq!(label, address_info.label.unwrap());
    assert!(address_info.first_payment_date.is_none());
    assert!(address_info.last_payment_date.is_none());
    assert_eq!(0, address_info.received_amount);
    assert!(address_info.received_payments.is_empty());
}

#[test]
fn test_gen_external_address_associates_pkh_to_account_in_db() {
    let (wallet, db) = factories::wallet(None);
    let account = 0;
    let keychain = constants::EXTERNAL_KEYCHAIN;
    let address = wallet.gen_external_address(None).unwrap();
    let pkh = &address.pkh;

    let path: model::Path = db.get(&keys::pkh(pkh)).unwrap();

    assert_eq!(account, path.account);
    assert_eq!(keychain, path.keychain);
    assert_eq!(0, path.index);
}

#[test]
fn test_list_internal_addresses() {
    let (wallet, _db) = factories::wallet(None);

    let address1 = (*wallet.gen_internal_address(None).unwrap()).clone();
    let address2 = (*wallet.gen_internal_address(None).unwrap()).clone();
    let address3 = (*wallet.gen_internal_address(None).unwrap()).clone();

    let offset = 0;
    let limit = 10;
    let addresses = wallet.internal_addresses(offset, limit).unwrap();

    assert_eq!(3, addresses.total);
    assert_eq!(address3, addresses[0]);
    assert_eq!(address2, addresses[1]);
    assert_eq!(address1, addresses[2]);
}

#[test]
fn test_list_internal_addresses_paginated() {
    let (wallet, _db) = factories::wallet(None);

    let _ = wallet.gen_internal_address(None).unwrap();
    let address = (*wallet.gen_internal_address(None).unwrap()).clone();
    let _ = wallet.gen_internal_address(None).unwrap();

    let offset = 1;
    let limit = 1;
    let addresses = wallet.internal_addresses(offset, limit).unwrap();

    assert_eq!(3, addresses.total);
    assert_eq!(1, addresses.len());
    assert_eq!(address, addresses[0]);
}

#[test]
fn test_get_address() {
    let (wallet, _db) = factories::wallet(None);
    let account = 0;
    let keychain = constants::EXTERNAL_KEYCHAIN;
    let index = 0;

    let res = wallet.get_address(account, keychain, index);

    assert!(res.is_err());

    let address = wallet.gen_external_address(None).unwrap();
    let res = wallet.get_address(account, keychain, index);

    assert!(res.is_ok());
    assert_eq!(&address.address, &res.unwrap().address);
}

#[test]
fn test_gen_internal_address() {
    let (wallet, _db) = factories::wallet(None);
    let label = "address label".to_string();
    let address = wallet.gen_internal_address(Some(label.clone())).unwrap();

    assert!(address.address.starts_with("wit"));
    assert_eq!("m/3'/4919'/0'/1/0", &address.path);
    assert_eq!(Some(label), address.info.label);

    let address_no_label = wallet.gen_internal_address(None).unwrap();

    assert_eq!(None, address_no_label.info.label);
}

#[test]
fn test_gen_internal_address_creates_different_addresses() {
    let (wallet, _db) = factories::wallet(None);
    let address = wallet.gen_internal_address(None).unwrap();

    assert_eq!("m/3'/4919'/0'/1/0", &address.path);
    assert_eq!(0, address.index);

    let new_address = wallet.gen_internal_address(None).unwrap();

    assert_eq!("m/3'/4919'/0'/1/1", &new_address.path);
    assert_eq!(1, new_address.index);
}

#[test]
fn test_gen_internal_address_stores_next_address_index_in_db() {
    let (wallet, db) = factories::wallet(None);
    let account = 0;
    let keychain = constants::INTERNAL_KEYCHAIN;
    wallet.gen_internal_address(None).unwrap();

    assert_eq!(
        1,
        db.get(&keys::account_next_index(account, keychain,))
            .unwrap()
    );

    wallet.gen_internal_address(None).unwrap();

    assert_eq!(
        2,
        db.get(&keys::account_next_index(account, keychain))
            .unwrap()
    );
}

#[test]
fn test_gen_internal_address_saves_details_in_db() {
    let (wallet, db) = factories::wallet(None);
    let account = 0;
    let keychain = constants::INTERNAL_KEYCHAIN;
    let index = 0;
    let label = "address label".to_string();
    let address = wallet.gen_internal_address(Some(label.clone())).unwrap();

    assert_eq!(
        address.address,
        db.get(&keys::address(account, keychain, index)).unwrap()
    );
    assert_eq!(
        address.path,
        db.get(&keys::address_path(account, keychain, index))
            .unwrap()
    );
    assert_eq!(
        address.pkh,
        db.get(&keys::address_pkh(account, keychain, index))
            .unwrap()
    );
    assert_eq!(
        label,
        db.get(&keys::address_info(account, keychain, index))
            .unwrap()
            .label
            .unwrap()
    );
}

#[test]
fn test_gen_internal_address_associates_pkh_to_account_in_db() {
    let (wallet, db) = factories::wallet(None);
    let account = 0;
    let keychain = constants::INTERNAL_KEYCHAIN;
    let address = wallet.gen_internal_address(None).unwrap();
    let pkh = &address.pkh;

    let path: model::Path = db.get(&keys::pkh(pkh)).unwrap();

    assert_eq!(account, path.account);
    assert_eq!(keychain, path.keychain,);
    assert_eq!(0, path.index);
}

#[test]
fn test_custom_kv() {
    let (wallet, _db) = factories::wallet(None);

    wallet.kv_set("my-key", "my-value").unwrap();

    assert_eq!(
        Some("my-value".to_string()),
        wallet.kv_get("my-key").unwrap()
    );

    wallet.kv_set("my-key", "my-other-value").unwrap();

    assert_eq!(
        Some("my-other-value".to_string()),
        wallet.kv_get("my-key").unwrap()
    );
}

#[test]
fn test_balance() {
    let (wallet, db) = factories::wallet(None);

    let balance = wallet.balance().unwrap();
    assert_eq!(0, balance.local);
    assert_eq!(0, balance.unconfirmed.available);
    assert_eq!(0, balance.unconfirmed.locked);
    assert_eq!(0, balance.confirmed.available);
    assert_eq!(0, balance.confirmed.locked);

    let new_balance = model::BalanceInfo {
        available: 99u64,
        locked: 0u64,
    };
    db.put(&keys::account_balance(0), new_balance).unwrap();

    let (wallet, _db) = factories::wallet(Some(db));

    assert_eq!(99, wallet.balance().unwrap().confirmed.available);
    assert_eq!(0, wallet.balance().unwrap().confirmed.locked);
}

#[test]
fn test_create_transaction_components_when_wallet_have_no_utxos() {
    let (wallet, _db) = factories::wallet(None);
    let mut state = wallet.state.write().unwrap();
    let value = 1;
    let fee = Fee::default();
    let pkh = factories::pkh();
    let time_lock = 0;
    let utxo_strategy = UtxoSelectionStrategy::Random { from: None };
    let vto = ValueTransferOutput {
        pkh,
        value,
        time_lock,
    };
    let err = wallet
        .create_vt_transaction_components(
            &mut state,
            vec![vto],
            fee,
            &utxo_strategy,
            HashSet::default(),
        )
        .unwrap_err();

    assert!(
        matches!(err, repository::Error::InsufficientBalance { .. }),
        "{:?}",
        err
    );
}

#[test]
fn test_create_transaction_components_without_a_change_address() {
    let pkh = factories::pkh();
    let out_pointer = model::OutPtr {
        txn_hash: vec![0; 32],
        output_index: 0,
    };
    let utxo_set: HashMap<model::OutPtr, model::OutputInfo> = HashMap::from_iter(vec![(
        out_pointer,
        model::OutputInfo {
            pkh,
            amount: 1,
            time_lock: 0,
        },
    )]);
    let path = model::Path {
        account: 0,
        keychain: constants::EXTERNAL_KEYCHAIN,
        index: 0,
    };

    let db = HashMapDb::default();
    db.put(&keys::account_utxo_set(0), utxo_set).unwrap();
    db.put(&keys::pkh(&pkh), path).unwrap();
    let (wallet, _db) = factories::wallet(Some(db));
    let mut state = wallet.state.write().unwrap();
    let pkh = factories::pkh();
    let value = 1;
    let fee = Fee::default();
    let time_lock = 0;
    let utxo_strategy = UtxoSelectionStrategy::Random { from: None };
    let vto = ValueTransferOutput {
        pkh,
        value,
        time_lock,
    };

    let TransactionComponents {
        inputs, outputs, ..
    } = wallet
        .create_vt_transaction_components(
            &mut state,
            vec![vto],
            fee,
            &utxo_strategy,
            HashSet::default(),
        )
        .unwrap();

    assert_eq!(1, inputs.pointers.len());
    assert_eq!(1, inputs.resolved.len());
    assert_eq!(1, outputs.len());
    assert_eq!(value, outputs[0].value);
}

#[test]
fn test_create_transaction_components_with_a_change_address() {
    let pkh = factories::pkh();
    let out_pointer = model::OutPtr {
        txn_hash: vec![0; 32],
        output_index: 0,
    };
    let utxo_set: HashMap<model::OutPtr, model::OutputInfo> = HashMap::from_iter(vec![(
        out_pointer,
        model::OutputInfo {
            pkh,
            amount: 2,
            time_lock: 0,
        },
    )]);
    let path = model::Path {
        account: 0,
        keychain: constants::EXTERNAL_KEYCHAIN,
        index: 0,
    };

    let db = HashMapDb::default();
    db.put(&keys::account_utxo_set(0), utxo_set).unwrap();
    db.put(&keys::pkh(&pkh), path).unwrap();
    let (wallet, _db) = factories::wallet(Some(db));
    let mut state = wallet.state.write().unwrap();
    let pkh = factories::pkh();
    let value = 1;
    let fee = Fee::default();
    let time_lock = 0;
    let utxo_strategy = UtxoSelectionStrategy::Random { from: None };

    let vto = ValueTransferOutput {
        pkh,
        value,
        time_lock,
    };

    let TransactionComponents {
        inputs, outputs, ..
    } = wallet
        .create_vt_transaction_components(
            &mut state,
            vec![vto],
            fee,
            &utxo_strategy,
            HashSet::default(),
        )
        .unwrap();

    assert_eq!(1, inputs.pointers.len());
    assert_eq!(1, inputs.resolved.len());
    assert_eq!(2, outputs.len());
    assert_eq!(value, outputs[0].value);
    let expected_change = 1;
    assert_eq!(expected_change, outputs[1].value);
}

#[test]
fn test_create_transaction_components_which_value_overflows() {
    let pkh = factories::pkh();
    let utxo_set: HashMap<model::OutPtr, model::OutputInfo> = HashMap::from_iter(vec![
        (
            model::OutPtr {
                txn_hash: vec![0; 32],
                output_index: 0,
            },
            model::OutputInfo {
                pkh,
                amount: 2,
                time_lock: 0,
            },
        ),
        (
            model::OutPtr {
                txn_hash: vec![0; 32],
                output_index: 1,
            },
            model::OutputInfo {
                pkh,
                amount: std::u64::MAX - 1,
                time_lock: 0,
            },
        ),
    ]);
    let path = model::Path {
        account: 0,
        keychain: constants::EXTERNAL_KEYCHAIN,
        index: 0,
    };
    let new_balance = model::BalanceInfo {
        available: std::u64::MAX,
        locked: 0u64,
    };

    let db = HashMapDb::default();
    db.put(&keys::account_utxo_set(0), utxo_set).unwrap();
    db.put(&keys::account_balance(0), new_balance).unwrap();
    db.put(&keys::pkh(&pkh), path).unwrap();
    let (wallet, _db) = factories::wallet(Some(db));
    let mut state = wallet.state.write().unwrap();
    let pkh = factories::pkh();
    let value = std::u64::MAX;
    let fee = Fee::default();
    let time_lock = 0;
    let utxo_strategy = UtxoSelectionStrategy::Random { from: None };
    let vto = ValueTransferOutput {
        pkh,
        value,
        time_lock,
    };
    let err = wallet
        .create_vt_transaction_components(
            &mut state,
            vec![vto],
            fee,
            &utxo_strategy,
            HashSet::default(),
        )
        .unwrap_err();

    assert_eq!(
        mem::discriminant(&repository::Error::TransactionValueOverflow),
        mem::discriminant(&err)
    );
}

#[test]
fn test_create_vtt_does_not_spend_utxos() {
    let pkh = factories::pkh();
    let out_pointer = model::OutPtr {
        txn_hash: vec![0; 32],
        output_index: 0,
    };
    let utxo_set: HashMap<model::OutPtr, model::OutputInfo> = HashMap::from_iter(vec![(
        out_pointer.clone(),
        model::OutputInfo {
            pkh,
            amount: 1,
            time_lock: 0,
        },
    )]);
    let path = model::Path {
        account: 0,
        keychain: constants::EXTERNAL_KEYCHAIN,
        index: 0,
    };
    let new_balance = model::BalanceInfo {
        available: 1u64,
        locked: 0u64,
    };

    let db = HashMapDb::default();
    db.put(&keys::account_utxo_set(0), utxo_set).unwrap();
    db.put(&keys::account_balance(0), new_balance).unwrap();
    db.put(&keys::pkh(&pkh), path).unwrap();
    let (wallet, db) = factories::wallet(Some(db));
    let pkh = factories::pkh();
    let value = 1;
    let fee = Fee::default();
    let time_lock = 0;
    let utxo_strategy = UtxoSelectionStrategy::Random { from: None };

    let state_utxo_set = wallet.utxo_set().unwrap();
    let utxo_set: HashMap<model::OutPtr, model::OutputInfo> =
        db.get(&keys::account_utxo_set(0)).unwrap();

    assert_eq!(1, wallet.balance().unwrap().confirmed.available);
    assert_eq!(0, wallet.balance().unwrap().confirmed.locked);
    assert!(utxo_set.contains_key(&out_pointer));
    assert!(state_utxo_set.contains_key(&out_pointer));

    let (extended, ..) = wallet
        .create_vtt(types::VttParams {
            fee,
            outputs: vec![ValueTransferOutput {
                pkh,
                value,
                time_lock,
            }],
            utxo_strategy,
            selected_utxos: HashSet::default(),
        })
        .unwrap();

    // the extended transaction should actually contain a value transfer transaction
    let vtt = if let Transaction::ValueTransfer(vtt) = extended.transaction {
        Some(vtt)
    } else {
        panic!("the extended transaction should actually contain a value transfer transaction, got: {:?}", extended.transaction);
    }
    .unwrap();

    // There is a signature for each input
    assert_eq!(vtt.body.inputs.len(), vtt.signatures.len());

    let state_utxo_set = wallet.utxo_set().unwrap();
    let new_utxo_set: HashMap<model::OutPtr, model::OutputInfo> =
        db.get(&keys::account_utxo_set(0)).unwrap();

    // nothing should change because VTT is only created but not yet confirmed (sent!)
    assert!(new_utxo_set.contains_key(&out_pointer));
    assert!(state_utxo_set.contains_key(&out_pointer));

    assert_eq!(1, wallet.balance().unwrap().confirmed.available);
    assert_eq!(0, wallet.balance().unwrap().confirmed.locked);
    assert_eq!(
        model::BalanceInfo {
            available: 1,
            locked: 0,
        },
        db.get(&keys::account_balance(0)).unwrap()
    );

    assert!(db
        .get(&keys::transactions_index(vtt.hash().as_ref()))
        .is_err());
}

#[test]
fn test_create_data_request_does_not_spend_utxos() {
    let pkh = factories::pkh();
    let out_pointer = model::OutPtr {
        txn_hash: vec![0; 32],
        output_index: 0,
    };
    let utxo_set: HashMap<model::OutPtr, model::OutputInfo> = HashMap::from_iter(vec![(
        out_pointer.clone(),
        model::OutputInfo {
            pkh,
            amount: 1000,
            time_lock: 0,
        },
    )]);
    let path = model::Path {
        account: 0,
        keychain: constants::EXTERNAL_KEYCHAIN,
        index: 0,
    };
    let new_balance = model::BalanceInfo {
        available: 1u64,
        locked: 0u64,
    };

    let db = HashMapDb::default();
    db.put(&keys::account_utxo_set(0), utxo_set).unwrap();
    db.put(&keys::account_balance(0), new_balance).unwrap();
    db.put(&keys::pkh(&pkh), path).unwrap();
    let (wallet, db) = factories::wallet(Some(db));

    let state_utxo_set = wallet.utxo_set().unwrap();
    let utxo_set: HashMap<model::OutPtr, model::OutputInfo> =
        db.get(&keys::account_utxo_set(0)).unwrap();

    assert_eq!(1, wallet.balance().unwrap().confirmed.available);
    assert_eq!(0, wallet.balance().unwrap().confirmed.locked);
    assert!(utxo_set.contains_key(&out_pointer));
    assert!(state_utxo_set.contains_key(&out_pointer));

    let request = DataRequestOutput {
        witness_reward: 5,
        witnesses: 99,
        ..DataRequestOutput::default()
    };

    let fee = Fee::absolute_from_nanowits(123);
    let (extended, _) = wallet
        .create_data_req(types::DataReqParams { fee, request })
        .unwrap();

    // the extended transaction should actually contain a data request transaction
    let data_req = if let Transaction::DataRequest(drt) = extended.transaction {
        Some(drt)
    } else {
        panic!("the extended transaction should actually contain a value transfer transaction, got: {:?}", extended.transaction);
    }
    .unwrap();

    // Make sure that the transaction contains a change output with the right change address and
    // amount (should reuse the address from the first input)
    let change_address = data_req.body.outputs[0].pkh;
    let change_amount = data_req.body.outputs[0].value;
    let change_timelock = data_req.body.outputs[0].time_lock;
    assert_eq!(change_address, pkh);
    assert_eq!(change_amount, 382);
    assert_eq!(change_timelock, 0);

    let state_utxo_set = wallet.utxo_set().unwrap();
    let new_utxo_set: HashMap<model::OutPtr, model::OutputInfo> =
        db.get(&keys::account_utxo_set(0)).unwrap();

    // nothing should change because DR is only created but not yet confirmed (sent!)
    assert!(new_utxo_set.contains_key(&out_pointer));
    assert!(state_utxo_set.contains_key(&out_pointer));

    assert_eq!(1, wallet.balance().unwrap().confirmed.available);
    assert_eq!(0, wallet.balance().unwrap().confirmed.locked);

    let db_balance = db.get(&keys::account_balance(0)).unwrap();
    assert_eq!(1, db_balance.available);
    assert_eq!(0, db_balance.locked);

    assert!(db
        .get(&keys::transactions_index(data_req.hash().as_ref()))
        .is_err());
}

#[test]
fn test_index_transaction_output_affects_balance() {
    let (wallet, db) = factories::wallet(None);

    assert_eq!(
        model::BalanceInfo {
            available: 0,
            locked: 0,
        },
        db.get_or_default(&keys::account_balance(0)).unwrap()
    );

    let value = 1u64;
    let address = wallet.gen_external_address(None).unwrap();
    let block = factories::BlockInfo::default().create();
    let inputs = vec![Input::default()];
    let outputs = vec![ValueTransferOutput {
        pkh: address.pkh,
        value,
        time_lock: 0,
    }];
    let body = VTTransactionBody::new(inputs, outputs);

    wallet
        .index_block_transactions(&block, &[vtt_from_body(body)], true)
        .unwrap();

    assert_eq!(
        model::BalanceInfo {
            available: 1,
            locked: 0,
        },
        db.get(&keys::account_balance(0)).unwrap()
    );
}

#[test]
fn test_index_transaction_input_affects_balance() {
    let (wallet, db) = factories::wallet(None);

    assert_eq!(
        model::BalanceInfo {
            available: 0,
            locked: 0,
        },
        db.get_or_default(&keys::account_balance(0)).unwrap()
    );

    let address = wallet.gen_external_address(None).unwrap();

    let a_block = factories::BlockInfo::default().create();

    // txn1 gives a credit of 3 to our pkh
    let txn1 = VTTransactionBody::new(
        vec![Input::default()],
        vec![ValueTransferOutput {
            pkh: address.pkh,
            value: 3,
            time_lock: 0,
        }],
    );

    // txn2 spends the previous credit and gives back a change of 1 to our pkh
    let txn2 = VTTransactionBody::new(
        vec![Input::new(OutputPointer {
            transaction_id: txn1.hash(),
            output_index: 0,
        })],
        vec![ValueTransferOutput {
            pkh: address.pkh,
            value: 1,
            time_lock: 0,
        }],
    );

    wallet
        .index_block_transactions(&a_block, &[vtt_from_body(txn1)], true)
        .unwrap();
    wallet
        .index_block_transactions(&a_block, &[vtt_from_body(txn2)], true)
        .unwrap();

    assert_eq!(
        model::BalanceInfo {
            available: 1,
            locked: 0,
        },
        db.get(&keys::account_balance(0)).unwrap()
    );
}

#[test]
fn test_index_transaction_updates_address_info() {
    let (wallet, db) = factories::wallet(None);

    assert_eq!(
        model::BalanceInfo {
            available: 0,
            locked: 0,
        },
        db.get_or_default(&keys::account_balance(0)).unwrap()
    );

    let value = 1u64;
    let address = wallet.gen_external_address(None).unwrap();
    let block = factories::BlockInfo::default().create();
    let inputs = vec![Input::default()];
    let outputs = vec![ValueTransferOutput {
        pkh: address.pkh,
        value,
        time_lock: 0,
    }];
    let body = VTTransactionBody::new(inputs, outputs);
    let vtt0 = vtt_from_body(body);
    let vtt0_hash = vtt0.transaction.hash();

    wallet
        .index_block_transactions(&block, &[vtt0], true)
        .unwrap();

    let address_updated = wallet
        .get_address(address.account, address.keychain, address.index)
        .unwrap();
    let expected_received_payments = vec![format!("{}:0", vtt0_hash)];
    assert_eq!(
        address_updated.info.received_payments,
        expected_received_payments,
    );
    assert_eq!(address_updated.info.received_amount, value,)
}

#[test]
fn test_index_transaction_updates_address_info_two_outputs_same_address() {
    // The total amount received in an address should be correctly updated when there is more than 1
    // output per transaction
    let (wallet, db) = factories::wallet(None);

    assert_eq!(
        model::BalanceInfo {
            available: 0,
            locked: 0,
        },
        db.get_or_default(&keys::account_balance(0)).unwrap()
    );

    let value = 1u64;
    let address = wallet.gen_external_address(None).unwrap();
    let block = factories::BlockInfo::default().create();
    let inputs = vec![Input::default()];
    let outputs = vec![
        ValueTransferOutput {
            pkh: address.pkh,
            value,
            time_lock: 0,
        };
        2
    ];
    let body = VTTransactionBody::new(inputs, outputs);
    let vtt0 = vtt_from_body(body);
    let vtt0_hash = vtt0.transaction.hash();

    wallet
        .index_block_transactions(&block, &[vtt0], true)
        .unwrap();

    let address_updated = wallet
        .get_address(address.account, address.keychain, address.index)
        .unwrap();
    let expected_received_payments = vec![format!("{}:0", vtt0_hash), format!("{}:1", vtt0_hash)];
    assert_eq!(
        address_updated.info.received_payments,
        expected_received_payments,
    );
    assert_eq!(address_updated.info.received_amount, value * 2,);
}

#[test]
fn test_index_transaction_updates_address_info_two_transactions_same_address() {
    // The total amount received in an address should be correctly updated when there is more than 1
    // transaction per block
    let (wallet, db) = factories::wallet(None);

    assert_eq!(
        model::BalanceInfo {
            available: 0,
            locked: 0,
        },
        db.get_or_default(&keys::account_balance(0)).unwrap()
    );
    let address = wallet.gen_external_address(None).unwrap();
    let block = factories::BlockInfo::default().create();
    let value = 1u64;

    let vtt0 = {
        let inputs = vec![Input::default()];
        let outputs = vec![ValueTransferOutput {
            pkh: address.pkh,
            value,
            time_lock: 0,
        }];
        let body = VTTransactionBody::new(inputs, outputs);
        vtt_from_body(body)
    };
    let vtt1 = {
        let inputs = vec![Input::default()];
        let outputs = vec![ValueTransferOutput {
            pkh: address.pkh,
            value,
            // Use different timelock to ensure different transaction hash
            time_lock: 1,
        }];
        let body = VTTransactionBody::new(inputs, outputs);
        vtt_from_body(body)
    };

    let vtt0_hash = vtt0.transaction.hash();
    let vtt1_hash = vtt1.transaction.hash();
    assert_ne!(vtt0_hash, vtt1_hash);

    wallet
        .index_block_transactions(&block, &[vtt0, vtt1], true)
        .unwrap();

    let address_updated = wallet
        .get_address(address.account, address.keychain, address.index)
        .unwrap();
    let expected_received_payments = vec![format!("{}:0", vtt0_hash), format!("{}:0", vtt1_hash)];
    assert_eq!(
        address_updated.info.received_payments,
        expected_received_payments,
    );
    assert_eq!(address_updated.info.received_amount, value * 2,);
}

#[test]
fn test_index_transaction_errors_if_balance_overflow() {
    let (wallet, _db) = factories::wallet(None);

    let address = wallet.gen_external_address(None).unwrap();
    let block = factories::BlockInfo::default().create();
    let inputs = vec![Input::default()];
    let outputs = vec![
        ValueTransferOutput {
            pkh: address.pkh,
            value: 1u64,
            time_lock: 0,
        },
        ValueTransferOutput {
            pkh: address.pkh,
            value: std::u64::MAX,
            time_lock: 0,
        },
    ];
    let txn = VTTransactionBody::new(inputs, outputs);

    let err = wallet
        .index_block_transactions(&block, &[factories::vtt_from_body(txn)], true)
        .unwrap_err();

    assert_eq!(
        mem::discriminant(&repository::Error::TransactionBalanceOverflow),
        mem::discriminant(&err)
    );
}

#[test]
fn test_index_transaction_vtt_created_by_wallet() {
    let (wallet, db) = factories::wallet(None);

    let a_block = factories::BlockInfo::default().create();
    let our_address = wallet.gen_external_address(None).unwrap();
    let their_pkh = factories::pkh();
    let fee = Fee::default();

    // index transaction to receive funds
    wallet
        .index_block_transactions(
            &a_block,
            &[factories::vtt_from_body(VTTransactionBody::new(
                vec![Input::default()],
                vec![ValueTransferOutput {
                    pkh: our_address.pkh,
                    value: 2,
                    time_lock: 0,
                }],
            ))],
            true,
        )
        .unwrap();

    // spend those funds to create a new transaction which is pending (it has no block)
    let (extended, ..) = wallet
        .create_vtt(types::VttParams {
            fee,
            outputs: vec![ValueTransferOutput {
                pkh: their_pkh,
                value: 1,
                time_lock: 0,
            }],
            utxo_strategy: UtxoSelectionStrategy::Random { from: None },
            selected_utxos: HashSet::default(),
        })
        .unwrap();

    // the extended transaction should actually contain a value transfer transaction
    let vtt = if let Transaction::ValueTransfer(vtt) = extended.transaction {
        Some(vtt)
    } else {
        panic!("the extended transaction should actually contain a value transfer transaction, got: {:?}", extended.transaction);
    }
    .unwrap();

    // There is a signature for each input
    assert_eq!(vtt.body.inputs.len(), vtt.signatures.len());

    // check that indeed, the previously created vtt has not been indexed
    let db_movement = db.get_opt(&keys::transaction_movement(0, 1)).unwrap();
    assert!(db_movement.is_none());

    // index another block confirming the previously created vtt
    wallet
        .index_block_transactions(&a_block, &[factories::vtt_from_body(vtt.body)], true)
        .unwrap();

    // check that indeed, the previously created vtt now has a block associated with it
    let block_after = db
        .get(&keys::transaction_movement(0, 1))
        .unwrap()
        .transaction
        .block;
    assert_eq!(Some(a_block), block_after);
}

#[test]
fn test_update_wallet_with_empty_values() {
    let (wallet, db) = factories::wallet(None);
    let wallet_data = wallet.public_data().unwrap();

    assert!(wallet_data.name.is_none());
    assert!(wallet_data.description.is_none());
    assert!(!db.contains(&keys::wallet_name()).unwrap());
    assert!(!db.contains(&keys::wallet_description()).unwrap());

    wallet.update(None, None).unwrap();

    let wallet_data = wallet.public_data().unwrap();

    assert!(wallet_data.name.is_none());
    assert!(wallet_data.description.is_none());
    assert!(!db.contains(&keys::wallet_name()).unwrap());
    assert!(!db.contains(&keys::wallet_description()).unwrap());
}

#[test]
fn test_update_wallet_with_values() {
    let (wallet, db) = factories::wallet(None);
    let wallet_data = wallet.public_data().unwrap();

    assert!(wallet_data.name.is_none());
    assert!(wallet_data.description.is_none());
    assert!(!db.contains(&keys::wallet_name()).unwrap());
    assert!(!db.contains(&keys::wallet_description()).unwrap());

    let name = Some("wallet name".to_string());
    let description = Some("wallet description".to_string());

    wallet.update(name.clone(), description.clone()).unwrap();

    let wallet_data = wallet.public_data().unwrap();

    assert_eq!(name, wallet_data.name);
    assert_eq!(description, wallet_data.description);
    assert_eq!(name, db.get_opt(&keys::wallet_name()).unwrap());
    assert_eq!(
        description,
        db.get_opt(&keys::wallet_description()).unwrap()
    );
}

#[test]
fn test_get_transaction() {
    let (wallet, _db) = factories::wallet(None);

    let a_block = factories::BlockInfo::default().create();
    let our_address = wallet.gen_external_address(None).unwrap();
    let their_pkh = factories::pkh();
    let fee = Fee::default();

    assert!(wallet.get_transaction(0, 0).is_err());
    // index transaction to receive funds
    wallet
        .index_block_transactions(
            &a_block,
            &[factories::vtt_from_body(VTTransactionBody::new(
                vec![Input::default()],
                vec![ValueTransferOutput {
                    pkh: our_address.pkh,
                    value: 2,
                    time_lock: 0,
                }],
            ))],
            true,
        )
        .unwrap();

    assert_eq!(1, wallet.state.read().unwrap().utxo_set.len());

    assert!(wallet.get_transaction(0, 0).is_ok());
    assert!(wallet.get_transaction(0, 1).is_err());

    assert_eq!(2, wallet.balance().unwrap().unconfirmed.available);
    assert_eq!(0, wallet.balance().unwrap().unconfirmed.locked);

    // spend those funds to create a new transaction which is pending (it has no block)
    let (extended, ..) = wallet
        .create_vtt(types::VttParams {
            fee,
            outputs: vec![ValueTransferOutput {
                pkh: their_pkh,
                value: 1,
                time_lock: 0,
            }],
            utxo_strategy: UtxoSelectionStrategy::Random { from: None },
            selected_utxos: HashSet::default(),
        })
        .unwrap();

    // the extended transaction should actually contain a value transfer transaction
    let vtt = if let Transaction::ValueTransfer(vtt) = extended.transaction {
        Some(vtt)
    } else {
        panic!("the extended transaction should actually contain a value transfer transaction, got: {:?}", extended.transaction);
    }
    .unwrap();

    // There is a signature for each input
    assert_eq!(vtt.body.inputs.len(), vtt.signatures.len());

    // the wallet does not store created VTT transactions until confirmation
    assert!(wallet.get_transaction(0, 1).is_err());

    // index another block confirming the previously created vtt
    wallet
        .index_block_transactions(&a_block, &[factories::vtt_from_body(vtt.body)], true)
        .unwrap();
    assert!(wallet.get_transaction(0, 1).is_ok());
}

#[test]
fn test_get_transactions() {
    let (wallet, _db) = factories::wallet(None);

    let no_transactions = crate::model::WalletTransactions {
        transactions: vec![],
        total: 0,
    };
    assert_eq!(wallet.transactions(0, 0).unwrap(), no_transactions);
    assert_eq!(wallet.transactions(0, 1).unwrap(), no_transactions);
    assert_eq!(wallet.transactions(1, 0).unwrap(), no_transactions);
    assert_eq!(wallet.transactions(1, 1).unwrap(), no_transactions);

    let a_block = factories::BlockInfo::default().create();
    let our_address = wallet.gen_external_address(None).unwrap();
    let their_pkh = factories::pkh();
    let fee = Fee::default();

    // index transaction to receive funds
    wallet
        .index_block_transactions(
            &a_block,
            &[factories::vtt_from_body(VTTransactionBody::new(
                vec![Input::default()],
                vec![ValueTransferOutput {
                    pkh: our_address.pkh,
                    value: 2,
                    time_lock: 0,
                }],
            ))],
            true,
        )
        .unwrap();

    // The total returned by wallet.transactions() will now always be 1, regardless of limit
    let no_transactions = crate::model::WalletTransactions {
        transactions: vec![],
        total: 1,
    };
    assert_eq!(wallet.transactions(0, 0).unwrap(), no_transactions);
    let x = wallet.transactions(0, 1).unwrap();
    assert_eq!(x.transactions.len(), 1);
    assert_eq!(x.total, 1);
    let first_tx = x.transactions[0].clone();
    assert_eq!(wallet.transactions(1, 0).unwrap(), no_transactions);
    assert_eq!(wallet.transactions(1, 1).unwrap(), no_transactions);

    // spend those funds to create a new transaction which is pending (it has no block)
    let (extended, ..) = wallet
        .create_vtt(types::VttParams {
            fee,
            outputs: vec![ValueTransferOutput {
                pkh: their_pkh,
                value: 1,
                time_lock: 0,
            }],
            utxo_strategy: UtxoSelectionStrategy::Random { from: None },
            selected_utxos: HashSet::default(),
        })
        .unwrap();

    // the extended transaction should actually contain a value transfer transaction
    let vtt = if let Transaction::ValueTransfer(vtt) = extended.transaction {
        Some(vtt)
    } else {
        panic!("the extended transaction should actually contain a value transfer transaction, got: {:?}", extended.transaction);
    }
    .unwrap();

    // There is a signature for each input
    assert_eq!(vtt.body.inputs.len(), vtt.signatures.len());

    // the wallet does not store created VTT transactions until confirmation
    let x = wallet.transactions(0, 1).unwrap();
    assert_eq!(x.transactions.len(), 1);
    assert_eq!(x.total, 1);

    // index another block confirming the previously created vtt
    wallet
        .index_block_transactions(&a_block, &[factories::vtt_from_body(vtt.body)], true)
        .unwrap();
    let x = wallet.transactions(0, 2).unwrap();
    assert_eq!(x.transactions.len(), 2);
    assert_eq!(x.total, 2);
    // The older transaction has index 1 now
    assert_eq!(x.transactions[1], first_tx);

    let x = wallet.transactions(1, 2).unwrap();
    assert_eq!(x.transactions.len(), 1);
    assert_eq!(x.total, 2);
    // The older transaction has index 0 now, because we used offset 1
    assert_eq!(x.transactions[0], first_tx);
}

#[test]
fn test_create_vtt_with_locked_balance() {
    let (wallet, _db) = factories::wallet(None);

    let a_block = factories::BlockInfo::default().create();
    let our_address = wallet.gen_external_address(None).unwrap();
    let their_pkh = factories::pkh();
    let fee = Fee::default();

    assert!(wallet.get_transaction(0, 0).is_err());
    // index transaction to receive funds
    wallet
        .index_block_transactions(
            &a_block,
            &[factories::vtt_from_body(VTTransactionBody::new(
                vec![Input::default()],
                vec![ValueTransferOutput {
                    pkh: our_address.pkh,
                    value: 2,
                    time_lock: u64::MAX,
                }],
            ))],
            true,
        )
        .unwrap();

    assert_eq!(1, wallet.state.read().unwrap().utxo_set.len());

    assert!(wallet.get_transaction(0, 0).is_ok());
    assert!(wallet.get_transaction(0, 1).is_err());

    assert_eq!(0, wallet.balance().unwrap().unconfirmed.available);
    assert_eq!(2, wallet.balance().unwrap().unconfirmed.locked);

    // try to spend locked funds to create a new transaction
    let err = wallet
        .create_vtt(types::VttParams {
            fee,
            outputs: vec![ValueTransferOutput {
                pkh: their_pkh,
                value: 1,
                time_lock: 0,
            }],
            utxo_strategy: UtxoSelectionStrategy::Random { from: None },
            selected_utxos: HashSet::default(),
        })
        .unwrap_err();

    assert!(
        matches!(err, repository::Error::InsufficientBalance { .. }),
        "{:?}",
        err
    );
}

#[test]
fn test_create_vtt_with_multiple_outputs() {
    let (wallet, _db) = factories::wallet(None);

    let a_block = factories::BlockInfo::default().create();
    let our_address = wallet.gen_external_address(None).unwrap();
    let fee = Fee::default();

    assert!(wallet.get_transaction(0, 0).is_err());
    // index transaction to receive funds
    wallet
        .index_block_transactions(
            &a_block,
            &[factories::vtt_from_body(VTTransactionBody::new(
                vec![Input::default()],
                vec![ValueTransferOutput {
                    pkh: our_address.pkh,
                    value: 2,
                    time_lock: 0,
                }],
            ))],
            true,
        )
        .unwrap();

    assert_eq!(1, wallet.state.read().unwrap().utxo_set.len());

    assert!(wallet.get_transaction(0, 0).is_ok());
    assert!(wallet.get_transaction(0, 1).is_err());

    assert_eq!(2, wallet.balance().unwrap().unconfirmed.available);
    assert_eq!(0, wallet.balance().unwrap().unconfirmed.locked);

    // create wallet with 2 multiple outputs
    let their_pkh1 = factories::pkh();
    let their_pkh2 = factories::pkh();
    let (extended, ..) = wallet
        .create_vtt(types::VttParams {
            fee,
            outputs: vec![
                ValueTransferOutput {
                    pkh: their_pkh1,
                    value: 1,
                    time_lock: 0,
                },
                ValueTransferOutput {
                    pkh: their_pkh2,
                    value: 1,
                    time_lock: 0,
                },
            ],
            utxo_strategy: UtxoSelectionStrategy::Random { from: None },
            selected_utxos: HashSet::default(),
        })
        .unwrap();

    // the extended transaction should actually contain a value transfer transaction
    let vtt = if let Transaction::ValueTransfer(vtt) = extended.transaction {
        Some(vtt)
    } else {
        panic!("the extended transaction should actually contain a value transfer transaction, got: {:?}", extended.transaction);
    }
    .unwrap();

    // There is a signature for each input
    assert_eq!(vtt.body.inputs.len(), vtt.signatures.len());

    // There 2 outputs
    assert_eq!(vtt.body.outputs.len(), 2);
}

#[test]
fn test_export_xprv_key() {
    let (wallet, _db) = factories::wallet(None);

    let password: types::Password = "password".to_string().into();
    assert!(wallet
        .export_master_key(password.clone())
        .unwrap()
        .starts_with("xprv"));
    assert!(!wallet
        .export_master_key(password)
        .unwrap()
        .starts_with("xprvdouble"));
}

#[test]
fn test_export_xprvdouble_key() {
    // Create a wallet that does not store the master key.
    // This is used to emulate a bug in previous versions of the wallet.
    // In that case, the exported master key format is not "xprv", it is "xprvdouble"
    let (wallet, _db) = factories::wallet_with_args(None, false);

    let password = "password".to_string().into();
    assert!(wallet
        .export_master_key(password)
        .unwrap()
        .starts_with("xprvdouble"));
}

#[test]
fn test_create_vt_components_weighted_fee() {
    let pkh = factories::pkh();
    let out_pointer = model::OutPtr {
        txn_hash: vec![0; 32],
        output_index: 0,
    };
    let utxo_set: HashMap<model::OutPtr, model::OutputInfo> = HashMap::from_iter(vec![(
        out_pointer,
        model::OutputInfo {
            pkh,
            amount: 20000,
            time_lock: 0,
        },
    )]);
    let path = model::Path {
        account: 0,
        keychain: constants::EXTERNAL_KEYCHAIN,
        index: 0,
    };
    let new_balance = model::BalanceInfo {
        available: 20000u64,
        locked: 0u64,
    };

    let db = HashMapDb::default();
    db.put(&keys::account_utxo_set(0), utxo_set).unwrap();
    db.put(&keys::account_balance(0), new_balance).unwrap();
    db.put(&keys::pkh(&pkh), path).unwrap();
    let (wallet, _db) = factories::wallet(Some(db));
    let mut state = wallet.state.write().unwrap();
    let pkh = factories::pkh();
    let value = 1;
    let fee = Fee::relative_from_float(1.0);
    let time_lock = 0;
    let utxo_strategy = UtxoSelectionStrategy::Random { from: None };
    let vto = ValueTransferOutput {
        pkh,
        value,
        time_lock,
    };
    let TransactionComponents {
        inputs, outputs, ..
    } = wallet
        .create_vt_transaction_components(
            &mut state,
            vec![vto],
            fee,
            &utxo_strategy,
            HashSet::default(),
        )
        .unwrap();

    assert_eq!(1, inputs.pointers.len());
    assert_eq!(1, inputs.resolved.len());
    assert_eq!(2, outputs.len());
}

#[test]
fn test_create_vt_components_weighted_fee_2() {
    let pkh = factories::pkh();
    let out_pointer = model::OutPtr {
        txn_hash: vec![0; 32],
        output_index: 0,
    };
    let mut out_pointer_1 = out_pointer.clone();
    out_pointer_1.output_index = 1;
    let utxo_set: HashMap<model::OutPtr, model::OutputInfo> = HashMap::from_iter(vec![
        (
            out_pointer,
            model::OutputInfo {
                pkh,
                amount: 800,
                time_lock: 0,
            },
        ),
        (
            out_pointer_1,
            model::OutputInfo {
                pkh,
                amount: 2000,
                time_lock: 0,
            },
        ),
    ]);
    let path = model::Path {
        account: 0,
        keychain: constants::EXTERNAL_KEYCHAIN,
        index: 0,
    };

    let new_balance = model::BalanceInfo {
        available: 2800u64,
        locked: 0u64,
    };

    let db = HashMapDb::default();
    db.put(&keys::account_utxo_set(0), utxo_set).unwrap();
    db.put(&keys::account_balance(0), new_balance).unwrap();
    db.put(&keys::pkh(&pkh), path).unwrap();
    let (wallet, _db) = factories::wallet(Some(db));

    let mut state = wallet.state.write().unwrap();
    let pkh = factories::pkh();
    let value = 1;
    let fee = Fee::relative_from_float(1.0);
    let time_lock = 0;
    let utxo_strategy = UtxoSelectionStrategy::Random { from: None };
    let vto = ValueTransferOutput {
        pkh,
        value,
        time_lock,
    };
    let TransactionComponents {
        inputs, outputs, ..
    } = wallet
        .create_vt_transaction_components(
            &mut state,
            vec![vto],
            fee,
            &utxo_strategy,
            HashSet::default(),
        )
        .unwrap();

    assert!(!inputs.pointers.is_empty());
    assert!(!inputs.resolved.is_empty());
    assert_eq!(2, outputs.len());
}

#[test]
fn test_create_vt_components_weighted_fee_3() {
    let pkh = factories::pkh();
    let out_pointer = model::OutPtr {
        txn_hash: vec![0; 32],
        output_index: 0,
    };
    let mut out_pointer_1 = out_pointer.clone();
    out_pointer_1.output_index = 1;
    let utxo_set: HashMap<model::OutPtr, model::OutputInfo> = HashMap::from_iter(vec![
        (
            out_pointer,
            model::OutputInfo {
                pkh,
                amount: 800,
                time_lock: 0,
            },
        ),
        (
            out_pointer_1,
            model::OutputInfo {
                pkh,
                amount: 1,
                time_lock: 0,
            },
        ),
    ]);
    let path = model::Path {
        account: 0,
        keychain: constants::EXTERNAL_KEYCHAIN,
        index: 0,
    };
    let new_balance = model::BalanceInfo {
        available: 801u64,
        locked: 0u64,
    };
    let db = HashMapDb::default();
    db.put(&keys::account_utxo_set(0), utxo_set).unwrap();
    db.put(&keys::account_balance(0), new_balance).unwrap();
    db.put(&keys::pkh(&pkh), path).unwrap();
    let (wallet, _db) = factories::wallet(Some(db));
    let mut state = wallet.state.write().unwrap();
    let pkh = factories::pkh();
    let value = 1;
    let fee = Fee::relative_from_float(1.0);
    let time_lock = 0;
    let utxo_strategy = UtxoSelectionStrategy::Random { from: None };
    let vto = ValueTransferOutput {
        pkh,
        value,
        time_lock,
    };
    let err = wallet
        .create_vt_transaction_components(
            &mut state,
            vec![vto],
            fee,
            &utxo_strategy,
            HashSet::default(),
        )
        .unwrap_err();

    assert!(
        matches!(err, repository::Error::InsufficientBalance { .. }),
        "{:?}",
        err
    );
}

#[test]
fn test_create_vt_components_weighted_fee_4() {
    let pkh = factories::pkh();
    let out_pointer = model::OutPtr {
        txn_hash: vec![0; 32],
        output_index: 0,
    };
    let mut out_pointer_1 = out_pointer.clone();
    out_pointer_1.output_index = 1;
    let mut out_pointer_2 = out_pointer.clone();
    out_pointer_2.output_index = 2;
    let mut out_pointer_3 = out_pointer.clone();
    out_pointer_3.output_index = 3;
    let utxo_set: HashMap<model::OutPtr, model::OutputInfo> = HashMap::from_iter(vec![
        (
            out_pointer,
            model::OutputInfo {
                pkh,
                amount: 1,
                time_lock: 0,
            },
        ),
        (
            out_pointer_1,
            model::OutputInfo {
                pkh,
                amount: 1,
                time_lock: 0,
            },
        ),
        (
            out_pointer_2,
            model::OutputInfo {
                pkh,
                amount: 1,
                time_lock: 0,
            },
        ),
        (
            out_pointer_3,
            model::OutputInfo {
                pkh,
                amount: 70000,
                time_lock: 0,
            },
        ),
    ]);
    let path = model::Path {
        account: 0,
        keychain: constants::EXTERNAL_KEYCHAIN,
        index: 0,
    };
    let new_balance = model::BalanceInfo {
        available: 70003u64,
        locked: 0u64,
    };

    let db = HashMapDb::default();
    db.put(&keys::account_utxo_set(0), utxo_set).unwrap();
    db.put(&keys::account_balance(0), new_balance).unwrap();
    db.put(&keys::pkh(&pkh), path).unwrap();
    let (wallet, _db) = factories::wallet(Some(db));
    let mut state = wallet.state.write().unwrap();
    let pkh = factories::pkh();
    let value = 1;
    let fee = Fee::relative_from_float(1.0);
    let time_lock = 0;
    let utxo_strategy = UtxoSelectionStrategy::Random { from: None };
    let vto = ValueTransferOutput {
        pkh,
        value,
        time_lock,
    };
    let TransactionComponents {
        inputs, outputs, ..
    } = wallet
        .create_vt_transaction_components(
            &mut state,
            vec![vto],
            fee,
            &utxo_strategy,
            HashSet::default(),
        )
        .unwrap();

    assert!(!inputs.pointers.is_empty());
    assert!(!inputs.resolved.is_empty());
    assert_eq!(2, outputs.len());
}

#[test]
fn test_create_vt_components_weighted_fee_5() {
    let pkh = factories::pkh();
    let out_pointer = model::OutPtr {
        txn_hash: vec![0; 32],
        output_index: 0,
    };
    let mut out_pointer_1 = out_pointer.clone();
    out_pointer_1.output_index = 1;
    let mut out_pointer_2 = out_pointer.clone();
    out_pointer_2.output_index = 2;
    let mut out_pointer_3 = out_pointer.clone();
    out_pointer_3.output_index = 3;
    let utxo_set: HashMap<model::OutPtr, model::OutputInfo> = HashMap::from_iter(vec![
        (
            out_pointer,
            model::OutputInfo {
                pkh,
                amount: 1300,
                time_lock: 0,
            },
        ),
        (
            out_pointer_1,
            model::OutputInfo {
                pkh,
                amount: 800,
                time_lock: 0,
            },
        ),
        (
            out_pointer_2,
            model::OutputInfo {
                pkh,
                amount: 800,
                time_lock: 0,
            },
        ),
        (
            out_pointer_3,
            model::OutputInfo {
                pkh,
                amount: 800,
                time_lock: 0,
            },
        ),
    ]);
    let path = model::Path {
        account: 0,
        keychain: constants::EXTERNAL_KEYCHAIN,
        index: 0,
    };
    let new_balance = model::BalanceInfo {
        available: 3700u64,
        locked: 0u64,
    };
    let db = HashMapDb::default();
    db.put(&keys::account_utxo_set(0), utxo_set).unwrap();
    db.put(&keys::account_balance(0), new_balance).unwrap();
    db.put(&keys::pkh(&pkh), path).unwrap();
    let (wallet, _db) = factories::wallet(Some(db));
    let mut state = wallet.state.write().unwrap();
    let pkh = factories::pkh();
    let value = 1;
    let fee = Fee::relative_from_float(1.0);
    let time_lock = 0;
    let utxo_strategy = UtxoSelectionStrategy::Random { from: None };
    let vto = ValueTransferOutput {
        pkh,
        value,
        time_lock,
    };
    let TransactionComponents {
        inputs, outputs, ..
    } = wallet
        .create_vt_transaction_components(
            &mut state,
            vec![vto],
            fee,
            &utxo_strategy,
            HashSet::default(),
        )
        .unwrap();

    assert!(!inputs.pointers.is_empty());
    assert!(!inputs.resolved.is_empty());
    assert_eq!(2, outputs.len());
}

#[test]
fn test_create_vt_components_weighted_fee_6() {
    let pkh = factories::pkh();
    let out_pointer = model::OutPtr {
        txn_hash: vec![0; 32],
        output_index: 0,
    };
    let mut out_pointer_1 = out_pointer.clone();
    out_pointer_1.output_index = 1;
    out_pointer_1.txn_hash = vec![1; 32];

    let mut out_pointer_2 = out_pointer.clone();
    out_pointer_2.output_index = 2;
    out_pointer_2.txn_hash = vec![2; 32];

    let mut out_pointer_3 = out_pointer.clone();
    out_pointer_3.output_index = 3;
    out_pointer_3.txn_hash = vec![3; 32];

    let utxo_set: HashMap<model::OutPtr, model::OutputInfo> = HashMap::from_iter(vec![
        (
            out_pointer,
            model::OutputInfo {
                pkh,
                amount: 400,
                time_lock: 0,
            },
        ),
        (
            out_pointer_1,
            model::OutputInfo {
                pkh,
                amount: 50,
                time_lock: 0,
            },
        ),
        (
            out_pointer_2,
            model::OutputInfo {
                pkh,
                amount: 50,
                time_lock: 0,
            },
        ),
        (
            out_pointer_3,
            model::OutputInfo {
                pkh,
                amount: 800,
                time_lock: 0,
            },
        ),
    ]);
    let path = model::Path {
        account: 0,
        keychain: constants::EXTERNAL_KEYCHAIN,
        index: 0,
    };
    let new_balance = model::BalanceInfo {
        available: 1300u64,
        locked: 0u64,
    };
    let db = HashMapDb::default();
    db.put(&keys::account_utxo_set(0), utxo_set).unwrap();
    db.put(&keys::account_balance(0), new_balance).unwrap();
    db.put(&keys::pkh(&pkh), path).unwrap();
    let (wallet, _db) = factories::wallet(Some(db));
    let mut state = wallet.state.write().unwrap();
    let pkh = factories::pkh();
    let value = 1;
    let fee = Fee::relative_from_float(1.0);
    let time_lock = 0;
    let utxo_strategy = UtxoSelectionStrategy::Random { from: None };
    let vto = ValueTransferOutput {
        pkh,
        value,
        time_lock,
    };
    let TransactionComponents {
        inputs, outputs, ..
    } = wallet
        .create_vt_transaction_components(
            &mut state,
            vec![vto],
            fee,
            &utxo_strategy,
            HashSet::default(),
        )
        .unwrap();

    assert!(inputs.pointers.len() >= 2);
    assert!(inputs.resolved.len() >= 2);
    assert_eq!(2, outputs.len());
}

#[test]
fn test_create_vt_components_weighted_fee_without_outputs() {
    let pkh = factories::pkh();
    let utxo_set: HashMap<model::OutPtr, model::OutputInfo> = HashMap::from_iter(vec![]);
    let path = model::Path {
        account: 0,
        keychain: constants::EXTERNAL_KEYCHAIN,
        index: 0,
    };
    let new_balance = model::BalanceInfo {
        available: 0u64,
        locked: 0u64,
    };
    let db = HashMapDb::default();
    db.put(&keys::account_utxo_set(0), utxo_set).unwrap();
    db.put(&keys::account_balance(0), new_balance).unwrap();
    db.put(&keys::pkh(&pkh), path).unwrap();
    let (wallet, _db) = factories::wallet(Some(db));
    let mut state = wallet.state.write().unwrap();
    let pkh = factories::pkh();
    let value = 1;
    let fee = Fee::relative_from_float(1.0);
    let time_lock = 0;
    let utxo_strategy = UtxoSelectionStrategy::Random { from: None };
    let vto = ValueTransferOutput {
        pkh,
        value,
        time_lock,
    };
    let err = wallet
        .create_vt_transaction_components(
            &mut state,
            vec![vto],
            fee,
            &utxo_strategy,
            HashSet::default(),
        )
        .unwrap_err();

    assert!(
        matches!(err, repository::Error::InsufficientBalance { .. }),
        "{:?}",
        err
    );
}

#[test]
fn test_create_vt_components_weighted_fee_with_too_large_fee() {
    let pkh = factories::pkh();
    let out_pointer = model::OutPtr {
        txn_hash: vec![0; 32],
        output_index: 0,
    };
    let utxo_set: HashMap<model::OutPtr, model::OutputInfo> = HashMap::from_iter(vec![(
        out_pointer,
        model::OutputInfo {
            pkh,
            amount: 1,
            time_lock: 0,
        },
    )]);
    let path = model::Path {
        account: 0,
        keychain: constants::EXTERNAL_KEYCHAIN,
        index: 0,
    };
    let new_balance = model::BalanceInfo {
        available: 1u64,
        locked: 0u64,
    };
    let db = HashMapDb::default();
    db.put(&keys::account_utxo_set(0), utxo_set).unwrap();
    db.put(&keys::account_balance(0), new_balance).unwrap();
    db.put(&keys::pkh(&pkh), path).unwrap();
    let (wallet, _db) = factories::wallet(Some(db));
    let mut state = wallet.state.write().unwrap();
    let pkh = factories::pkh();
    let value = 1;
    let fee = Fee::relative_from_float(f64::MAX);
    let time_lock = 0;
    let utxo_strategy = UtxoSelectionStrategy::Random { from: None };
    let vto = ValueTransferOutput {
        pkh,
        value,
        time_lock,
    };
    let err = wallet
        .create_vt_transaction_components(
            &mut state,
            vec![vto],
            fee,
            &utxo_strategy,
            HashSet::default(),
        )
        .unwrap_err();

    assert_eq!(
        mem::discriminant(&repository::Error::FeeTooLarge),
        mem::discriminant(&err),
        "{:?}",
        err,
    );
}
#[test]
fn test_create_vt_weight_too_large() {
    let pkh = factories::pkh();
    let mut output_vec: Vec<(model::OutPtr, model::OutputInfo)> = vec![];
    for index in 0u8..200u8 {
        let out_pointer = model::OutPtr {
            txn_hash: vec![index; 32],
            output_index: u32::from(index),
        };
        output_vec.push((
            out_pointer,
            model::OutputInfo {
                pkh,
                amount: 1,
                time_lock: 0,
            },
        ));
    }

    let utxo_set: HashMap<model::OutPtr, model::OutputInfo> = HashMap::from_iter(output_vec);
    let path = model::Path {
        account: 0,
        keychain: constants::EXTERNAL_KEYCHAIN,
        index: 0,
    };
    let new_balance = model::BalanceInfo {
        available: 200u64,
        locked: 0u64,
    };
    let db = HashMapDb::default();
    db.put(&keys::account_utxo_set(0), utxo_set).unwrap();
    db.put(&keys::account_balance(0), new_balance).unwrap();
    db.put(&keys::pkh(&pkh), path).unwrap();
    let (wallet, _db) = factories::wallet(Some(db));
    let mut state = wallet.state.write().unwrap();
    let pkh = factories::pkh();
    let value = 150;
    let fee = Fee::relative_from_float(0.0);
    let time_lock = 0;
    let utxo_strategy = UtxoSelectionStrategy::Random { from: None };
    let vto = ValueTransferOutput {
        pkh,
        value,
        time_lock,
    };
    let err = wallet
        .create_vt_transaction_components(
            &mut state,
            vec![vto],
            fee,
            &utxo_strategy,
            HashSet::default(),
        )
        .unwrap_err();

    assert_eq!(
        mem::discriminant(&repository::Error::MaximumVTTWeightReached(value)),
        mem::discriminant(&err),
        "{:?}",
        err,
    );
}

#[test]
fn test_create_dr_components_weighted_fee_1() {
    let pkh = factories::pkh();
    let out_pointer = model::OutPtr {
        txn_hash: vec![0; 32],
        output_index: 0,
    };

    let utxo_set: HashMap<model::OutPtr, model::OutputInfo> = HashMap::from_iter(vec![(
        out_pointer,
        model::OutputInfo {
            pkh,
            amount: 2000,
            time_lock: 0,
        },
    )]);
    let path = model::Path {
        account: 0,
        keychain: constants::EXTERNAL_KEYCHAIN,
        index: 0,
    };
    let new_balance = model::BalanceInfo {
        available: 2000u64,
        locked: 0u64,
    };
    let db = HashMapDb::default();
    db.put(&keys::account_utxo_set(0), utxo_set).unwrap();
    db.put(&keys::account_balance(0), new_balance).unwrap();
    db.put(&keys::pkh(&pkh), path).unwrap();
    let (wallet, _db) = factories::wallet(Some(db));

    let request = DataRequestOutput {
        witness_reward: 1,
        witnesses: 1,
        ..DataRequestOutput::default()
    };

    let mut state = wallet.state.write().unwrap();
    let fee = Fee::relative_from_float(1.0);
    let TransactionComponents { inputs, .. } = wallet
        .create_dr_transaction_components(&mut state, request, fee)
        .unwrap();

    assert_eq!(inputs.pointers.len(), 1);
    assert_eq!(inputs.resolved.len(), 1);
}

#[test]
fn test_create_dr_components_weighted_fee_2_not_enough_funds() {
    let pkh = factories::pkh();
    let out_pointer = model::OutPtr {
        txn_hash: vec![0; 32],
        output_index: 0,
    };

    let utxo_set: HashMap<model::OutPtr, model::OutputInfo> = HashMap::from_iter(vec![(
        out_pointer,
        model::OutputInfo {
            pkh,
            amount: 2,
            time_lock: 0,
        },
    )]);
    let path = model::Path {
        account: 0,
        keychain: constants::EXTERNAL_KEYCHAIN,
        index: 0,
    };
    let new_balance = model::BalanceInfo {
        available: 2u64,
        locked: 0u64,
    };
    let db = HashMapDb::default();
    db.put(&keys::account_utxo_set(0), utxo_set).unwrap();
    db.put(&keys::account_balance(0), new_balance).unwrap();
    db.put(&keys::pkh(&pkh), path).unwrap();
    let (wallet, _db) = factories::wallet(Some(db));

    let request = DataRequestOutput {
        witness_reward: 1,
        witnesses: 1,
        ..DataRequestOutput::default()
    };

    let mut state = wallet.state.write().unwrap();
    let fee = Fee::relative_from_float(1.0);
    let err = wallet
        .create_dr_transaction_components(&mut state, request, fee)
        .unwrap_err();

    assert!(
        matches!(err, repository::Error::InsufficientBalance { .. }),
        "{:?}",
        err
    );
}

#[test]
fn test_create_dr_components_weighted_fee_3_funds_splitted() {
    let pkh = factories::pkh();
    let out_pointer = model::OutPtr {
        txn_hash: vec![0; 32],
        output_index: 0,
    };

    let utxo_set: HashMap<model::OutPtr, model::OutputInfo> = HashMap::from_iter(vec![(
        out_pointer.clone(),
        model::OutputInfo {
            pkh,
            amount: 2000,
            time_lock: 0,
        },
    )]);
    let path = model::Path {
        account: 0,
        keychain: constants::EXTERNAL_KEYCHAIN,
        index: 0,
    };
    let new_balance = model::BalanceInfo {
        available: 2000u64,
        locked: 0u64,
    };
    let db = HashMapDb::default();
    db.put(&keys::account_utxo_set(0), utxo_set).unwrap();
    db.put(&keys::account_balance(0), new_balance).unwrap();
    db.put(&keys::pkh(&pkh), &path).unwrap();
    let (wallet, _db) = factories::wallet(Some(db));

    let request = DataRequestOutput {
        witness_reward: 1,
        witnesses: 1,
        ..DataRequestOutput::default()
    };

    let mut state = wallet.state.write().unwrap();

    let fee = Fee::relative_from_float(1.0);
    let TransactionComponents { inputs, .. } = wallet
        .create_dr_transaction_components(&mut state, request.clone(), fee)
        .unwrap();
    let weight = calculate_weight(inputs.pointers.len(), 1, Some(&request), u32::MAX).unwrap();

    let mut out_pointer_1 = out_pointer.clone();
    out_pointer_1.output_index = 1;
    out_pointer_1.txn_hash = vec![1; 32];

    let mut out_pointer_2 = out_pointer.clone();
    out_pointer_2.output_index = 2;
    out_pointer_2.txn_hash = vec![2; 32];

    let mut out_pointer_3 = out_pointer;
    out_pointer_3.output_index = 3;
    out_pointer_3.txn_hash = vec![3; 32];

    let utxo_set_2: HashMap<model::OutPtr, model::OutputInfo> = HashMap::from_iter(vec![
        (
            out_pointer_1,
            model::OutputInfo {
                pkh,
                amount: u64::from(weight) / 2,
                time_lock: 0,
            },
        ),
        (
            out_pointer_2,
            model::OutputInfo {
                pkh,
                amount: u64::from(weight) / 2,
                time_lock: 0,
            },
        ),
        (
            out_pointer_3,
            model::OutputInfo {
                pkh,
                amount: u64::from(weight) / 2,
                time_lock: 0,
            },
        ),
    ]);

    let new_balance_2 = model::BalanceInfo {
        available: u64::from(weight) * 3 / 2,
        locked: 0u64,
    };

    let db_2 = HashMapDb::default();
    db_2.put(&keys::account_utxo_set(0), utxo_set_2).unwrap();
    db_2.put(&keys::account_balance(0), new_balance_2).unwrap();
    db_2.put(&keys::pkh(&pkh), path).unwrap();

    let (wallet_2, _db) = factories::wallet(Some(db_2));
    let mut state_2 = wallet_2.state.write().unwrap();

    let TransactionComponents { inputs, .. } = wallet_2
        .create_dr_transaction_components(&mut state_2, request, fee)
        .unwrap();

    assert_eq!(inputs.pointers.len(), 3);
    assert_eq!(inputs.resolved.len(), 3);
}

#[test]
fn test_create_dr_components_weighted_fee_without_outputs() {
    let pkh = factories::pkh();
    let utxo_set: HashMap<model::OutPtr, model::OutputInfo> = HashMap::from_iter(vec![]);
    let path = model::Path {
        account: 0,
        keychain: constants::EXTERNAL_KEYCHAIN,
        index: 0,
    };
    let new_balance = model::BalanceInfo {
        available: 0u64,
        locked: 0u64,
    };
    let db = HashMapDb::default();
    db.put(&keys::account_utxo_set(0), utxo_set).unwrap();
    db.put(&keys::account_balance(0), new_balance).unwrap();
    db.put(&keys::pkh(&pkh), path).unwrap();
    let (wallet, _db) = factories::wallet(Some(db));

    let mut state = wallet.state.write().unwrap();
    let request = DataRequestOutput {
        witness_reward: 1,
        witnesses: 1,
        ..DataRequestOutput::default()
    };

    let fee = Fee::relative_from_float(1.0);
    let err = wallet
        .create_dr_transaction_components(&mut state, request, fee)
        .unwrap_err();

    assert!(
        matches!(err, repository::Error::InsufficientBalance { .. }),
        "{:?}",
        err
    );
}

#[test]
fn test_create_dr_components_weighted_fee_weight_too_large() {
    let pkh = factories::pkh();
    let mut output_vec: Vec<(model::OutPtr, model::OutputInfo)> = vec![];
    for index in 0u32..1000u32 {
        let out_pointer = model::OutPtr {
            txn_hash: vec![0; 32],
            output_index: index,
        };
        output_vec.push((
            out_pointer,
            model::OutputInfo {
                pkh,
                amount: 1,
                time_lock: 0,
            },
        ));
    }

    let utxo_set: HashMap<model::OutPtr, model::OutputInfo> = HashMap::from_iter(output_vec);
    let path = model::Path {
        account: 0,
        keychain: constants::EXTERNAL_KEYCHAIN,
        index: 0,
    };
    let new_balance = model::BalanceInfo {
        available: 1000u64,
        locked: 0u64,
    };
    let db = HashMapDb::default();
    db.put(&keys::account_utxo_set(0), utxo_set).unwrap();
    db.put(&keys::account_balance(0), new_balance).unwrap();
    db.put(&keys::pkh(&pkh), path).unwrap();

    let (wallet, _db) = factories::wallet(Some(db));
    let mut state = wallet.state.write().unwrap();
    let request = DataRequestOutput {
        witness_reward: 0,
        witnesses: 1000,
        ..DataRequestOutput::default()
    };
    let fee = Fee::relative_from_float(0);
    let err = wallet
        .create_dr_transaction_components(&mut state, request.clone(), fee)
        .unwrap_err();

    assert_eq!(
        mem::discriminant(&repository::Error::MaximumDRWeightReached(Box::new(
            request
        ))),
        mem::discriminant(&err),
        "{:?}",
        err,
    );
}

#[test]
fn test_create_dr_components_weighted_fee_fee_too_large() {
    let pkh = factories::pkh();
    let out_pointer = model::OutPtr {
        txn_hash: vec![0; 32],
        output_index: 0,
    };

    let utxo_set: HashMap<model::OutPtr, model::OutputInfo> = HashMap::from_iter(vec![(
        out_pointer,
        model::OutputInfo {
            pkh,
            amount: 2000,
            time_lock: 0,
        },
    )]);
    let path = model::Path {
        account: 0,
        keychain: constants::EXTERNAL_KEYCHAIN,
        index: 0,
    };
    let new_balance = model::BalanceInfo {
        available: 2000u64,
        locked: 0u64,
    };
    let db = HashMapDb::default();
    db.put(&keys::account_utxo_set(0), utxo_set).unwrap();
    db.put(&keys::account_balance(0), new_balance).unwrap();
    db.put(&keys::pkh(&pkh), path).unwrap();
    let (wallet, _db) = factories::wallet(Some(db));

    let request = DataRequestOutput {
        witness_reward: 1,
        witnesses: 1,
        ..DataRequestOutput::default()
    };

    let mut state = wallet.state.write().unwrap();

    let fee = Fee::relative_from_float(f64::MAX / 2.0);
    let err = wallet
        .create_dr_transaction_components(&mut state, request, fee)
        .unwrap_err();

    assert_eq!(
        mem::discriminant(&repository::Error::FeeTooLarge),
        mem::discriminant(&err),
        "{:?}",
        err,
    );
}

#[test]
fn test_create_transaction_components_filter_from_address() {
    // Create UTXO set with 2 UTXOs from different addresses
    let pkh1 = factories::pkh();
    let pkh2 = factories::pkh();
    assert_ne!(pkh1, pkh2);
    let mut utxo_set: HashMap<model::OutPtr, model::OutputInfo> = HashMap::new();

    for i in 0..100 {
        // 50 UTXOs with pkh1 and 50 UTXOs with pkh2
        let pkh = if i < 50 { pkh1 } else { pkh2 };
        utxo_set.insert(
            model::OutPtr {
                txn_hash: vec![0; 32],
                output_index: i,
            },
            model::OutputInfo {
                pkh,
                amount: 1,
                time_lock: 0,
            },
        );
    }

    let path1 = model::Path {
        account: 0,
        keychain: constants::EXTERNAL_KEYCHAIN,
        index: 0,
    };
    let path2 = model::Path {
        account: 0,
        keychain: constants::EXTERNAL_KEYCHAIN,
        index: 1,
    };
    let new_balance = model::BalanceInfo {
        available: 10_000 + 10_000,
        locked: 0u64,
    };

    let db = HashMapDb::default();
    db.put(&keys::account_utxo_set(0), utxo_set).unwrap();
    db.put(&keys::account_balance(0), new_balance).unwrap();
    db.put(&keys::pkh(&pkh1), path1).unwrap();
    db.put(&keys::pkh(&pkh2), path2).unwrap();
    let (wallet, _db) = factories::wallet(Some(db));
    let mut state = wallet.state.write().unwrap();
    let pkh3 = factories::pkh();
    let value = 50;
    let fee = Fee::default();
    let time_lock = 0;
    let utxo_strategy = UtxoSelectionStrategy::Random { from: Some(pkh1) };
    let vto = ValueTransferOutput {
        pkh: pkh3,
        value,
        time_lock,
    };
    let TransactionComponents { inputs, .. } = wallet
        .create_vt_transaction_components(
            &mut state,
            vec![vto],
            fee,
            &utxo_strategy,
            HashSet::default(),
        )
        .unwrap();

    assert!(inputs
        .pointers
        .iter()
        .all(|pointer| { pointer.output_index < 50 }))
}

#[test]
fn test_create_transaction_components_filter_from_address_2() {
    // Create UTXO set with 2 UTXOs from different addresses
    let pkh1 = factories::pkh();
    let pkh2 = factories::pkh();
    assert_ne!(pkh1, pkh2);
    let mut utxo_set: HashMap<model::OutPtr, model::OutputInfo> = HashMap::new();

    for i in 0..100 {
        // 50 UTXOs with pkh1 and 50 UTXOs with pkh2
        let pkh = if i < 50 { pkh1 } else { pkh2 };
        utxo_set.insert(
            model::OutPtr {
                txn_hash: vec![0; 32],
                output_index: i,
            },
            model::OutputInfo {
                pkh,
                amount: 1,
                time_lock: 0,
            },
        );
    }

    let path1 = model::Path {
        account: 0,
        keychain: constants::EXTERNAL_KEYCHAIN,
        index: 0,
    };
    let path2 = model::Path {
        account: 0,
        keychain: constants::EXTERNAL_KEYCHAIN,
        index: 1,
    };
    let new_balance = model::BalanceInfo {
        available: 10_000 + 10_000,
        locked: 0u64,
    };

    let db = HashMapDb::default();
    db.put(&keys::account_utxo_set(0), utxo_set).unwrap();
    db.put(&keys::account_balance(0), new_balance).unwrap();
    db.put(&keys::pkh(&pkh1), path1).unwrap();
    db.put(&keys::pkh(&pkh2), path2).unwrap();
    let (wallet, _db) = factories::wallet(Some(db));
    let mut state = wallet.state.write().unwrap();
    let pkh3 = factories::pkh();
    let value = 50;
    let fee = Fee::default();
    let time_lock = 0;
    let utxo_strategy = UtxoSelectionStrategy::Random { from: Some(pkh2) };
    let vto = ValueTransferOutput {
        pkh: pkh3,
        value,
        time_lock,
    };
    let TransactionComponents { inputs, .. } = wallet
        .create_vt_transaction_components(
            &mut state,
            vec![vto],
            fee,
            &utxo_strategy,
            HashSet::default(),
        )
        .unwrap();

    assert!(inputs
        .pointers
        .iter()
        .all(|pointer| { pointer.output_index >= 50 }))
}

#[test]
fn test_create_transaction_components_filter_from_address_3() {
    // Create UTXO set with 2 UTXOs from different addresses
    let pkh1 = factories::pkh();
    let pkh2 = factories::pkh();
    assert_ne!(pkh1, pkh2);
    let mut utxo_set: HashMap<model::OutPtr, model::OutputInfo> = HashMap::new();

    for i in 0..100 {
        // 50 UTXOs with pkh1 and 50 UTXOs with pkh2
        let pkh = if i < 50 { pkh1 } else { pkh2 };
        utxo_set.insert(
            model::OutPtr {
                txn_hash: vec![0; 32],
                output_index: i,
            },
            model::OutputInfo {
                pkh,
                amount: 1,
                time_lock: 0,
            },
        );
    }

    let path1 = model::Path {
        account: 0,
        keychain: constants::EXTERNAL_KEYCHAIN,
        index: 0,
    };
    let path2 = model::Path {
        account: 0,
        keychain: constants::EXTERNAL_KEYCHAIN,
        index: 1,
    };
    let new_balance = model::BalanceInfo {
        available: 10_000 + 10_000,
        locked: 0u64,
    };

    let db = HashMapDb::default();
    db.put(&keys::account_utxo_set(0), utxo_set).unwrap();
    db.put(&keys::account_balance(0), new_balance).unwrap();
    db.put(&keys::pkh(&pkh1), path1).unwrap();
    db.put(&keys::pkh(&pkh2), path2).unwrap();
    let (wallet, _db) = factories::wallet(Some(db));
    let mut state = wallet.state.write().unwrap();
    let pkh3 = factories::pkh();
    let value = 50;
    let fee = Fee::default();
    let time_lock = 0;
    let utxo_strategy = UtxoSelectionStrategy::Random { from: Some(pkh3) };
    let vto = ValueTransferOutput {
        pkh: pkh3,
        value,
        time_lock,
    };
    let err = wallet
        .create_vt_transaction_components(
            &mut state,
            vec![vto],
            fee,
            &utxo_strategy,
            HashSet::default(),
        )
        .unwrap_err();

    assert!(
        matches!(err, repository::Error::InsufficientBalance { .. }),
        "{:?}",
        err
    );
}

#[test]
fn test_create_transaction_components_does_not_use_unconfirmed_utxos() {
    let txn_hash_confirmed = vec![1; 32];
    let txn_hash_pending = vec![2; 32];
    let pkh = factories::pkh();
    let utxo_set: HashMap<model::OutPtr, model::OutputInfo> = HashMap::from_iter(vec![
        (
            model::OutPtr {
                txn_hash: txn_hash_confirmed,
                output_index: 0,
            },
            model::OutputInfo {
                pkh,
                amount: 2,
                time_lock: 0,
            },
        ),
        (
            model::OutPtr {
                txn_hash: txn_hash_pending.clone(),
                output_index: 1,
            },
            model::OutputInfo {
                pkh,
                amount: 5,
                time_lock: 0,
            },
        ),
    ]);
    let path = model::Path {
        account: 0,
        keychain: constants::EXTERNAL_KEYCHAIN,
        index: 0,
    };
    let new_balance = model::BalanceInfo {
        available: 7,
        locked: 0u64,
    };

    let db = HashMapDb::default();
    db.put(&keys::account_utxo_set(0), utxo_set).unwrap();
    db.put(&keys::account_balance(0), new_balance).unwrap();
    db.put(&keys::pkh(&pkh), path).unwrap();
    let (mut wallet, _db) = factories::wallet(Some(db));
    // Forbid wallet from using unconfirmed UTXOs
    wallet.params.use_unconfirmed_utxos = false;
    let mut state = wallet.state.write().unwrap();
    // Mark transaction as pending
    state
        .pending_transactions
        .insert(Hash::from(txn_hash_pending));
    let pkh = factories::pkh();
    let value = 4;
    let fee = Fee::default();
    let time_lock = 0;
    let utxo_strategy = UtxoSelectionStrategy::Random { from: None };
    let vto = ValueTransferOutput {
        pkh,
        value,
        time_lock,
    };
    let err = wallet
        .create_vt_transaction_components(
            &mut state,
            vec![vto],
            fee,
            &utxo_strategy,
            HashSet::default(),
        )
        .unwrap_err();

    assert!(
        matches!(err, repository::Error::InsufficientBalance { .. }),
        "{:?}",
        err
    );

    // But creating a transaction that only uses the confirmed UTXO works
    let vto = ValueTransferOutput {
        pkh,
        value: 2,
        time_lock,
    };

    let TransactionComponents {
        inputs, outputs, ..
    } = wallet
        .create_vt_transaction_components(
            &mut state,
            vec![vto],
            fee,
            &utxo_strategy,
            HashSet::default(),
        )
        .unwrap();

    assert_eq!(inputs.pointers.len(), 1);
    assert_eq!(inputs.resolved.len(), 1);
    assert_eq!(outputs.len(), 1);
}

#[test]
fn test_create_transaction_components_uses_unconfirmed_utxos() {
    let txn_hash_confirmed = vec![1; 32];
    let txn_hash_pending = vec![2; 32];
    let pkh = factories::pkh();
    let utxo_set: HashMap<model::OutPtr, model::OutputInfo> = HashMap::from_iter(vec![
        (
            model::OutPtr {
                txn_hash: txn_hash_confirmed,
                output_index: 0,
            },
            model::OutputInfo {
                pkh,
                amount: 2,
                time_lock: 0,
            },
        ),
        (
            model::OutPtr {
                txn_hash: txn_hash_pending.clone(),
                output_index: 1,
            },
            model::OutputInfo {
                pkh,
                amount: 5,
                time_lock: 0,
            },
        ),
    ]);
    let path = model::Path {
        account: 0,
        keychain: constants::EXTERNAL_KEYCHAIN,
        index: 0,
    };
    let new_balance = model::BalanceInfo {
        available: 7,
        locked: 0u64,
    };

    let db = HashMapDb::default();
    db.put(&keys::account_utxo_set(0), utxo_set).unwrap();
    db.put(&keys::account_balance(0), new_balance).unwrap();
    db.put(&keys::pkh(&pkh), path).unwrap();
    let (mut wallet, _db) = factories::wallet(Some(db));
    // Allow wallet to use unconfirmed UTXOs
    wallet.params.use_unconfirmed_utxos = true;
    let mut state = wallet.state.write().unwrap();
    // Mark transaction as pending
    state
        .pending_transactions
        .insert(Hash::from(txn_hash_pending));
    let pkh = factories::pkh();
    let value = 7;
    let fee = Fee::default();
    let time_lock = 0;
    let utxo_strategy = UtxoSelectionStrategy::Random { from: None };
    let vto = ValueTransferOutput {
        pkh,
        value,
        time_lock,
    };
    let TransactionComponents {
        inputs, outputs, ..
    } = wallet
        .create_vt_transaction_components(
            &mut state,
            vec![vto],
            fee,
            &utxo_strategy,
            HashSet::default(),
        )
        .unwrap();

    // The transaction should use both UTXOs as inputs, including the unconfirmed one
    assert_eq!(inputs.pointers.len(), 2);
    assert_eq!(inputs.resolved.len(), 2);
    assert_eq!(outputs.len(), 1);
}

#[test]
fn test_create_vtt_selecting_utxos() {
    let pkh = factories::pkh();
    let out_pointer_0 = model::OutPtr {
        txn_hash: vec![0; 32],
        output_index: 0,
    };
    let out_pointer_1 = model::OutPtr {
        txn_hash: vec![0; 32],
        output_index: 1,
    };
    let utxo_set: HashMap<model::OutPtr, model::OutputInfo> = HashMap::from_iter(vec![
        (
            out_pointer_0.clone(),
            model::OutputInfo {
                pkh,
                amount: 2,
                time_lock: 0,
            },
        ),
        (
            out_pointer_1.clone(),
            model::OutputInfo {
                pkh,
                amount: 5,
                time_lock: 0,
            },
        ),
    ]);
    let path = model::Path {
        account: 0,
        keychain: constants::EXTERNAL_KEYCHAIN,
        index: 0,
    };

    let db = HashMapDb::default();
    db.put(&keys::account_utxo_set(0), utxo_set).unwrap();
    db.put(&keys::pkh(&pkh), path).unwrap();
    let (wallet, _db) = factories::wallet(Some(db));
    let mut state = wallet.state.write().unwrap();

    let pkh = factories::pkh();
    let value = 4;
    let fee = Fee::default();
    let time_lock = 0;
    let utxo_strategy = UtxoSelectionStrategy::Random { from: None };

    let vto = ValueTransferOutput {
        pkh,
        value,
        time_lock,
    };

    // In case of using the small utxo, return Error::InsufficientBalance
    let err = wallet
        .create_vt_transaction_components(
            &mut state,
            vec![vto.clone()],
            fee,
            &utxo_strategy,
            vec![out_pointer_0].into_iter().collect(),
        )
        .unwrap_err();

    assert!(
        matches!(err, repository::Error::InsufficientBalance { .. }),
        "{:?}",
        err
    );

    // In case of using the big utxo, everything goes well
    let TransactionComponents {
        inputs, outputs, ..
    } = wallet
        .create_vt_transaction_components(
            &mut state,
            vec![vto.clone()],
            fee,
            &utxo_strategy,
            vec![out_pointer_1].into_iter().collect(),
        )
        .unwrap();

    assert_eq!(1, inputs.pointers.len());
    assert_eq!(1, inputs.resolved.len());
    assert_eq!(2, outputs.len());
    assert_eq!(value, outputs[0].value);
    let expected_change = 1;
    assert_eq!(expected_change, outputs[1].value);

    // In case of no specify any utxo, everything goes well
    let TransactionComponents { outputs, .. } = wallet
        .create_vt_transaction_components(
            &mut state,
            vec![vto],
            fee,
            &utxo_strategy,
            HashSet::default(),
        )
        .unwrap();

    assert_eq!(value, outputs[0].value);
}

#[test]
fn test_create_transaction_components_does_not_use_unconfirmed_utxos_and_selecting_utxos() {
    let txn_hash_pending = vec![2; 32];
    let pkh = factories::pkh();
    let pending_outptr = model::OutPtr {
        txn_hash: txn_hash_pending.clone(),
        output_index: 1,
    };
    let utxo_set: HashMap<model::OutPtr, model::OutputInfo> = HashMap::from_iter(vec![(
        pending_outptr.clone(),
        model::OutputInfo {
            pkh,
            amount: 5,
            time_lock: 0,
        },
    )]);
    let path = model::Path {
        account: 0,
        keychain: constants::EXTERNAL_KEYCHAIN,
        index: 0,
    };
    let new_balance = model::BalanceInfo {
        available: 7,
        locked: 0u64,
    };

    let db = HashMapDb::default();
    db.put(&keys::account_utxo_set(0), utxo_set).unwrap();
    db.put(&keys::account_balance(0), new_balance).unwrap();
    db.put(&keys::pkh(&pkh), path).unwrap();
    let (mut wallet, _db) = factories::wallet(Some(db));
    // Forbid wallet from using unconfirmed UTXOs
    wallet.params.use_unconfirmed_utxos = false;
    let mut state = wallet.state.write().unwrap();
    // Mark transaction as pending
    state
        .pending_transactions
        .insert(Hash::from(txn_hash_pending));
    let pkh = factories::pkh();
    let value = 4;
    let fee = Fee::default();
    let time_lock = 0;
    let utxo_strategy = UtxoSelectionStrategy::Random { from: None };
    let vto = ValueTransferOutput {
        pkh,
        value,
        time_lock,
    };

    // In case of no using unconfirmed transaction and we select a pending utxo, we still can not use it
    let err = wallet
        .create_vt_transaction_components(
            &mut state,
            vec![vto],
            fee,
            &utxo_strategy,
            vec![pending_outptr].into_iter().collect(),
        )
        .unwrap_err();

    assert!(
        matches!(err, repository::Error::InsufficientBalance { .. }),
        "{:?}",
        err
    );
}

#[test]
fn test_unlock_wallet_backwards_compatible() {
    // db created using:
    // let (wallet, db) = factories::wallet(None);
    // println!("{}", db.export_to_json().unwrap());

    let db = HashMapDb::import_from_json("[[[101,120,97,109,112,108,101,45,119,97,108,108,101,116,105,118],[16,0,0,0,0,0,0,0,253,220,27,197,124,58,52,48,149,209,246,175,226,167,24,235]],[[100,101,102,97,117,108,116,45,97,99,99,111,117,110,116],[0,0,0,0]],[[98,105,114,116,104,45,100,97,116,101],[0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0]],[[97,99,99,111,117,110,116,45,48,45,48,45,107,101,121],[32,0,0,0,0,0,0,0,138,147,131,114,62,143,164,77,65,3,128,60,53,66,230,108,34,29,118,222,26,170,237,96,102,215,94,227,107,132,164,179,32,0,0,0,0,0,0,0,31,132,33,245,90,11,143,28,106,239,4,135,237,90,168,51,103,119,71,41,85,64,211,24,187,167,46,148,194,32,181,25]],[[109,97,115,116,101,114,45,107,101,121],[32,0,0,0,0,0,0,0,104,215,222,1,179,103,48,156,129,116,214,126,65,8,186,43,227,75,42,174,149,13,59,190,61,147,106,113,156,234,112,103,32,0,0,0,0,0,0,0,233,91,131,28,53,151,204,57,207,6,225,50,113,67,70,16,120,151,29,93,114,9,217,245,212,145,169,196,138,206,81,232]],[[97,99,99,111,117,110,116,45,48,45,49,45,107,101,121],[32,0,0,0,0,0,0,0,159,251,203,85,170,0,252,117,89,207,127,183,202,138,136,177,9,166,37,188,2,117,143,179,121,90,111,70,108,156,252,7,32,0,0,0,0,0,0,0,117,94,110,127,110,158,134,8,141,217,209,199,209,173,223,91,131,92,167,133,147,7,79,157,249,183,18,136,1,230,8,53]],[[108,97,115,116,45,115,121,110,99],[0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0]],[[119,97,108,108,101,116,115],[1,0,0,0,0,0,0,0,14,0,0,0,0,0,0,0,101,120,97,109,112,108,101,45,119,97,108,108,101,116]],[[101,120,97,109,112,108,101,45,119,97,108,108,101,116,115,97,108,116],[32,0,0,0,0,0,0,0,26,124,168,170,31,187,78,102,12,152,23,213,241,242,206,201,58,116,200,79,251,130,103,254,13,178,54,228,142,39,208,161]]]").unwrap();
    let id = "example-wallet";
    let session_id = types::SessionId::from(String::from(id));
    let params = factories::default_params();
    let _wallet = Wallet::unlock(id, session_id, db, params).unwrap();
}
