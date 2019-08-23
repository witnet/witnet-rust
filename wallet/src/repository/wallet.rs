use std::collections::HashMap;
use std::ops::Deref as _;
use std::sync::{Mutex, RwLock};

use bech32::ToBase32 as _;

use super::*;
use crate::types::Hashable as _;
use crate::{
    crypto,
    db::{Database, WriteBatch as _},
    model,
    params::Params,
    types,
};

type AccountIndex = u32;
type TransactionId = u32;
type Balance = u64;
type Pkh = Vec<u8>;
type Index = u32;
type Utxo = (Pkh, Index);

pub struct Wallet<T> {
    db: T,
    params: Params,
    engine: types::SignEngine,
    gen_address_mutex: Mutex<()>,
    /// Current account being used by the client.
    current_account: RwLock<u32>,
    /// Number of transactions per account
    transactions_count: RwLock<HashMap<AccountIndex, TransactionId>>,
    /// Account balances for the wallet
    account_balances: RwLock<HashMap<AccountIndex, Balance>>,
    /// Map pkh -> account index
    pkhs: RwLock<HashMap<Pkh, AccountIndex>>,
    /// Map account index -> utxo set, which maps output pointer -> value
    utxo_set: RwLock<HashMap<AccountIndex, HashMap<Utxo, Balance>>>,
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
            current_account: Default::default(),
            gen_address_mutex: Default::default(),
            transactions_count: Default::default(),
            account_balances: Default::default(),
            pkhs: Default::default(),
            utxo_set: Default::default(),
        }
    }

    pub fn unlock(&self) -> Result<types::WalletData> {
        let name: Option<String> = self.db.get_opt(keys::wallet_name())?;
        let caption: Option<String> = self.db.get_opt(keys::wallet_caption())?;
        let account: u32 = self
            .db
            .get_opt(keys::wallet_default_account())?
            .unwrap_or(*self.current_account.read()?);
        let accounts: Vec<u32> = self
            .db
            .get_opt(keys::wallet_accounts())?
            .unwrap_or_else(|| vec![account]);
        let wallet_pkhs: HashMap<Pkh, AccountIndex> = self.db.get_or_default(keys::wallet_pkhs())?;
        let wallet_utxo_set: HashMap<AccountIndex, HashMap<Utxo, Balance>> =
            self.db.get_or_default(keys::wallet_utxo_set())?;
        let wallet_transactions_count: HashMap<AccountIndex, TransactionId> =
            self.db.get_or_default(keys::wallet_transactions_count())?;
        let wallet_account_balances: HashMap<AccountIndex, Balance> =
            self.db.get_or_default(keys::wallet_account_balances())?;
        let balance = wallet_account_balances
            .get(&account)
            .cloned()
            .unwrap_or_else(|| 0);

        let mut current_account = self.current_account.write()?;
        *current_account = account;
        drop(current_account);

        let mut transactions_count = self.transactions_count.write()?;
        *transactions_count = wallet_transactions_count;
        drop(transactions_count);

        let mut account_balances = self.account_balances.write()?;
        *account_balances = wallet_account_balances;
        drop(account_balances);

        let mut pkhs = self.pkhs.write()?;
        *pkhs = wallet_pkhs;
        drop(pkhs);

        let mut utxo_set = self.utxo_set.write()?;
        *utxo_set = wallet_utxo_set;
        drop(utxo_set);

        let wallet = types::WalletData {
            name,
            caption,
            balance,
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

        let mut pkhs = self.pkhs.write()?;
        pkhs.insert(pkh, account_index);
        batch.put(keys::wallet_pkhs(), pkhs.deref())?;
        drop(pkhs);

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

    pub fn index_txns(&self, txns: &[types::VTTransactionBody]) -> Result<()> {
        let mut batch = self.db.batch();

        for txn in txns {
            let txn_hash = txn.hash().as_ref().to_vec();

            for input in &txn.inputs {
                let p = input.output_pointer();
                let pointed_txn_hash = p.transaction_id.as_ref().to_vec();
                let pointed_output_index = p.output_index;

                if let Some(account_index) =
                    self.db
                        .get_opt::<_, u32>(&keys::transaction_output_recipient(
                            &pointed_txn_hash,
                            pointed_output_index,
                        ))?
                {
                    let utxo_key = (pointed_txn_hash, pointed_output_index);

                    // remove the UTXO from the utxo set
                    let mut utxo_set = self.utxo_set.write()?;
                    let account_utxo_set = utxo_set
                        .get_mut(&account_index)
                        .expect("utxo set not found for account_index");
                    let value = match account_utxo_set.remove(&utxo_key) {
                        Some(value) => value,
                        None => Err(Error::NoUtxoForInput)?,
                    };
                    drop(utxo_set);

                    // record transaction for this account
                    let txn_id = self.next_transaction_id(account_index)?;
                    batch.put(&keys::transaction_value(account_index, txn_id), value)?;
                    batch.put(&keys::transaction_type(account_index, txn_id), "debit")?;

                    // update balance
                    self.update_account_balance(account_index, value, BalanceOp::Sub)?;
                }
            }

            for (output_index, output) in txn.outputs.iter().enumerate() {
                let pkh = output.pkh.as_ref();
                let value = output.value;

                if let Some(account_index) = self.pkhs.read()?.get(pkh).cloned() {
                    // add UTXO to the utxo set
                    let mut utxo_set = self.utxo_set.write()?;
                    let account_utxo_set = utxo_set
                        .get_mut(&account_index)
                        .expect("utxo set not found for account");
                    account_utxo_set.insert((txn_hash.clone(), output_index as u32), value);
                    drop(utxo_set);

                    // record transaction for this account
                    let txn_id = self.next_transaction_id(account_index)?;
                    batch.put(&keys::transaction_value(account_index, txn_id), value)?;
                    batch.put(&keys::transaction_type(account_index, txn_id), "credit")?;

                    self.db.put(
                        &keys::transaction_output_recipient(&txn_hash, output_index as u32),
                        account_index,
                    )?;

                    // update balance
                    self.update_account_balance(account_index, value, BalanceOp::Add)?;
                }
            }
        }

        // persist modified utxo set
        let utxo_set_guard = self.utxo_set.read()?;
        let utxo_set = utxo_set_guard.deref();
        self.db.put(keys::wallet_utxo_set(), utxo_set)?;

        // persist modified transactions count per account
        let transactions_count_guard = self.transactions_count.read()?;
        let transactions_count = transactions_count_guard.deref();
        self.db
            .put(keys::wallet_transactions_count(), transactions_count)?;

        // persist transactions
        self.db.write(batch)?;

        Ok(())
    }

    /// Retrieve the balance for the current wallet account.
    pub fn balance(&self) -> Result<(AccountIndex, Balance)> {
        let account = *self.current_account.read()?;
        let balance = self
            .account_balances
            .read()?
            .get(&account)
            .cloned()
            .expect("balance not found for account");

        Ok((account, balance))
    }

    fn next_transaction_id(&self, account_index: u32) -> Result<u32> {
        let transactions_count = self.transactions_count.write()?;
        let next_id = transactions_count
            .get(&account_index)
            .expect("transactions count not found for account index");
        let id = *next_id;

        next_id
            .checked_add(1)
            .ok_or_else(|| Error::TransactionIdOverflow)?;

        Ok(id)
    }

    fn update_account_balance(&self, account_index: u32, value: u64, op: BalanceOp) -> Result<()> {
        let mut account_balances = self.account_balances.write()?;
        let balance = account_balances
            .get_mut(&account_index)
            .expect("balance not found for account index");

        match op {
            BalanceOp::Add => {
                balance
                    .checked_add(value)
                    .ok_or_else(|| Error::BalanceOverflow)?;
            }
            BalanceOp::Sub => {
                balance
                    .checked_sub(1)
                    .ok_or_else(|| Error::BalanceUnderflow)?;
            }
        }

        Ok(())
    }
}

enum BalanceOp {
    Add,
    Sub,
}

#[inline]
fn account_keypath(index: u32) -> types::KeyPath {
    types::KeyPath::default()
        .hardened(3)
        .hardened(4919)
        .hardened(index)
}
