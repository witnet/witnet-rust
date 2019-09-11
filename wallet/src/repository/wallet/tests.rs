use std::{cell::RefCell, collections::HashMap, iter::FromIterator as _, mem, rc::Rc};

use super::*;
use crate::*;

#[test]
fn test_wallet_public_data() {
    let (wallet, _db) = wallet_factory(None);
    let data = wallet.public_data().unwrap();

    assert!(data.name.is_none());
    assert!(data.caption.is_none());
    assert_eq!(0, data.balance);
    assert_eq!(0, data.current_account);
    assert_eq!(vec![0], data.available_accounts);
}

#[test]
fn test_gen_external_address() {
    let (wallet, _db) = wallet_factory(None);
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
    let (wallet, _db) = wallet_factory(None);
    let address = wallet.gen_external_address(None).unwrap();

    assert_eq!("m/3'/4919'/0'/0/0", &address.path);
    assert_eq!(0, address.index);

    let new_address = wallet.gen_external_address(None).unwrap();

    assert_eq!("m/3'/4919'/0'/0/1", &new_address.path);
    assert_eq!(1, new_address.index);
}

#[test]
fn test_gen_external_address_stores_next_address_index_in_db() {
    let (wallet, db) = wallet_factory(None);
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
    let (wallet, db) = wallet_factory(None);
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
    let (wallet, db) = wallet_factory(None);
    let account = 0;
    let keychain = constants::EXTERNAL_KEYCHAIN;
    let address = wallet.gen_external_address(None).unwrap();
    let pkh = &address.pkh;

    let path: model::Path = db.get(pkh).unwrap();

    assert_eq!(account, path.account);
    assert_eq!(keychain, path.keychain);
    assert_eq!(0, path.index);
}

#[test]
fn test_list_external_addresses() {
    let (wallet, _db) = wallet_factory(None);

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
    let (wallet, _db) = wallet_factory(None);

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
    let (wallet, _db) = wallet_factory(None);
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
    let (wallet, _db) = wallet_factory(None);
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
    let (wallet, _db) = wallet_factory(None);
    let address = wallet.gen_internal_address(None).unwrap();

    assert_eq!("m/3'/4919'/0'/1/0", &address.path);
    assert_eq!(0, address.index);

    let new_address = wallet.gen_internal_address(None).unwrap();

    assert_eq!("m/3'/4919'/0'/1/1", &new_address.path);
    assert_eq!(1, new_address.index);
}

#[test]
fn test_gen_internal_address_stores_next_address_index_in_db() {
    let (wallet, db) = wallet_factory(None);
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
    let (wallet, db) = wallet_factory(None);
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
    let (wallet, db) = wallet_factory(None);
    let account = 0;
    let keychain = constants::INTERNAL_KEYCHAIN;
    let address = wallet.gen_internal_address(None).unwrap();
    let pkh = &address.pkh;

    let path: model::Path = db.get(pkh).unwrap();

    assert_eq!(account, path.account);
    assert_eq!(keychain, path.keychain,);
    assert_eq!(0, path.index);
}

#[test]
fn test_custom_kv() {
    let (wallet, _db) = wallet_factory(None);

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
    let (wallet, _db) = wallet_factory(None);

    assert_eq!(0, wallet.balance().unwrap().amount);

    let mut db = HashMap::new();
    db.insert(
        keys::account_balance(0).as_bytes().to_vec(),
        bincode::serialize(&99u64).unwrap(),
    );

    let (wallet, _db) = wallet_factory(Some(db));

    assert_eq!(99, wallet.balance().unwrap().amount);
}

#[test]
fn test_create_vtt_components_when_wallet_have_no_utxos() {
    let (wallet, _db) = wallet_factory(None);
    let value = 1;
    let fee = 0;
    let pkh = pkh_factory();
    let err = wallet.create_vtt_components(pkh, value, fee).unwrap_err();

    assert_eq!(
        mem::discriminant(&repository::Error::InsufficientBalance),
        mem::discriminant(&err)
    );
}

