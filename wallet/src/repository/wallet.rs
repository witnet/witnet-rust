use std::sync::Mutex;

use bech32::ToBase32 as _;

use super::*;
use crate::{
    crypto,
    db::{Database, WriteBatch as _},
    model,
    params::Params,
    types,
};

pub struct Wallet<T> {
    db: T,
    params: Params,
    engine: types::SignEngine,
    gen_address_mutex: Mutex<()>,
}

impl<T> Wallet<T>
where
    T: Database,
{
    pub fn new(db: T, params: Params, engine: types::SignEngine) -> Self {
        Self {
            db,
            params,
            engine,
            gen_address_mutex: Mutex::new(()),
        }
    }

    pub fn data(&self) -> Result<types::WalletData> {
        let name: Option<String> = self.db.get_opt(keys::wallet_name())?;
        let caption: Option<String> = self.db.get_opt(keys::wallet_caption())?;
        let accounts: Vec<u32> = self.db.get(keys::wallet_accounts())?;
        let account: u32 = self.db.get(keys::wallet_default_account())?;

        let wallet = types::WalletData {
            name,
            caption,
            current_account: account,
            available_accounts: accounts,
        };

        Ok(wallet)
    }

    pub fn gen_address(&self, label: Option<String>) -> Result<model::Address> {
        let account_index: u32 = self.db.get(keys::wallet_default_account())?;
        let addresses_counter_key = keys::account_next_ek_index(account_index);
        let external_key: types::ExtendedSK = self.db.get(&keys::account_ek(account_index))?;
        // FIXME: Use a merge operator or rocksdb transaction when available in rocksdb crate
        let lock = self.gen_address_mutex.lock()?;
        let address_index: u32 = self.db.get_or_default(&addresses_counter_key)?;
        let address_next_index = address_index
            .checked_add(1)
            .ok_or_else(|| Error::IndexOverflow)?;
        self.db.put(addresses_counter_key, address_next_index)?;
        drop(lock);

        let extended_sk = external_key.derive(
            &self.engine,
            &types::KeyPath::default().index(address_index),
        )?;
        let types::ExtendedPK { key, .. } =
            types::ExtendedPK::from_secret_key(&self.engine, &extended_sk);

        let bytes = crypto::calculate_sha256(&key.serialize_uncompressed());
        let pkh = bytes.as_ref()[..20].to_vec();
        let address = bech32::encode(
            if self.params.testnet { "twit" } else { "wit" },
            pkh.to_base32(),
        )?;
        let path = format!("{}/0/{}", account_keypath(account_index), address_index);

        let mut batch = self.db.batch();

        batch.put(keys::address(account_index, address_index), &address)?;
        batch.put(keys::address_path(account_index, address_index), &path)?;
        if let Some(label) = &label {
            batch.put(keys::address_label(account_index, address_index), label)?;
        }

        self.db.write(batch)?;

        Ok(model::Address {
            address,
            path,
            label,
        })
    }

    pub fn addresses(&self, offset: u32, limit: u32) -> Result<model::Addresses> {
        let account_index: u32 = self.db.get(keys::wallet_default_account())?;
        let last_index: u32 = self
            .db
            .get_or_default(&keys::account_next_ek_index(account_index))?;

        let end = last_index.saturating_sub(offset);
        let start = end.saturating_sub(limit);
        let range = start..end;
        let mut addresses = Vec::with_capacity(range.len());

        for address_index in range.rev() {
            let address = self.db.get(&keys::address(account_index, address_index))?;
            let path = self
                .db
                .get(&keys::address_path(account_index, address_index))?;
            let label = self
                .db
                .get(&keys::address_label(account_index, address_index))?;

            addresses.push(model::Address {
                address,
                path,
                label,
            });
        }

        Ok(model::Addresses {
            addresses,
            total: last_index,
        })
    }

    pub fn db_get(&self, key: &str) -> Result<Option<String>> {
        let value = self.db.get_opt(&keys::custom(key))?;

        Ok(value)
    }

    pub fn db_set(&self, key: &str, value: &str) -> Result<()> {
        self.db.put(&keys::custom(key), value)?;

        Ok(())
    }
}

#[inline]
fn account_keypath(index: u32) -> types::KeyPath {
    types::KeyPath::default()
        .hardened(3)
        .hardened(4919)
        .hardened(index)
}
