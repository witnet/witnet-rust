use std::sync::Arc;

use super::*;

#[derive(Clone)]
pub struct Wallet {
    db: EncryptedDb,
    name: String,
}

impl Wallet {}
