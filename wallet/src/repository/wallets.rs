use std::sync::Mutex;

use super::*;
use crate::{
    db::{Database, WriteBatch as _},
    model, types,
};

pub struct Wallets<T> {
    db: T,
    wallets_mutex: Mutex<()>,
}

impl<T: Database> Wallets<T> {
    pub fn new(db: T) -> Self {
        Self {
            db,
            wallets_mutex: Default::default(),
        }
    }

    pub fn flush_db(&self) -> Result<()> {
        self.db.flush()?;

        Ok(())
    }

    pub fn infos(&self) -> Result<Vec<model::Wallet>> {
        let ids: Vec<String> = self.db.get_or_default(keys::wallet_ids())?;
        let mut wallets = Vec::with_capacity(ids.len());

        for id in ids {
            let name = self.db.get_opt(&keys::wallet_id_name(&id))?;
            let caption = self.db.get_opt(&keys::wallet_id_caption(&id))?;

            wallets.push(model::Wallet { id, name, caption })
        }

        Ok(wallets)
    }

    pub fn create<D: Database>(
        &self,
        wallet_db: D,
        id: &str,
        name: Option<String>,
        caption: Option<String>,
        iv: Vec<u8>,
        salt: Vec<u8>,
        account: &types::Account,
    ) -> Result<()> {
        let mut wbatch = wallet_db.batch();

        if let Some(name) = name {
            wbatch.put(keys::wallet_name(), name)?;
        }
        if let Some(caption) = caption {
            wbatch.put(keys::wallet_caption(), caption)?;
        }
        wbatch.put(keys::wallet_default_account(), account.index)?;
        wbatch.put(keys::account_ek(account.index), &account.external)?;
        wbatch.put(keys::account_ik(account.index), &account.internal)?;
        wbatch.put(keys::account_rk(account.index), &account.rad)?;

        wallet_db.write(wbatch)?;

        let mut batch = self.db.batch();
        batch.put(keys::wallet_id_salt(&id), &salt)?;
        batch.put(keys::wallet_id_iv(&id), &iv)?;

        // // FIXME: Use merge operator or a transaction when available in rocksdb crate
        let wallet_id = id.to_string();
        let lock = self.wallets_mutex.lock()?;
        let mut ids: Vec<String> = self.db.get_or_default(&keys::wallet_ids())?;
        if !ids.contains(&wallet_id) {
            ids.push(wallet_id);
            batch.put(keys::wallet_ids(), ids)?;
            self.db.write(batch)?;
        }
        drop(lock);

        Ok(())
    }

    pub fn wallet_salt_and_iv(&self, id: &str) -> Result<(Vec<u8>, Vec<u8>)> {
        let salt = self.db.get(&keys::wallet_id_salt(id))?;
        let iv = self.db.get(&keys::wallet_id_iv(id))?;

        Ok((salt, iv))
    }
}
