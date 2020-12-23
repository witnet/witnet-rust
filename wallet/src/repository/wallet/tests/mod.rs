use std::{collections::HashMap, iter::FromIterator as _, mem};

use super::*;
use crate::{db::HashMapDb, repository::wallet::tests::factories::vtt_from_body, *};
use witnet_data_structures::{
    chain::Hashable, transaction::VTTransaction, transaction_factory::calculate_weight,
};

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

    let mut address1 = (*wallet.gen_internal_address(None).unwrap()).clone();
    address1.info.db_key = Default::default();
    let mut address2 = (*wallet.gen_internal_address(None).unwrap()).clone();
    address2.info.db_key = Default::default();
    let mut address3 = (*wallet.gen_internal_address(None).unwrap()).clone();
    address3.info.db_key = Default::default();

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
    let mut address = (*wallet.gen_internal_address(None).unwrap()).clone();
    address.info.db_key = Default::default();
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
    db.put(&keys::account_balance(0), &new_balance).unwrap();

    let (wallet, _db) = factories::wallet(Some(db));

    assert_eq!(99, wallet.balance().unwrap().confirmed.available);
    assert_eq!(0, wallet.balance().unwrap().confirmed.locked);
}

#[test]
fn test_create_transaction_components_when_wallet_have_no_utxos() {
    let (wallet, _db) = factories::wallet(None);
    let mut state = wallet.state.write().unwrap();
    let value = 1;
    let fee = 0;
    let pkh = factories::pkh();
    let time_lock = 0;
    let vto = ValueTransferOutput {
        pkh,
        value,
        time_lock,
    };
    let err = wallet
        .create_vt_transaction_components(&mut state, vec![vto], fee, FeeType::Absolute)
        .unwrap_err();

    assert_eq!(
        mem::discriminant(&repository::Error::InsufficientBalance),
        mem::discriminant(&err)
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
    let fee = 0;
    let time_lock = 0;
    let vto = ValueTransferOutput {
        pkh,
        value,
        time_lock,
    };

    let (inputs, outputs) = wallet
        .create_vt_transaction_components(&mut state, vec![vto], fee, FeeType::Absolute)
        .unwrap();

    assert_eq!(1, inputs.len());
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
    let fee = 0;
    let time_lock = 0;

    let vto = ValueTransferOutput {
        pkh,
        value,
        time_lock,
    };

    let (inputs, outputs) = wallet
        .create_vt_transaction_components(&mut state, vec![vto], fee, FeeType::Absolute)
        .unwrap();

    assert_eq!(1, inputs.len());
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
    let fee = 0;
    let time_lock = 0;
    let vto = ValueTransferOutput {
        pkh,
        value,
        time_lock,
    };
    let err = wallet
        .create_vt_transaction_components(&mut state, vec![vto], fee, FeeType::Absolute)
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
    let fee = 0;
    let time_lock = 0;

    let state_utxo_set = wallet.utxo_set().unwrap();
    let utxo_set: HashMap<model::OutPtr, model::OutputInfo> =
        db.get(&keys::account_utxo_set(0)).unwrap();

    assert_eq!(1, wallet.balance().unwrap().confirmed.available);
    assert_eq!(0, wallet.balance().unwrap().confirmed.locked);
    assert!(utxo_set.contains_key(&out_pointer));
    assert!(state_utxo_set.contains_key(&out_pointer));

    let vtt = wallet
        .create_vtt(types::VttParams {
            fee,
            outputs: vec![ValueTransferOutput {
                pkh,
                value,
                time_lock,
            }],
            fee_type: FeeType::Absolute,
        })
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

    let state_utxo_set = wallet.utxo_set().unwrap();
    let utxo_set: HashMap<model::OutPtr, model::OutputInfo> =
        db.get(&keys::account_utxo_set(0)).unwrap();

    assert_eq!(1, wallet.balance().unwrap().confirmed.available);
    assert_eq!(0, wallet.balance().unwrap().confirmed.locked);
    assert!(utxo_set.contains_key(&out_pointer));
    assert!(state_utxo_set.contains_key(&out_pointer));

    let request = DataRequestOutput {
        witness_reward: 1,
        witnesses: 1,
        ..DataRequestOutput::default()
    };

    let data_req = wallet
        .create_data_req(types::DataReqParams {
            fee: 0,
            request,
            fee_type: FeeType::Absolute,
        })
        .unwrap();

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
        .index_block_transactions(&block, &[vtt_from_body(body)], true, false)
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
        .index_block_transactions(&a_block, &[vtt_from_body(txn1)], true, false)
        .unwrap();
    wallet
        .index_block_transactions(&a_block, &[vtt_from_body(txn2)], true, false)
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
fn test_index_transaction_does_not_duplicate_transactions() {
    let account = 0;
    let (wallet, db) = factories::wallet(None);

    assert_eq!(
        0,
        db.get_or_default(&keys::transaction_next_id(account))
            .unwrap()
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
    let txn = VTTransactionBody::new(inputs, outputs);

    wallet
        .index_block_transactions(
            &block,
            &[factories::vtt_from_body(txn.clone())],
            true,
            false,
        )
        .unwrap();
    wallet
        .index_block_transactions(&block, &[factories::vtt_from_body(txn)], true, false)
        .unwrap();

    assert_eq!(1, db.get(&keys::transaction_next_id(account)).unwrap());
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
        .index_block_transactions(&block, &[factories::vtt_from_body(txn)], true, false)
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
            false,
        )
        .unwrap();

    // spend those funds to create a new transaction which is pending (it has no block)
    let vtt = wallet
        .create_vtt(types::VttParams {
            fee: 0,
            outputs: vec![ValueTransferOutput {
                pkh: their_pkh,
                value: 1,
                time_lock: 0,
            }],
            fee_type: FeeType::Absolute,
        })
        .unwrap();

    // There is a signature for each input
    assert_eq!(vtt.body.inputs.len(), vtt.signatures.len());

    // check that indeed, the previously created vtt has not been indexed
    let db_movement = db.get_opt(&keys::transaction_movement(0, 1)).unwrap();
    assert!(db_movement.is_none());

    // index another block confirming the previously created vtt
    wallet
        .index_block_transactions(&a_block, &[factories::vtt_from_body(vtt.body)], true, false)
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
            false,
        )
        .unwrap();

    assert_eq!(1, wallet.state.read().unwrap().utxo_set.len());

    assert!(wallet.get_transaction(0, 0).is_ok());
    assert!(wallet.get_transaction(0, 1).is_err());

    assert_eq!(2, wallet.balance().unwrap().unconfirmed.available);
    assert_eq!(0, wallet.balance().unwrap().unconfirmed.locked);

    // spend those funds to create a new transaction which is pending (it has no block)
    let vtt = wallet
        .create_vtt(types::VttParams {
            fee: 0,
            outputs: vec![ValueTransferOutput {
                pkh: their_pkh,
                value: 1,
                time_lock: 0,
            }],
            fee_type: FeeType::Absolute,
        })
        .unwrap();

    // There is a signature for each input
    assert_eq!(vtt.body.inputs.len(), vtt.signatures.len());

    // the wallet does not store created VTT transactions until confirmation
    assert!(wallet.get_transaction(0, 1).is_err());

    // index another block confirming the previously created vtt
    wallet
        .index_block_transactions(&a_block, &[factories::vtt_from_body(vtt.body)], true, false)
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
            false,
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
    let vtt = wallet
        .create_vtt(types::VttParams {
            fee: 0,
            outputs: vec![ValueTransferOutput {
                pkh: their_pkh,
                value: 1,
                time_lock: 0,
            }],
            fee_type: FeeType::Absolute,
        })
        .unwrap();

    // There is a signature for each input
    assert_eq!(vtt.body.inputs.len(), vtt.signatures.len());

    // the wallet does not store created VTT transactions until confirmation
    let x = wallet.transactions(0, 1).unwrap();
    assert_eq!(x.transactions.len(), 1);
    assert_eq!(x.total, 1);

    // index another block confirming the previously created vtt
    wallet
        .index_block_transactions(&a_block, &[factories::vtt_from_body(vtt.body)], true, false)
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
            false,
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
            fee: 0,
            outputs: vec![ValueTransferOutput {
                pkh: their_pkh,
                value: 1,
                time_lock: 0,
            }],
            fee_type: FeeType::Absolute,
        })
        .unwrap_err();

    assert_eq!(
        mem::discriminant(&repository::Error::InsufficientBalance),
        mem::discriminant(&err),
        "{:?}",
        err,
    );
}

