use std::collections::HashMap;

use super::*;
use crate::*;

mod factories;

#[test]
fn test_wallet_infos_when_no_wallets() {
    let (wallets, _db) = factories::wallets(None);

    let infos = wallets.infos().unwrap();

    assert!(infos.is_empty());
}

#[test]
fn test_wallet_infos_when_wallets() {
    let (wallets, db) = factories::wallets(Some(HashMap::new()));
    db.put(&keys::wallet_ids(), vec!["a-wallet-id".to_string()])
        .unwrap();

    let infos = wallets.infos().unwrap();

    assert_eq!(1, infos.len());
}

#[test]
fn test_update_wallet_info() {
    let id = "a-wallet-id".to_string();
    let (wallets, db) = factories::wallets(Some(HashMap::new()));
    db.put(&keys::wallet_ids(), vec![id.clone()]).unwrap();

    let wallet_info = &wallets.infos().unwrap()[0];

    assert!(wallet_info.name.is_none());
    assert!(!db.contains(&keys::wallet_id_name(&id)).unwrap());

    let name = Some("Testing".to_string());

    wallets.update_info(&id, name.clone()).unwrap();

    let wallet_info = &wallets.infos().unwrap()[0];

    assert_eq!(name, wallet_info.name);
    assert_eq!(name, db.get_opt(&keys::wallet_id_name(&id)).unwrap());
}
