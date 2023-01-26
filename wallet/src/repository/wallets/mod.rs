use std::sync::Mutex;

use super::*;
use crate::{
    constants,
    db::{Database, WriteBatch as _},
    model, types,
};

#[cfg(test)]
mod tests;

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

    /// Retrieve public information of wallets stored in the wallets DB
    pub fn infos(&self) -> Result<Vec<model::Wallet>> {
        let ids: Vec<String> = self.db.get_or_default(&keys::wallet_ids())?;
        let mut wallets = Vec::with_capacity(ids.len());

        for id in ids {
            let name = self.db.get_opt(&keys::wallet_id_name(&id))?;

            wallets.push(model::Wallet { id, name })
        }

        Ok(wallets)
    }

    /// Update a wallet's public info in the wallets db .
    pub fn update_info(&self, id: &str, name: Option<String>) -> Result<()> {
        let mut batch = self.db.batch();

        if let Some(name) = name {
            batch.put(&keys::wallet_id_name(id), name)?;
        }

        self.db.write(batch)?;

        Ok(())
    }

    /// Create a wallet based on name, description, IV, salt and account. The name is stored in the
    /// public wallets DB, while all parameters are stored in the private encrypted wallet DB
    pub fn create<D: Database>(
        &self,
        wallet_db: &D,
        wallet_data: types::CreateWalletData<'_>,
    ) -> Result<()> {
        let types::CreateWalletData {
            id,
            name,
            description,
            iv,
            salt,
            account,
            master_key,
            birth_date,
        } = wallet_data;
        let mut batch = self.db.batch();
        let mut wbatch = wallet_db.batch();

        if let Some(master_key) = master_key {
            wbatch.put(&keys::master_key(), master_key)?;
        }

        // We first write name and description into private wallet DB
        if let Some(name) = name.as_ref() {
            wbatch.put(&keys::wallet_name(), name.clone())?;
            batch.put(&keys::wallet_id_name(id), name.clone())?;
        }

        if let Some(description) = description {
            wbatch.put(&keys::wallet_description(), description)?;
        }

        wbatch.put(&keys::wallet_default_account(), account.index)?;
        wbatch.put(
            &keys::account_key(account.index, constants::EXTERNAL_KEYCHAIN),
            &account.external,
        )?;
        wbatch.put(
            &keys::account_key(account.index, constants::INTERNAL_KEYCHAIN),
            &account.internal,
        )?;

        wbatch.put(&keys::birth_date(), birth_date)?;
        wbatch.put(&keys::wallet_last_sync(), birth_date)?;

        wallet_db.write(wbatch)?;

        batch.put(&keys::wallet_id_salt(id), &salt)?;
        batch.put(&keys::wallet_id_iv(id), &iv)?;

        // FIXME: Use merge operator or a transaction when available in rocksdb crate
        let wallet_id = id.to_string();
        let lock = self.wallets_mutex.lock()?;
        let mut ids: Vec<String> = self.db.get_or_default(&keys::wallet_ids())?;
        if !ids.contains(&wallet_id) {
            ids.push(wallet_id);
            batch.put(&keys::wallet_ids(), ids)?;
        }
        self.db.write(batch)?;
        drop(lock);

        Ok(())
    }

    /// Delete a wallet by its ID
    pub fn delete(&self, wallet_id: String) -> Result<()> {
        let mut batch = self.db.batch();
        let lock = self.wallets_mutex.lock()?;
        let mut ids: Vec<String> = self.db.get_or_default(&keys::wallet_ids())?;

        if let Some(index) = ids.iter().position(|x| x == &wallet_id) {
            ids.remove(index);
            batch.put(&keys::wallet_ids(), ids)?;
        }

        self.db.write(batch)?;
        drop(lock);

        Ok(())
    }

    /// Get a wallet's salt and IV based on its provided ID
    pub fn wallet_salt_and_iv(&self, id: &str) -> Result<(Vec<u8>, Vec<u8>)> {
        let ids: Vec<String> = self.db.get_or_default(&keys::wallet_ids())?;
        // This check is necessary because without it, a deleted wallet could be unlocked
        if !ids.contains(&id.to_string()) {
            Err(Error::WalletNotFound)
        } else {
            let salt = self.db.get(&keys::wallet_id_salt(id))?;
            let iv = self.db.get(&keys::wallet_id_iv(id))?;

            Ok((salt, iv))
        }
    }
}