#[test]
fn test_create_vtt_components_without_a_change_address() {
    let pkh = pkh_factory().as_ref().to_vec();
    let out_pointer = model::OutPtr {
        txn_hash: vec![0; 32],
        output_index: 0,
    };
    let utxo_set: HashMap<model::OutPtr, model::KeyBalance> = HashMap::from_iter(vec![(
        out_pointer.clone(),
        model::KeyBalance {
            pkh: pkh.clone(),
            amount: 1,
        },
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

    let (wallet, _db) = wallet_factory(Some(db));
    let pkh = pkh_factory();
    let value = 1;
    let fee = 0;
    let vtt = wallet.create_vtt_components(pkh, value, fee).unwrap();

    assert_eq!(1, vtt.value);
    assert_eq!(0, vtt.change);
    assert_eq!(vec![out_pointer], vtt.used);
    assert_eq!(1, vtt.sign_keys.len());
    assert_eq!(1, vtt.inputs.len());
    assert_eq!(1, vtt.outputs.len());
}

#[test]
fn test_create_vtt_components_whith_a_change_address() {
    let pkh = pkh_factory().as_ref().to_vec();
    let out_pointer = model::OutPtr {
        txn_hash: vec![0; 32],
        output_index: 0,
    };
    let utxo_set: HashMap<model::OutPtr, model::KeyBalance> = HashMap::from_iter(vec![(
        out_pointer.clone(),
        model::KeyBalance {
            pkh: pkh.clone(),
            amount: 2,
        },
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

    let (wallet, _db) = wallet_factory(Some(db));
    let pkh = pkh_factory();
    let value = 1;
    let fee = 0;
    let out_pointer = model::OutPtr {
        txn_hash: vec![0; 32],
        output_index: 0,
    };
    let vtt = wallet.create_vtt_components(pkh, value, fee).unwrap();

    assert_eq!(1, vtt.value);
    assert_eq!(1, vtt.change);
    assert_eq!(vec![out_pointer], vtt.used);
    assert_eq!(1, vtt.sign_keys.len());
    assert_eq!(1, vtt.inputs.len());
    assert_eq!(2, vtt.outputs.len());
}

#[test]
fn test_create_vtt_components_which_value_overflows() {
    let pkh = pkh_factory().as_ref().to_vec();
    let utxo_set: HashMap<model::OutPtr, model::KeyBalance> = HashMap::from_iter(vec![
        (
            model::OutPtr {
                txn_hash: vec![0; 32],
                output_index: 0,
            },
            model::KeyBalance {
                pkh: pkh.clone(),
                amount: 2,
            },
        ),
        (
            model::OutPtr {
                txn_hash: vec![0; 32],
                output_index: 1,
            },
            model::KeyBalance {
                pkh: pkh.clone(),
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

    let (wallet, _db) = wallet_factory(Some(db));
    let pkh = pkh_factory();
    let value = std::u64::MAX;
    let fee = 0;
    let err = wallet.create_vtt_components(pkh, value, fee).unwrap_err();

    assert_eq!(
        mem::discriminant(&repository::Error::TransactionValueOverflow),
        mem::discriminant(&err)
    );
}

#[test]
fn test_create_vtt_spends_utxos() {
    let pkh = pkh_factory().as_ref().to_vec();
    let out_pointer = model::OutPtr {
        txn_hash: vec![0; 32],
        output_index: 0,
    };
    let utxo_set: HashMap<model::OutPtr, model::KeyBalance> = HashMap::from_iter(vec![(
        out_pointer.clone(),
        model::KeyBalance {
            pkh: pkh.clone(),
            amount: 1,
        },
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

    let (wallet, db) = wallet_factory(Some(db));
    let pkh = pkh_factory();
    let value = 1;
    let fee = 0;

    let state_utxo_set = wallet.utxo_set().unwrap();
    let utxo_set: HashMap<model::OutPtr, model::KeyBalance> =
        db.get(&keys::account_utxo_set(0)).unwrap();

    assert!(utxo_set.contains_key(&out_pointer));
    assert!(state_utxo_set.contains_key(&out_pointer));

    wallet
        .create_vtt(types::VttParams {
            pkh,
            value,
            fee,
            label: None,
        })
        .unwrap();

    let state_utxo_set = wallet.utxo_set().unwrap();
    let new_utxo_set: HashMap<model::OutPtr, model::KeyBalance> =
        db.get(&keys::account_utxo_set(0)).unwrap();

    assert!(!new_utxo_set.contains_key(&out_pointer));
    assert!(!state_utxo_set.contains_key(&out_pointer));
}

/// Create an empty Wallet repository and return it along with its
/// underlying database so the tests can spy underneath it.
fn wallet_factory(
    data: Option<HashMap<Vec<u8>, Vec<u8>>>,
) -> (Wallet<db::HashMapDb>, db::HashMapDb) {
    let params = params::Params::default();
    let mnemonic = types::MnemonicGen::new()
        .with_len(types::MnemonicLength::Words12)
        .generate();
    let source = types::SeedSource::Mnemonics(mnemonic);
    let master_key = crypto::gen_master_key(
        params.seed_password.as_ref(),
        params.master_key_salt.as_ref(),
        &source,
    )
    .unwrap();
    let engine = types::SignEngine::signing_only();
    let default_account_index = 0;
    let default_account =
        account::gen_account(&engine, default_account_index, &master_key).unwrap();

    let mut rng = rand::rngs::OsRng;
    let salt = crypto::salt(&mut rng, params.db_salt_length);
    let iv = crypto::salt(&mut rng, params.db_iv_length);

    let storage = Rc::new(RefCell::new(data.unwrap_or_default()));
    let db = db::HashMapDb::new(storage);
    let wallets = Wallets::new(db.clone());

    // Create the initial data required by the wallet
    wallets
        .create(
            &db,
            types::CreateWalletData {
                iv,
                salt,
                id: "test-wallet-id",
                name: None,
                caption: None,
                account: &default_account,
            },
        )
        .unwrap();

    let wallet = Wallet::unlock(db.clone(), params, engine).unwrap();

    (wallet, db)
}

fn pkh_factory() -> types::PublicKeyHash {
    types::PublicKeyHash::from_bytes(&[0; 20]).expect("PKH of 20 bytes failed")
}
