use super::*;

#[derive(Clone)]
pub struct Wallets {
    db: Db,
}

impl Wallets {
    pub fn new(db: Db) -> Self {
        Self { db }
    }

    pub fn get_wallet_infos(&self) -> Result<Vec<WalletInfo>> {
        self.db.get_or_default(keys::wallet_ids())
    }
}
