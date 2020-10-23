use std::collections::HashMap;
use std::iter::FromIterator;

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
    let data = HashMap::from_iter(vec![(
        keys::wallet_ids().as_bytes().to_vec(),
        bincode::serialize(&vec!["a-wallet-id".to_string()]).unwrap(),
    )]);
    let (wallets, _db) = factories::wallets(Some(data));

    let infos = wallets.infos().unwrap();

    assert_eq!(1, infos.len());
}

#[test]
fn test_update_wallet_info() {
    let id = "a-wallet-id".to_string();
    let data = HashMap::from_iter(vec![(
        keys::wallet_ids().as_bytes().to_vec(),
        bincode::serialize(&vec![id.clone()]).unwrap(),
    )]);
    let (wallets, db) = factories::wallets(Some(data));

    let wallet_info = &wallets.infos().unwrap()[0];

    assert!(wallet_info.name.is_none());
    assert!(wallet_info.description.is_none());
    assert!(!db.contains(&keys::wallet_id_name(&id)).unwrap());
    assert!(!db.contains(&keys::wallet_id_description(&id)).unwrap());

    let name = Some("Testing".to_string());
    let description = Some("A testing wallet".to_string());

    wallets
        .update_info(&id, name.clone(), description.clone())
        .unwrap();

    let wallet_info = &wallets.infos().unwrap()[0];

    assert_eq!(name, wallet_info.name);
    assert_eq!(description, wallet_info.description);
    assert_eq!(
        name,
        db.get_opt::<_, String>(&keys::wallet_id_name(&id)).unwrap()
    );
    assert_eq!(
        description,
        db.get_opt::<_, String>(&keys::wallet_id_description(&id))
            .unwrap()
    );
}
