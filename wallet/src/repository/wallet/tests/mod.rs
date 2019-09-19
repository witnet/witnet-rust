use std::{collections::HashMap, iter::FromIterator as _, mem};

use super::*;
use crate::*;

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
    assert_eq!(Some(label), address.label);

    let address_no_label = wallet.gen_external_address(None).unwrap();

    assert_eq!(None, address_no_label.label);
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
    assert_eq!(
        label,
        db.get::<_, String>(&keys::address_label(account, keychain, index))
            .unwrap()
    );
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
    assert_eq!(Some(label.clone()), address.label);

    let address_no_label = wallet.gen_internal_address(None).unwrap();

    assert_eq!(None, address_no_label.label);
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
        db.get::<_, String>(&keys::address_label(account, keychain, index))
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
    let value = 1;
    let fee = 0;
    let pkh = factories::pkh();
    let time_lock = 0;
    let err = wallet
        .create_transaction_components(value, fee, Some((pkh, time_lock)))
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
    let pkh = factories::pkh();
    let value = 1;
    let fee = 0;
    let time_lock = 0;
    let vtt = wallet
        .create_transaction_components(value, fee, Some((pkh, time_lock)))
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
    let utxo_set: HashMap<model::OutPtr, model::KeyBalance> = HashMap::from_iter(vec![(
        out_pointer.clone(),
        model::KeyBalance { pkh, amount: 2 },
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
    let pkh = factories::pkh();
    let value = 1;
    let fee = 0;
    let out_pointer = model::OutPtr {
        txn_hash: vec![0; 32],
        output_index: 0,
    };
    let time_lock = 0;
    let vtt = wallet
        .create_transaction_components(value, fee, Some((pkh, time_lock)))
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
        (keys::pkh(&pkh), bincode::serialize(&path).unwrap()),
    ]);

    let (wallet, _db) = factories::wallet(Some(db));
    let pkh = factories::pkh();
    let value = std::u64::MAX;
    let fee = 0;
    let time_lock = 0;
    let err = wallet
        .create_transaction_components(value, fee, Some((pkh, time_lock)))
        .unwrap_err();

    assert_eq!(
        mem::discriminant(&repository::Error::TransactionValueOverflow),
        mem::discriminant(&err)
    );
}

#[test]
fn test_create_vtt_spends_utxos() {
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

    let (wallet, db) = factories::wallet(Some(db));
    let pkh = factories::pkh();
    let value = 1;
    let fee = 0;
    let time_lock = 0;

    let state_utxo_set = wallet.utxo_set().unwrap();
    let utxo_set: HashMap<model::OutPtr, model::KeyBalance> =
        db.get(&keys::account_utxo_set(0)).unwrap();

    assert!(utxo_set.contains_key(&out_pointer));
    assert!(state_utxo_set.contains_key(&out_pointer));

    let vtt = wallet
        .create_vtt(types::VttParams {
            pkh,
            value,
            fee,
            label: None,
            time_lock,
        })
        .unwrap();

    let state_utxo_set = wallet.utxo_set().unwrap();
    let new_utxo_set: HashMap<model::OutPtr, model::KeyBalance> =
        db.get(&keys::account_utxo_set(0)).unwrap();

    assert!(!new_utxo_set.contains_key(&out_pointer));
    assert!(!state_utxo_set.contains_key(&out_pointer));

    assert!(db.contains(&keys::transaction_timestamp(0, 0)).unwrap());
    assert!(db
        .contains(&keys::transaction(&hex::encode(vtt.hash().as_ref())))
        .unwrap());
    assert_eq!(
        value,
        db.get::<_, u64>(&keys::transaction_value(0, 0)).unwrap()
    );
    assert_eq!(
        vtt.hash().as_ref(),
        db.get::<_, Vec<u8>>(&keys::transaction_hash(0, 0))
            .unwrap()
            .as_slice()
    );
    assert_eq!(fee, db.get::<_, u64>(&keys::transaction_fee(0, 0)).unwrap());
    assert_eq!(
        None,
        db.get_opt::<_, u64>(&keys::transaction_block(0, 0))
            .unwrap()
    );
    assert_eq!(
        mem::discriminant(&model::TransactionKind::Debit),
        mem::discriminant(
            &db.get::<_, model::TransactionKind>(&keys::transaction_type(0, 0))
                .unwrap()
        )
    );
    assert_eq!(
        0,
        db.get::<_, u32>(&keys::transactions_index(vtt.hash().as_ref()))
            .unwrap()
    );
}

