use std::{collections::HashMap, iter::FromIterator as _, mem};

use super::*;
use crate::repository::wallet::tests::factories::vtt_from_body;
use crate::types::Hashable;
use crate::*;
use witnet_data_structures::transaction::VTTransaction;

mod factories;

#[test]
fn test_wallet_public_data() {
    let (wallet, _db) = factories::wallet(None);
    let data = wallet.public_data().unwrap();

    assert!(data.name.is_none());
    assert!(data.caption.is_none());
    assert_eq!(0, data.balance);
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
        db.get::<_, u32>(&keys::account_next_index(account, keychain))
            .unwrap()
    );

    wallet.gen_external_address(None).unwrap();

    assert_eq!(
        2,
        db.get::<_, u32>(&keys::account_next_index(account, keychain))
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
        db.get::<_, String>(&keys::address(account, keychain, index))
            .unwrap()
    );
    assert_eq!(
        address.path,
        db.get::<_, String>(&keys::address_path(account, keychain, index))
            .unwrap()
    );
    assert_eq!(
        address.pkh,
        db.get::<_, types::PublicKeyHash>(&keys::address_pkh(account, keychain, index))
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
fn test_list_external_addresses() {
    let (wallet, _db) = factories::wallet(None);

    let address1 = wallet.gen_external_address(None).unwrap();
    let address2 = wallet.gen_external_address(None).unwrap();
    let address3 = wallet.gen_external_address(None).unwrap();

    let offset = 0;
    let limit = 10;
    let addresses = wallet.external_addresses(offset, limit).unwrap();

    assert_eq!(3, addresses.total);
    assert_eq!(address3, addresses[0]);
    assert_eq!(address2, addresses[1]);
    assert_eq!(address1, addresses[2]);
}

#[test]
fn test_list_external_addresses_paginated() {
    let (wallet, _db) = factories::wallet(None);

    let _ = wallet.gen_external_address(None).unwrap();
    let address2 = wallet.gen_external_address(None).unwrap();
    let _ = wallet.gen_external_address(None).unwrap();

    let offset = 1;
    let limit = 1;
    let addresses = wallet.external_addresses(offset, limit).unwrap();

    assert_eq!(3, addresses.total);
    assert_eq!(1, addresses.len());
    assert_eq!(address2, addresses[0]);
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
        db.get::<_, u32>(&keys::account_next_index(account, keychain,))
            .unwrap()
    );

    wallet.gen_internal_address(None).unwrap();

    assert_eq!(
        2,
        db.get::<_, u32>(&keys::account_next_index(account, keychain))
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
        db.get::<_, String>(&keys::address(account, keychain, index))
            .unwrap()
    );
    assert_eq!(
        address.path,
        db.get::<_, String>(&keys::address_path(account, keychain, index))
            .unwrap()
    );
    assert_eq!(
        address.pkh,
        db.get::<_, types::PublicKeyHash>(&keys::address_pkh(account, keychain, index))
            .unwrap()
    );
    assert_eq!(
        label,
        db.get::<_, model::AddressInfo>(&keys::address_info(account, keychain, index))
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
    let (wallet, _db) = factories::wallet(None);

    assert_eq!(0, wallet.balance().unwrap().amount);

    let mut db = HashMap::new();
    db.insert(
        keys::account_balance(0).as_bytes().to_vec(),
        bincode::serialize(&99u64).unwrap(),
    );

    let (wallet, _db) = factories::wallet(Some(db));

    assert_eq!(99, wallet.balance().unwrap().amount);
}

#[test]
fn test_create_transaction_components_when_wallet_have_no_utxos() {
    let (wallet, _db) = factories::wallet(None);
    let mut state = wallet.state.write().unwrap();
    let value = 1;
    let fee = 0;
    let pkh = factories::pkh();
    let time_lock = 0;
    let err = wallet
        ._create_transaction_components(&mut state, value, fee, Some((pkh, time_lock)), false)
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
    let utxo_set: HashMap<model::OutPtr, model::KeyBalance> = HashMap::from_iter(vec![(
        out_pointer.clone(),
        model::KeyBalance { pkh, amount: 1 },
    )]);
    let path = model::Path {
        account: 0,
        keychain: constants::EXTERNAL_KEYCHAIN,
        index: 0,
    };
    let db = HashMap::from_iter(vec![
        (
            keys::account_utxo_set(0).as_bytes().to_vec(),
            bincode::serialize(&utxo_set).unwrap(),
        ),
        (keys::pkh(&pkh), bincode::serialize(&path).unwrap()),
    ]);

    let (wallet, _db) = factories::wallet(Some(db));
    let mut state = wallet.state.write().unwrap();
    let pkh = factories::pkh();
    let value = 1;
    let fee = 0;
    let time_lock = 0;
    let vtt = wallet
        ._create_transaction_components(&mut state, value, fee, Some((pkh, time_lock)), false)
        .unwrap();

    assert_eq!(1, vtt.value);
    assert_eq!(0, vtt.change);
    assert_eq!(vec![out_pointer], vtt.used_utxos);
    assert_eq!(1, vtt.sign_keys.len());
    assert_eq!(1, vtt.inputs.len());
    assert_eq!(1, vtt.outputs.len());
}

#[test]
fn test_create_transaction_components_whith_a_change_address() {
    let pkh = factories::pkh();
    let out_pointer = model::OutPtr {
        txn_hash: vec![0; 32],
        output_index: 0,
    };
    let utxo_set: HashMap<model::OutPtr, model::KeyBalance> =
        HashMap::from_iter(vec![(out_pointer, model::KeyBalance { pkh, amount: 2 })]);
    let path = model::Path {
        account: 0,
        keychain: constants::EXTERNAL_KEYCHAIN,
        index: 0,
    };
    let db = HashMap::from_iter(vec![
        (
            keys::account_utxo_set(0).as_bytes().to_vec(),
            bincode::serialize(&utxo_set).unwrap(),
        ),
        (keys::pkh(&pkh), bincode::serialize(&path).unwrap()),
    ]);

    let (wallet, _db) = factories::wallet(Some(db));
    let mut state = wallet.state.write().unwrap();
    let pkh = factories::pkh();
    let value = 1;
    let fee = 0;
    let out_pointer = model::OutPtr {
        txn_hash: vec![0; 32],
        output_index: 0,
    };
    let time_lock = 0;
    let vtt = wallet
        ._create_transaction_components(&mut state, value, fee, Some((pkh, time_lock)), false)
        .unwrap();

    assert_eq!(1, vtt.value);
    assert_eq!(1, vtt.change);
    assert_eq!(vec![out_pointer], vtt.used_utxos);
    assert_eq!(1, vtt.sign_keys.len());
    assert_eq!(1, vtt.inputs.len());
    assert_eq!(2, vtt.outputs.len());
}

#[test]
fn test_create_transaction_components_which_value_overflows() {
    let pkh = factories::pkh();
    let utxo_set: HashMap<model::OutPtr, model::KeyBalance> = HashMap::from_iter(vec![
        (
            model::OutPtr {
                txn_hash: vec![0; 32],
                output_index: 0,
            },
            model::KeyBalance { pkh, amount: 2 },
        ),
        (
            model::OutPtr {
                txn_hash: vec![0; 32],
                output_index: 1,
            },
            model::KeyBalance {
                pkh,
                amount: std::u64::MAX - 1,
            },
        ),
    ]);
    let path = model::Path {
        account: 0,
        keychain: constants::EXTERNAL_KEYCHAIN,
        index: 0,
    };
    let db = HashMap::from_iter(vec![
        (
            keys::account_utxo_set(0).as_bytes().to_vec(),
            bincode::serialize(&utxo_set).unwrap(),
        ),
        (
            keys::account_balance(0).as_bytes().to_vec(),
            bincode::serialize(&std::u64::MAX).unwrap(),
        ),
        (keys::pkh(&pkh), bincode::serialize(&path).unwrap()),
    ]);

    let (wallet, _db) = factories::wallet(Some(db));
    let mut state = wallet.state.write().unwrap();
    let pkh = factories::pkh();
    let value = std::u64::MAX;
    let fee = 0;
    let time_lock = 0;
    let err = wallet
        ._create_transaction_components(&mut state, value, fee, Some((pkh, time_lock)), false)
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
    let utxo_set: HashMap<model::OutPtr, model::KeyBalance> = HashMap::from_iter(vec![(
        out_pointer.clone(),
        model::KeyBalance { pkh, amount: 1 },
    )]);
    let path = model::Path {
        account: 0,
        keychain: constants::EXTERNAL_KEYCHAIN,
        index: 0,
    };
    let db = HashMap::from_iter(vec![
        (
            keys::account_utxo_set(0).as_bytes().to_vec(),
            bincode::serialize(&utxo_set).unwrap(),
        ),
        (
            keys::account_balance(0).as_bytes().to_vec(),
            bincode::serialize(&1u64).unwrap(),
        ),
        (keys::pkh(&pkh), bincode::serialize(&path).unwrap()),
    ]);

    let (wallet, db) = factories::wallet(Some(db));
    let pkh = factories::pkh();
    let value = 1;
    let fee = 0;
    let time_lock = 0;

    let state_utxo_set = wallet.utxo_set().unwrap();
    let utxo_set: HashMap<model::OutPtr, model::KeyBalance> =
        db.get(&keys::account_utxo_set(0)).unwrap();

    assert_eq!(1, wallet.balance().unwrap().amount);
    assert!(utxo_set.contains_key(&out_pointer));
    assert!(state_utxo_set.contains_key(&out_pointer));

    let vtt = wallet
        .create_vtt(types::VttParams {
            pkh,
            value,
            fee,
            time_lock,
        })
        .unwrap();

    let state_utxo_set = wallet.utxo_set().unwrap();
    let new_utxo_set: HashMap<model::OutPtr, model::KeyBalance> =
        db.get(&keys::account_utxo_set(0)).unwrap();

    // nothing should change because VTT is only created but not yet confirmed (sent!)
    assert!(new_utxo_set.contains_key(&out_pointer));
    assert!(state_utxo_set.contains_key(&out_pointer));

    assert_eq!(1, wallet.balance().unwrap().amount);
    assert_eq!(1, db.get::<_, u64>(&keys::account_balance(0)).unwrap());

    assert!(db
        .get::<_, u32>(&keys::transactions_index(vtt.hash().as_ref()))
        .is_err());
}

#[test]
fn test_create_data_request_does_not_spend_utxos() {
    let pkh = factories::pkh();
    let out_pointer = model::OutPtr {
        txn_hash: vec![0; 32],
        output_index: 0,
    };
    let utxo_set: HashMap<model::OutPtr, model::KeyBalance> = HashMap::from_iter(vec![(
        out_pointer.clone(),
        model::KeyBalance { pkh, amount: 1 },
    )]);
    let path = model::Path {
        account: 0,
        keychain: constants::EXTERNAL_KEYCHAIN,
        index: 0,
    };
    let db = HashMap::from_iter(vec![
        (
            keys::account_utxo_set(0).as_bytes().to_vec(),
            bincode::serialize(&utxo_set).unwrap(),
        ),
        (
            keys::account_balance(0).as_bytes().to_vec(),
            bincode::serialize(&1u64).unwrap(),
        ),
        (keys::pkh(&pkh), bincode::serialize(&path).unwrap()),
    ]);

    let (wallet, db) = factories::wallet(Some(db));

    let state_utxo_set = wallet.utxo_set().unwrap();
    let utxo_set: HashMap<model::OutPtr, model::KeyBalance> =
        db.get(&keys::account_utxo_set(0)).unwrap();

    assert_eq!(1, wallet.balance().unwrap().amount);
    assert!(utxo_set.contains_key(&out_pointer));
    assert!(state_utxo_set.contains_key(&out_pointer));

    let request = types::DataRequestOutput {
        witness_reward: 1,
        witnesses: 1,
        ..types::DataRequestOutput::default()
    };

    let data_req = wallet
        .create_data_req(types::DataReqParams { fee: 0, request })
        .unwrap();

    let state_utxo_set = wallet.utxo_set().unwrap();
    let new_utxo_set: HashMap<model::OutPtr, model::KeyBalance> =
        db.get(&keys::account_utxo_set(0)).unwrap();

    // nothing should change because DR is only created but not yet confirmed (sent!)
    assert!(new_utxo_set.contains_key(&out_pointer));
    assert!(state_utxo_set.contains_key(&out_pointer));

    assert_eq!(1, wallet.balance().unwrap().amount);
    assert_eq!(1, db.get::<_, u64>(&keys::account_balance(0)).unwrap());

    assert!(db
        .get::<_, u32>(&keys::transactions_index(data_req.hash().as_ref()))
        .is_err());
}

#[test]
fn test_index_transaction_output_affects_balance() {
    let (wallet, db) = factories::wallet(None);

    assert_eq!(
        0,
        db.get_or_default::<_, u64>(&keys::account_balance(0))
            .unwrap()
    );

    let value = 1u64;
    let address = wallet.gen_external_address(None).unwrap();
    let block = factories::BlockInfo::default().create();
    let inputs = vec![factories::Input::default().create()];
    let outputs = vec![factories::VttOutput::default()
        .with_pkh(address.pkh)
        .with_value(value)
        .create()];
    let body = types::VTTransactionBody::new(inputs, outputs);

    wallet
        .index_transactions(&block, &[vtt_from_body(body)])
        .unwrap();

    assert_eq!(1, db.get::<_, u64>(&keys::account_balance(0)).unwrap());
}

#[test]
fn test_index_transaction_input_affects_balance() {
    let (wallet, db) = factories::wallet(None);

    assert_eq!(
        0,
        db.get_or_default::<_, u64>(&keys::account_balance(0))
            .unwrap()
    );

    let address = wallet.gen_external_address(None).unwrap();

    let a_block = factories::BlockInfo::default().create();

    // txn1 gives a credit of 3 to our pkh
    let txn1 = types::VTTransactionBody::new(
        vec![factories::Input::default().create()],
        vec![factories::VttOutput::default()
            .with_pkh(address.pkh)
            .with_value(3)
            .create()],
    );

    // txn2 spends the previous credit and gives back a change of 1 to our pkh
    let txn2 = types::VTTransactionBody::new(
        vec![factories::Input::default()
            .with_transaction(txn1.hash())
            .with_output_index(0)
            .create()],
        vec![factories::VttOutput::default()
            .with_pkh(address.pkh)
            .with_value(1)
            .create()],
    );

    wallet
        .index_transactions(&a_block, &[vtt_from_body(txn1)])
        .unwrap();
    wallet
        .index_transactions(&a_block, &[vtt_from_body(txn2)])
        .unwrap();

    assert_eq!(1, db.get::<_, u64>(&keys::account_balance(0)).unwrap());
}

#[test]
fn test_index_transaction_does_not_duplicate_transactions() {
    let account = 0;
    let (wallet, db) = factories::wallet(None);

    assert_eq!(
        0,
        db.get_or_default::<_, u32>(&keys::transaction_next_id(account))
            .unwrap()
    );

    let value = 1u64;
    let address = wallet.gen_external_address(None).unwrap();
    let block = factories::BlockInfo::default().create();
    let inputs = vec![factories::Input::default().create()];
    let outputs = vec![factories::VttOutput::default()
        .with_pkh(address.pkh)
        .with_value(value)
        .create()];
    let txn = types::VTTransactionBody::new(inputs, outputs);

    wallet
        .index_transactions(&block, &[factories::vtt_from_body(txn.clone())])
        .unwrap();
    wallet
        .index_transactions(&block, &[factories::vtt_from_body(txn)])
        .unwrap();

    assert_eq!(
        1,
        db.get::<_, u32>(&keys::transaction_next_id(account))
            .unwrap()
    );
}

#[test]
fn test_index_transaction_errors_if_balance_overflow() {
    let (wallet, _db) = factories::wallet(None);

    let address = wallet.gen_external_address(None).unwrap();
    let block = factories::BlockInfo::default().create();
    let inputs = vec![factories::Input::default().create()];
    let outputs = vec![
        factories::VttOutput::default()
            .with_pkh(address.pkh)
            .with_value(1u64)
            .create(),
        factories::VttOutput::default()
            .with_pkh(address.pkh)
            .with_value(std::u64::MAX)
            .create(),
    ];
    let txn = types::VTTransactionBody::new(inputs, outputs);

    let err = wallet
        .index_transactions(&block, &[factories::vtt_from_body(txn)])
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
        .index_transactions(
            &a_block,
            &[factories::vtt_from_body(types::VTTransactionBody::new(
                vec![factories::Input::default().create()],
                vec![factories::VttOutput::default()
                    .with_pkh(our_address.pkh)
                    .with_value(2)
                    .create()],
            ))],
        )
        .unwrap();

    // spend those funds to create a new transaction which is pending (it has no block)
    let vtt = wallet
        .create_vtt(types::VttParams {
            pkh: their_pkh,
            value: 1,
            fee: 0,
            time_lock: 0,
        })
        .unwrap();

    // check that indeed, the previously created vtt has not been indexed
    let db_movement = db
        .get_opt::<_, model::BalanceMovement>(&keys::transaction_movement(0, 1))
        .unwrap();
    assert!(db_movement.is_none());

    // index another block confirming the previously created vtt
    wallet
        .index_transactions(&a_block, &[factories::vtt_from_body(vtt.body)])
        .unwrap();

    // check that indeed, the previously created vtt now has a block associated with it
    let block_after = db
        .get::<_, model::BalanceMovement>(&keys::transaction_movement(0, 1))
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
    assert!(wallet_data.caption.is_none());
    assert!(!db.contains(&keys::wallet_name()).unwrap());
    assert!(!db.contains(&keys::wallet_caption()).unwrap());

    wallet.update(None, None).unwrap();

    let wallet_data = wallet.public_data().unwrap();

    assert!(wallet_data.name.is_none());
    assert!(wallet_data.caption.is_none());
    assert!(!db.contains(&keys::wallet_name()).unwrap());
    assert!(!db.contains(&keys::wallet_caption()).unwrap());
}

#[test]
fn test_update_wallet_with_values() {
    let (wallet, db) = factories::wallet(None);
    let wallet_data = wallet.public_data().unwrap();

    assert!(wallet_data.name.is_none());
    assert!(wallet_data.caption.is_none());
    assert!(!db.contains(&keys::wallet_name()).unwrap());
    assert!(!db.contains(&keys::wallet_caption()).unwrap());

    let name = Some("wallet name".to_string());
    let caption = Some("wallet caption".to_string());

    wallet.update(name.clone(), caption.clone()).unwrap();

    let wallet_data = wallet.public_data().unwrap();

    assert_eq!(name, wallet_data.name);
    assert_eq!(caption, wallet_data.caption);
    assert_eq!(name, db.get_opt::<_, String>(&keys::wallet_name()).unwrap());
    assert_eq!(
        caption,
        db.get_opt::<_, String>(&keys::wallet_caption()).unwrap()
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
        .index_transactions(
            &a_block,
            &[factories::vtt_from_body(types::VTTransactionBody::new(
                vec![factories::Input::default().create()],
                vec![factories::VttOutput::default()
                    .with_pkh(our_address.pkh)
                    .with_value(2)
                    .create()],
            ))],
        )
        .unwrap();

    assert!(wallet.get_transaction(0, 0).is_ok());
    assert!(wallet.get_transaction(0, 1).is_err());

    // spend those funds to create a new transaction which is pending (it has no block)
    let vtt = wallet
        .create_vtt(types::VttParams {
            pkh: their_pkh,
            value: 1,
            fee: 0,
            time_lock: 0,
        })
        .unwrap();
    // the wallet does not store created VTT transactions until confirmation
    assert!(wallet.get_transaction(0, 1).is_err());

    // index another block confirming the previously created vtt
    wallet
        .index_transactions(&a_block, &[factories::vtt_from_body(vtt.body)])
        .unwrap();
    assert!(wallet.get_transaction(0, 1).is_ok());
}
