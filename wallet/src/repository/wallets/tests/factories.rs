use std::{cell::RefCell, rc::Rc};

use super::*;

pub fn wallets(data: Option<HashMap<Vec<u8>, Vec<u8>>>) -> (Wallets<db::HashMapDb>, db::HashMapDb) {
    let storage = Rc::new(RefCell::new(data.unwrap_or_default()));
    let db = db::HashMapDb::new(storage);
    let wallets = Wallets::new(db.clone());

    (wallets, db)
}