#[test]
fn test_create_data_request_spends_utxos() {
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

    let (wallet, db) = factories::wallet(Some(db));

    let state_utxo_set = wallet.utxo_set().unwrap();
    let utxo_set: HashMap<model::OutPtr, model::KeyBalance> =
        db.get(&keys::account_utxo_set(0)).unwrap();

    assert!(utxo_set.contains_key(&out_pointer));
    assert!(state_utxo_set.contains_key(&out_pointer));

    let request = types::DataRequestOutput {
        data_request: Default::default(),
        value: 1,
        witnesses: 1,
        backup_witnesses: 0,
        commit_fee: 0,
        reveal_fee: 0,
        tally_fee: 0,
    };

    let data_req = wallet
        .create_data_req(types::DataReqParams {
            label: None,
            request,
        })
        .unwrap();

    let state_utxo_set = wallet.utxo_set().unwrap();
    let new_utxo_set: HashMap<model::OutPtr, model::KeyBalance> =
        db.get(&keys::account_utxo_set(0)).unwrap();

    assert!(!new_utxo_set.contains_key(&out_pointer));
    assert!(!state_utxo_set.contains_key(&out_pointer));

    assert!(db.contains(&keys::transaction_timestamp(0, 0)).unwrap());
    assert!(db
        .contains(&keys::transaction(&hex::encode(data_req.hash().as_ref())))
        .unwrap());
    assert_eq!(1, db.get::<_, u64>(&keys::transaction_value(0, 0)).unwrap());
    assert_eq!(
        data_req.hash().as_ref(),
        db.get::<_, Vec<u8>>(&keys::transaction_hash(0, 0))
            .unwrap()
            .as_slice()
    );
    assert_eq!(0, db.get::<_, u64>(&keys::transaction_fee(0, 0)).unwrap());
    assert_eq!(
        None,
        db.get_opt::<_, u64>(&keys::transaction_block(0, 0))
            .unwrap()
    );
    assert_eq!(
        mem::discriminant(&model::TransactionKind::Debit),
        mem::discriminant(
            &db.get::<_, model::TransactionKind>(&keys::transaction_type(0, 0))
                .unwrap()
        )
    );
    assert_eq!(
        0,
        db.get::<_, u32>(&keys::transactions_index(data_req.hash().as_ref()))
            .unwrap()
    );
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
    let txn = types::VTTransactionBody::new(inputs, outputs);

    wallet.index_transactions(&block, &[txn]).unwrap();

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

    wallet.index_transactions(&a_block, &[txn1]).unwrap();
    wallet.index_transactions(&a_block, &[txn2]).unwrap();

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

    wallet.index_transactions(&block, &[txn.clone()]).unwrap();
    wallet.index_transactions(&block, &[txn]).unwrap();

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

    let err = wallet.index_transactions(&block, &[txn]).unwrap_err();

    assert_eq!(
        mem::discriminant(&repository::Error::BalanceOverflow),
        mem::discriminant(&err)
    );
}

#[test]
fn test_index_transaction_vtt_created_by_wallet() {
    let (wallet, db) = factories::wallet(None);

    let a_block = factories::BlockInfo::default().create();
    let our_address = wallet.gen_external_address(None).unwrap();
    let their_address = wallet.gen_external_address(None).unwrap();

    // index transaction to receive funds
    wallet
        .index_transactions(
            &a_block,
            &[types::VTTransactionBody::new(
                vec![factories::Input::default().create()],
                vec![factories::VttOutput::default()
                    .with_pkh(our_address.pkh)
                    .with_value(2)
                    .create()],
            )],
        )
        .unwrap();

    // spend those funds to create a new transaction which is pending (it has no block)
    let vtt = wallet
        .create_vtt(types::VttParams {
            pkh: their_address.pkh,
            value: 1,
            fee: 0,
            label: None,
            time_lock: 0,
        })
        .unwrap();

    // check that indeed, the previously created vtt has no block associated with it
    assert_eq!(
        None,
        db.get_opt::<_, model::BlockInfo>(&keys::transaction_block(0, 1))
            .unwrap()
    );

    // index another block confirming the previously created vtt
    wallet.index_transactions(&a_block, &[vtt.body]).unwrap();

    // check that indeed, the previously created vtt now has a block associated with it
    assert_eq!(
        Some(a_block),
        db.get_opt::<_, model::BlockInfo>(&keys::transaction_block(0, 1))
            .unwrap()
    );
}