#[test]
fn test_create_vtt_with_multiple_outputs() {
    let (wallet, _db) = factories::wallet(None);

    let a_block = factories::BlockInfo::default().create();
    let our_address = wallet.gen_external_address(None).unwrap();

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
            false,
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
    let vtt = wallet
        .create_vtt(types::VttParams {
            fee: 0,
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
            fee_type: FeeType::Absolute,
        })
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
    assert_eq!(
        wallet
            .export_master_key(password)
            .unwrap()
            .starts_with("xprvdouble"),
        false
    );
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
    let fee = 1;
    let time_lock = 0;
    let vto = ValueTransferOutput {
        pkh,
        value,
        time_lock,
    };
    let (inputs, outputs) = wallet
        .create_vt_transaction_components(&mut state, vec![vto], fee, FeeType::Weighted)
        .unwrap();

    assert_eq!(1, inputs.len());
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
    let fee = 1;
    let time_lock = 0;
    let vto = ValueTransferOutput {
        pkh,
        value,
        time_lock,
    };
    let (inputs, outputs) = wallet
        .create_vt_transaction_components(&mut state, vec![vto], fee, FeeType::Weighted)
        .unwrap();

    assert!(!inputs.is_empty());
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
    let fee = 1;
    let time_lock = 0;
    let vto = ValueTransferOutput {
        pkh,
        value,
        time_lock,
    };
    let err = wallet
        .create_vt_transaction_components(&mut state, vec![vto], fee, FeeType::Weighted)
        .unwrap_err();

    assert_eq!(
        mem::discriminant(&repository::Error::InsufficientBalance),
        mem::discriminant(&err),
        "{:?}",
        err,
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
    let fee = 1;
    let time_lock = 0;
    let vto = ValueTransferOutput {
        pkh,
        value,
        time_lock,
    };
    let (inputs, outputs) = wallet
        .create_vt_transaction_components(&mut state, vec![vto], fee, FeeType::Weighted)
        .unwrap();

    assert!(!inputs.is_empty());
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
    let fee = 1;
    let time_lock = 0;
    let vto = ValueTransferOutput {
        pkh,
        value,
        time_lock,
    };
    let (inputs, outputs) = wallet
        .create_vt_transaction_components(&mut state, vec![vto], fee, FeeType::Weighted)
        .unwrap();

    assert!(!inputs.is_empty());
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
    let fee = 1;
    let time_lock = 0;
    let vto = ValueTransferOutput {
        pkh,
        value,
        time_lock,
    };
    let (inputs, outputs) = wallet
        .create_vt_transaction_components(&mut state, vec![vto], fee, FeeType::Weighted)
        .unwrap();

    assert!(inputs.len() >= 2);
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
    let fee = 1;
    let time_lock = 0;
    let vto = ValueTransferOutput {
        pkh,
        value,
        time_lock,
    };
    let err = wallet
        .create_vt_transaction_components(&mut state, vec![vto], fee, FeeType::Weighted)
        .unwrap_err();

    assert_eq!(
        mem::discriminant(&repository::Error::InsufficientBalance),
        mem::discriminant(&err),
        "{:?}",
        err,
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
    let fee = u64::MAX;
    let time_lock = 0;
    let vto = ValueTransferOutput {
        pkh,
        value,
        time_lock,
    };
    let err = wallet
        .create_vt_transaction_components(&mut state, vec![vto], fee, FeeType::Weighted)
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
    let fee = 0;
    let time_lock = 0;
    let vto = ValueTransferOutput {
        pkh,
        value,
        time_lock,
    };
    let err = wallet
        .create_vt_transaction_components(&mut state, vec![vto], fee, FeeType::Weighted)
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
    let fee = 1;
    let (inputs, _) = wallet
        .create_dr_transaction_components(&mut state, request, fee, FeeType::Weighted)
        .unwrap();

    assert_eq!(inputs.len(), 1);
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
    let fee = 1;
    let err = wallet
        .create_dr_transaction_components(&mut state, request, fee, FeeType::Weighted)
        .unwrap_err();

    assert_eq!(
        mem::discriminant(&repository::Error::InsufficientBalance),
        mem::discriminant(&err),
        "{:?}",
        err,
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

    let fee = 1;
    let (inputs, _) = wallet
        .create_dr_transaction_components(&mut state, request.clone(), fee, FeeType::Weighted)
        .unwrap();
    let weight = calculate_weight(inputs.len(), 1, Some(&request), u32::MAX).unwrap();

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

    let (inputs, _) = wallet_2
        .create_dr_transaction_components(&mut state_2, request, fee, FeeType::Weighted)
        .unwrap();

    assert_eq!(inputs.len(), 3);
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

    let fee = 1;
    let err = wallet
        .create_dr_transaction_components(&mut state, request, fee, FeeType::Weighted)
        .unwrap_err();

    assert_eq!(
        mem::discriminant(&repository::Error::InsufficientBalance),
        mem::discriminant(&err),
        "{:?}",
        err,
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
    let fee = 0;
    let err = wallet
        .create_dr_transaction_components(&mut state, request.clone(), fee, FeeType::Weighted)
        .unwrap_err();

    assert_eq!(
        mem::discriminant(&repository::Error::MaximumDRWeightReached(request)),
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

    let fee = u64::MAX / 2;
    let err = wallet
        .create_dr_transaction_components(&mut state, request, fee, FeeType::Weighted)
        .unwrap_err();

    assert_eq!(
        mem::discriminant(&repository::Error::FeeTooLarge),
        mem::discriminant(&err),
        "{:?}",
        err,
    );
}
