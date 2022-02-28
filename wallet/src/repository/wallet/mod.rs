use itertools::Itertools;
use std::{
    cmp::min,
    collections::{hash_map::Entry, HashMap, HashSet},
    convert::TryFrom,
    ops::Range,
    str::FromStr,
    sync::{Arc, RwLock, RwLockReadGuard},
};

use bech32::ToBase32;
use state::State;
use witnet_crypto::{
    hash::calculate_sha256,
    key::{CryptoEngine, ExtendedPK, ExtendedSK, KeyPath, PK},
    signature,
};
use witnet_data_structures::{
    chain::{
        CheckpointBeacon, DataRequestOutput, Environment, Epoch, EpochConstants, Hash, Hashable,
        Input, KeyedSignature, OutputPointer, PublicKeyHash, ValueTransferOutput,
    },
    get_environment,
    radon_error::RadonError,
    transaction::{
        DRTransaction, DRTransactionBody, TallyTransaction, Transaction, VTTransaction,
        VTTransactionBody,
    },
    transaction_factory::{insert_change_output, FeeType, OutputsCollection},
    utxo_pool::UtxoSelectionStrategy,
};
use witnet_rad::{error::RadError, types::RadonTypes};
use witnet_util::timestamp::get_timestamp;

use crate::{
    constants, crypto,
    db::{Database, WriteBatch as _},
    model,
    params::Params,
    types,
};

use super::*;

mod state;
#[cfg(test)]
mod tests;

/// Internal structure used to gather state mutations while indexing block transactions
struct AccountMutation {
    balance_movement: model::BalanceMovement,
    utxo_inserts: Vec<(model::OutPtr, model::OutputInfo)>,
    utxo_removals: Vec<model::OutPtr>,
}

/// Struct that keep the unspent outputs pool and the own unspent outputs pool
#[derive(Debug)]
pub struct WalletUtxos<'a> {
    pub utxo_set: &'a model::UtxoSet,
    pub used_outputs: &'a mut model::UsedOutputs,
    pub unconfirmed_transactions: &'a HashSet<Hash>,
    pub selected_utxos: HashSet<model::OutPtr>,
}

impl<'a> OutputsCollection for WalletUtxos<'a> {
    fn sort_by(&self, strategy: &UtxoSelectionStrategy) -> Vec<OutputPointer> {
        let filter_utxos = |out_ptr: &model::OutPtr| {
            let pointer: OutputPointer = out_ptr.into();

            if !self
                .unconfirmed_transactions
                .contains(&pointer.transaction_id)
                && (self.selected_utxos.contains(out_ptr) || self.selected_utxos.is_empty())
            {
                Some(pointer)
            } else {
                None
            }
        };

        match strategy {
            UtxoSelectionStrategy::BigFirst { from } => {
                sort_utxo_set(self.utxo_set, true, from.as_ref())
                    .filter_map(filter_utxos)
                    .collect()
            }
            UtxoSelectionStrategy::SmallFirst { from } => {
                sort_utxo_set(self.utxo_set, false, from.as_ref())
                    .filter_map(filter_utxos)
                    .collect()
            }
            UtxoSelectionStrategy::Random { from } => self
                .utxo_set
                .iter()
                .filter_map(|(o, info)| match from {
                    None => Some(o),
                    Some(from) => {
                        if from == &info.pkh {
                            Some(o)
                        } else {
                            None
                        }
                    }
                })
                .filter_map(filter_utxos)
                .collect(),
        }
    }

    fn get_time_lock(&self, outptr: &OutputPointer) -> Option<u64> {
        let time_lock = self.utxo_set.get(&outptr.into()).map(|vto| vto.time_lock);
        let time_lock_by_used = self.used_outputs.get(&outptr.into()).copied();

        // The most restrictive time_lock will be used to avoid UTXOs during a transaction creation
        match (time_lock, time_lock_by_used) {
            (Some(a), Some(b)) => Some(std::cmp::max(a, b)),
            (Some(a), None) => Some(a),
            (None, Some(b)) => Some(b),
            _ => None,
        }
    }

    fn get_value(&self, outptr: &OutputPointer) -> Option<u64> {
        self.utxo_set.get(&outptr.into()).map(|vto| vto.amount)
    }

    fn get_included_block_number(&self, _outptr: &OutputPointer) -> Option<u32> {
        None
    }

    fn set_used_output_pointer(&mut self, inputs: &[Input], ts: u64) {
        for input in inputs {
            self.used_outputs.insert(input.output_pointer().into(), ts);
        }
    }
}

/// Method to sort own_utxos by value
pub fn sort_utxo_set<'a>(
    utxo_set: &'a model::UtxoSet,
    bigger_first: bool,
    from: Option<&PublicKeyHash>,
) -> impl Iterator<Item = &'a model::OutPtr> + 'a {
    utxo_set
        .iter()
        .filter_map(|(o, info)| match from {
            None => Some((o, info)),
            Some(from) => {
                if from == &info.pkh {
                    Some((o, info))
                } else {
                    None
                }
            }
        })
        .sorted_by_key(|(_o, info)| {
            let value = i128::from(info.amount);

            if bigger_first {
                -value
            } else {
                value
            }
        })
        .map(|(o, _info)| o)
}

pub struct Wallet<T> {
    pub id: String,
    pub session_id: types::SessionId,
    db: T,
    params: Params,
    engine: CryptoEngine,
    state: RwLock<State>,
}

impl<T> Wallet<T>
where
    T: Database,
{
    /// Generate transient addresses for synchronization purposes
    /// This function only creates and inserts addresses
    pub fn initialize_transient_addresses(
        &self,
        external_addresses: u16,
        internal_addresses: u16,
    ) -> Result<()> {
        let mut state = self.state.write()?;

        let external_range =
            state.next_external_index..state.next_external_index + u32::from(external_addresses);
        let internal_range =
            state.next_internal_index..state.next_internal_index + u32::from(internal_addresses);

        self._generate_transient_address_ranges(&mut state, external_range, internal_range)
    }

    /// Non-locking transient address generation by defining the ranges for external and internal
    pub fn _generate_transient_address_ranges(
        &self,
        state: &mut State,
        external_range: Range<u32>,
        internal_range: Range<u32>,
    ) -> Result<()> {
        // Generate external addresses
        for index in external_range {
            let account = state.account;
            let keychain = constants::EXTERNAL_KEYCHAIN;
            let parent_key = &state.keychains[keychain as usize].clone();

            let (address, _) =
                self.derive_and_persist_address(None, parent_key, account, keychain, index, false)?;
            state
                .transient_external_addresses
                .insert(address.pkh, (*address).clone());
        }

        // Generate internal addresses
        for index in internal_range {
            let account = state.account;
            let keychain = constants::INTERNAL_KEYCHAIN;
            let parent_key = &state.keychains[keychain as usize].clone();

            let (address, _) =
                self.derive_and_persist_address(None, parent_key, account, keychain, index, false)?;
            state
                .transient_internal_addresses
                .insert(address.pkh, (*address).clone());
        }

        Ok(())
    }

    /// Clear the transient address generated for synchronization purposes
    pub fn clear_transient_addresses(&self) -> Result<()> {
        let mut state = self.state.write()?;

        self._clear_transient_addresses(&mut state)
    }

    /// Non-locking version of `clear_transient_addresses`
    pub fn _clear_transient_addresses(&self, state: &mut State) -> Result<()> {
        state.transient_internal_addresses.clear();
        state.transient_external_addresses.clear();

        Ok(())
    }

    /// Returns the bootstrap hash consensus constant
    pub fn get_bootstrap_hash(&self) -> Hash {
        self.params.genesis_prev_hash
    }

    /// Clears local pending wallet state to match the persisted state in database
    pub fn clear_pending_state(&self) -> Result<()> {
        let account = 0;

        let mut state = self.state.write()?;

        state.last_sync = state.last_confirmed;
        state.pending_blocks.clear();
        state.pending_movements.clear();
        state.pending_transactions.clear();
        state.pending_addresses_by_path.clear();
        state.pending_addresses_by_block.clear();
        state.local_movements.clear();
        state.db_movements_to_update.clear();

        // Restore state from database
        state.transaction_next_id = self
            .db
            .get_or_default(&keys::transaction_next_id(account))?;
        state.utxo_set = self.db.get_or_default(&keys::account_utxo_set(account))?;
        state.used_outputs = model::clean_used_outputs(&state.used_outputs, &state.utxo_set);
        state.balance.confirmed = self.db.get_or_default(&keys::account_balance(account))?;
        state.balance.unconfirmed = state.balance.confirmed;

        Ok(())
    }

    pub fn unlock(
        id: &str,
        session_id: types::SessionId,
        db: T,
        params: Params,
        engine: CryptoEngine,
    ) -> Result<Self> {
        let id = id.to_owned();
        let name = db.get_opt(&keys::wallet_name())?;
        let description = db.get_opt(&keys::wallet_description())?;
        let account = db.get_or_default(&keys::wallet_default_account())?;
        let available_accounts = db
            .get_opt(&keys::wallet_accounts())?
            .unwrap_or_else(|| vec![account]);

        let transaction_next_id = db.get_or_default(&keys::transaction_next_id(account))?;
        let utxo_set: model::UtxoSet = db.get_or_default(&keys::account_utxo_set(account))?;
        let timestamp =
            u64::try_from(get_timestamp()).expect("Get timestamp should return a positive value");
        let balance_info = db
            .get_opt(&keys::account_balance(account))?
            .unwrap_or_else(|| {
                // compute balance from utxo set if is not cached in the
                // database, this is mostly used for testing where overflow
                // checks are enabled
                utxo_set
                    .iter()
                    .map(|(_, balance)| (balance.amount, balance.time_lock))
                    .fold(
                        model::BalanceInfo::default(),
                        |mut acc, (amount, time_lock)| {
                            if timestamp >= time_lock {
                                acc.available =
                                    acc.available.checked_add(amount).expect("balance overflow");
                            } else {
                                acc.locked =
                                    acc.locked.checked_add(amount).expect("balance overflow");
                            }

                            acc
                        },
                    )
            });
        let balance = model::WalletBalance {
            local: 0,
            unconfirmed: balance_info,
            confirmed: balance_info,
        };

        let last_sync = db
            .get(&keys::wallet_last_sync())
            .unwrap_or(CheckpointBeacon {
                checkpoint: 0,
                hash_prev_block: params.genesis_prev_hash,
            });

        let last_confirmed = last_sync;

        let external_key = db.get(&keys::account_key(account, constants::EXTERNAL_KEYCHAIN))?;
        let next_external_index = db.get_or_default(&keys::account_next_index(
            account,
            constants::EXTERNAL_KEYCHAIN,
        ))?;
        let internal_key = db.get(&keys::account_key(account, constants::INTERNAL_KEYCHAIN))?;
        let next_internal_index = db.get_or_default(&keys::account_next_index(
            account,
            constants::INTERNAL_KEYCHAIN,
        ))?;
        let keychains = [external_key, internal_key];
        let epoch_constants = params.epoch_constants;
        let birth_date = db.get(&keys::birth_date()).unwrap_or(CheckpointBeacon {
            checkpoint: 0,
            hash_prev_block: params.genesis_prev_hash,
        });

        let state = RwLock::new(State {
            name,
            description,
            account,
            keychains,
            next_external_index,
            next_internal_index,
            available_accounts,
            balance,
            transaction_next_id,
            utxo_set,
            used_outputs: Default::default(),
            epoch_constants,
            last_sync,
            last_confirmed,
            local_movements: Default::default(),
            pending_movements: Default::default(),
            pending_transactions: Default::default(),
            pending_addresses_by_block: Default::default(),
            pending_addresses_by_path: Default::default(),
            pending_blocks: Default::default(),
            pending_dr_movements: Default::default(),
            db_movements_to_update: Default::default(),
            transient_external_addresses: Default::default(),
            transient_internal_addresses: Default::default(),
            stop_syncing: false,
            birth_date,
        });

        Ok(Self {
            id,
            session_id,
            db,
            params,
            engine,
            state,
        })
    }

    /// Return all non-sensitive data regarding the wallet.
    pub fn public_data(&self) -> Result<types::WalletData> {
        let state = self.state.read()?;
        let current_account = state.account;
        let balance = state.balance;
        let last_sync = state.last_sync;
        let last_confirmed = state.last_confirmed;
        let birth_date = state.birth_date;

        Ok(types::WalletData {
            id: self.id.clone(),
            name: state.name.clone(),
            description: state.description.clone(),
            balance,
            current_account,
            available_accounts: state.available_accounts.clone(),
            last_sync,
            last_confirmed,
            birth_date,
        })
    }

    /// Generic method for deriving an address and persist it in the DB.
    pub fn derive_and_persist_address(
        &self,
        label: Option<String>,
        parent_key: &ExtendedSK,
        account: u32,
        keychain: u32,
        index: u32,
        persist_db: bool,
    ) -> Result<(Arc<model::Address>, u32)> {
        let extended_sk = parent_key.derive(&self.engine, &KeyPath::default().index(index))?;
        let ExtendedPK { key, .. } = ExtendedPK::from_secret_key(&self.engine, &extended_sk);

        let pkh = witnet_data_structures::chain::PublicKey::from(key).pkh();
        let address = pkh.bech32(get_environment());
        let path = model::Path {
            account,
            keychain,
            index,
        }
        .to_string();
        let info = model::AddressInfo {
            label,
            received_payments: vec![],
            received_amount: 0,
            first_payment_date: None,
            last_payment_date: None,
        };

        let next_index = index.checked_add(1).ok_or(Error::IndexOverflow)?;
        if persist_db {
            // Persist changes and new address in database
            let mut batch = self.db.batch();

            batch.put(&keys::address(account, keychain, index), &address)?;
            batch.put(&keys::address_path(account, keychain, index), &path)?;
            batch.put(&keys::address_pkh(account, keychain, index), &pkh)?;
            batch.put(&keys::address_info(account, keychain, index), &info)?;
            batch.put(
                &keys::pkh(&pkh),
                &model::Path {
                    account,
                    keychain,
                    index,
                },
            )?;

            batch.put(&keys::account_next_index(account, keychain), &next_index)?;

            self.db.write(batch)?;
        }

        let address = model::Address {
            address,
            index,
            keychain,
            account,
            path,
            info,
            pkh,
        };

        Ok((Arc::new(address), next_index))
    }

    /// Set stop syncing flag to true
    pub fn set_stop_syncing(&self) -> Result<()> {
        let mut state = self.state.write()?;
        state.stop_syncing = true;

        Ok(())
    }

    /// Generate an address in the external keychain (WIP-0001).
    pub fn gen_external_address(&self, label: Option<String>) -> Result<Arc<model::Address>> {
        let mut state = self.state.write()?;

        self._gen_external_address(&mut state, label)
    }

    /// Generate an address in the internal keychain (WIP-0001).
    pub fn gen_internal_address(&self, label: Option<String>) -> Result<Arc<model::Address>> {
        let mut state = self.state.write()?;

        self._gen_internal_address(&mut state, label)
    }

    /// Return a list of the generated external addresses that.
    pub fn external_addresses(&self, offset: u32, limit: u32) -> Result<model::Addresses> {
        self.addresses(constants::EXTERNAL_KEYCHAIN, offset, limit)
    }

    /// Return a list of the generated internal addresses that.
    pub fn internal_addresses(&self, offset: u32, limit: u32) -> Result<model::Addresses> {
        self.addresses(constants::INTERNAL_KEYCHAIN, offset, limit)
    }

    /// Return a list of internal or external addresses.
    fn addresses(&self, keychain: u32, offset: u32, limit: u32) -> Result<model::Addresses> {
        let state = self.state.read()?;
        let account = state.account;
        let total = if keychain == constants::EXTERNAL_KEYCHAIN {
            state.next_external_index
        } else {
            state.next_internal_index
        };

        let end = total.saturating_sub(offset);
        let start = end.saturating_sub(limit);
        let range = start..end;
        let mut addresses = Vec::with_capacity(range.len());

        log::debug!(
            "Retrieving external addresses in range {:?}. Start({}), End({}), Total({})",
            range,
            start,
            end,
            total
        );
        for index in range.rev() {
            let address = self._get_address(&state, account, keychain, index)?;
            addresses.push((*address).clone());
        }

        Ok(model::Addresses { addresses, total })
    }

    /// Return a list of the transactions.
    pub fn transactions(&self, offset: u32, limit: u32) -> Result<model::WalletTransactions> {
        let state = self.state.read()?;
        let account = state.account;

        // Total amount of state and db transactions
        let total = state.transaction_next_id + u32::try_from(state.local_movements.len()).unwrap();
        let mut transactions: Vec<model::BalanceMovement> = Vec::new();

        // Query database `transaction_next_id` to compute total amount of transactions
        let db_total = self
            .db
            .get_or_default(&keys::transaction_next_id(account))?;

        // get number of non-repated pending movements.
        let pending_length = state
            .pending_movements
            .values()
            .fold(0, |acc: usize, x| acc.saturating_add(x.len()));
        let total_local = state.local_movements.len();

        // Lets get the ranges for pending and db transactions
        let (range_local, range_pending, range_db) = calculate_transaction_ranges(
            offset as usize,
            limit as usize,
            total_local,
            pending_length,
            db_total as usize,
        );

        // Append local movements if any
        if let Some(range_local) = range_local {
            // Append local pending balance movements (not yet included in blocks)
            let mut local_movements: Vec<model::BalanceMovement> =
                state.local_movements.values().cloned().collect();
            local_movements.sort_by(|a, b| a.db_key.cmp(&b.db_key));
            transactions.extend_from_slice(local_movements.drain(range_local).as_slice());
        }

        // Append balance movements of pending blocks
        if let Some(range_pending) = range_pending {
            // We need to order transaction by beacon
            let mut beacon_list: Vec<model::Beacon> = state
                .pending_blocks
                .values()
                .map(|state| state.beacon.clone())
                .collect();
            beacon_list.sort_by(|a, b| a.epoch.cmp(&b.epoch));

            // Get all pending movements in a vec
            let mut all_pending_movements: Vec<model::BalanceMovement> = vec![];
            beacon_list.iter().for_each(|beacon| {
                all_pending_movements.extend_from_slice(
                    state
                        .pending_movements
                        .get(&beacon.block_hash.to_string())
                        .unwrap_or(&vec![]),
                );
            });

            transactions.extend_from_slice(&all_pending_movements[range_pending]);
        }

        // Build a HashMap<transaction_index, balance_movement>
        let mut db_movements_to_update: HashMap<u32, model::BalanceMovement> = HashMap::new();
        state.db_movements_to_update.values().for_each(|movements| {
            db_movements_to_update.extend(movements.iter().map(|x| (x.db_key, x.clone())))
        });

        if let Some(range_db) = range_db {
            for index in range_db.rev() {
                let index = u32::try_from(index).unwrap();

                // Check if there is a pending update for the queried balance movement,
                // otherwise query the database
                if let Some(transaction) = db_movements_to_update.get(&index) {
                    log::debug!(
                        "Updating transaction {:?} with pending tally found",
                        transaction.transaction.hash
                    );
                    transactions.push(transaction.clone());
                } else {
                    match self.get_transaction(account, index) {
                        Ok(transaction) => {
                            transactions.push(transaction);
                        }
                        Err(e) => {
                            log::error!(
                                "Error while retrieving transaction with index {}: {}",
                                index,
                                e
                            );
                        }
                    }
                }
            }
        }

        Ok(model::WalletTransactions {
            transactions,
            total,
        })
    }

    #[cfg(test)]
    /// Get an address if it exists in memory or storage.
    pub fn get_address(
        &self,
        account: u32,
        keychain: u32,
        index: u32,
    ) -> Result<Arc<model::Address>> {
        let state = self.state.read()?;

        self._get_address(&state, account, keychain, index)
    }

    /// Non-locking version of `get_address` (requires a reference to `State` to be passed as
    /// argument instead of taking a read lock on `self.state`, so as to avoid deadlocks).
    pub fn _get_address(
        &self,
        state: &State,
        account: u32,
        keychain: u32,
        index: u32,
    ) -> Result<Arc<model::Address>> {
        let path = model::Path {
            account,
            keychain,
            index,
        }
        .to_string();

        if let Some(address) = state.pending_addresses_by_path.get(&path) {
            log::trace!("Address {} found in memory", path);

            Ok(address.clone())
        } else {
            log::trace!(
                "Address {} not found in memory, looking for it in storage...",
                path,
            );
            let address = self.db.get(&keys::address(account, keychain, index))?;
            let pkh = self.db.get(&keys::address_pkh(account, keychain, index))?;
            let info = self.db.get(&keys::address_info(account, keychain, index))?;

            Ok(Arc::new(model::Address {
                address,
                index,
                keychain,
                account,
                path,
                info,
                pkh,
            }))
        }
    }

    /// Get a transaction if exists.
    pub fn get_transaction(&self, account: u32, index: u32) -> Result<model::BalanceMovement> {
        Ok(self.db.get(&keys::transaction_movement(account, index))?)
    }

    /// Get a previously put serialized value.
    ///
    /// See `kv_set`.
    pub fn kv_get(&self, key: &str) -> Result<Option<String>> {
        let value = self.db.get_opt(&keys::custom(key))?;

        Ok(value)
    }

    /// Set an arbitrary string value under a custom key.
    ///
    /// See `kv_get`.
    pub fn kv_set(&self, key: &str, value: &str) -> Result<()> {
        self.db.put(&keys::custom(key), value.to_string())?;

        Ok(())
    }

    /// Update a wallet's name and/or description
    pub fn update(&self, name: Option<String>, description: Option<String>) -> Result<()> {
        let mut batch = self.db.batch();
        let mut state = self.state.write()?;

        state.name = name;
        if let Some(ref name) = state.name {
            batch.put(&keys::wallet_name(), name)?;
        }

        state.description = description;
        if let Some(ref description) = state.description {
            batch.put(&keys::wallet_description(), description)?;
        }

        self.db.write(batch)?;

        Ok(())
    }

    /// Filter transactions in a block (received from a node) if they belong to wallet accounts.
    pub fn filter_wallet_transactions(
        &self,
        txns: impl Iterator<Item = Transaction>,
    ) -> Result<Vec<Transaction>> {
        let state = self.state.read()?;

        let mut filtered_txns = vec![];
        for txn in txns {
            // Inputs and outputs from different transaction types
            let (inputs, outputs): (&[Input], &[ValueTransferOutput]) = match txn {
                Transaction::ValueTransfer(ref vt) => (&vt.body.inputs, &vt.body.outputs),
                Transaction::DataRequest(ref dr) => (&dr.body.inputs, &dr.body.outputs),
                Transaction::Commit(ref commit) => (&commit.body.collateral, &commit.body.outputs),
                Transaction::Tally(ref tally) => (&[], &tally.outputs),
                Transaction::Mint(ref mint) => (&[], &mint.outputs),
                _ => continue,
            };

            // Check if tally txn corresponds to a wallet sent data request
            if let Transaction::Tally(tally) = &txn {
                // There is a DR transaction persisted in database whose tally was found or
                // there is a DR transaction in pending state whose tally was found
                if state
                    .pending_dr_movements
                    .contains_key(&tally.dr_pointer.to_string())
                    || self
                        .db
                        .get(&keys::transactions_index(tally.dr_pointer.as_ref()))
                        .is_ok()
                {
                    filtered_txns.push(txn.clone());
                    continue;
                }
            }

            let check_db_and_transient = |output: &ValueTransferOutput| {
                self.db.get(&keys::pkh(&output.pkh)).is_ok()
                    || state.transient_external_addresses.contains_key(&output.pkh)
                    || state.transient_internal_addresses.contains_key(&output.pkh)
            };
            // Check if any input or output is from the wallet (input is an UTXO or output points to any wallet's pkh)
            if inputs
                .iter()
                .any(|input| state.utxo_set.get(&input.output_pointer().into()).is_some())
                || outputs.iter().any(check_db_and_transient)
            {
                filtered_txns.push(txn.clone());
            }
        }

        Ok(filtered_txns)
    }

    /// Index transactions in a block received from a node.
    pub fn index_block_transactions(
        &self,
        block_info: &model::Beacon,
        txns: &[model::ExtendedTransaction],
        confirmed: bool,
    ) -> Result<Vec<model::BalanceMovement>> {
        let mut state = self.state.write()?;
        let mut addresses = HashMap::new();
        let mut block_balance_movements = Vec::new();
        let mut dr_balance_movements = HashMap::new();
        let mut db_movements_to_update = Vec::new();

        // Index all transactions
        for txn in txns {
            // Transactions are only indexed if they do not exist in database, or if resynchronizing.
            match self._index_transaction(&mut state, &mut addresses, txn, block_info, confirmed) {
                Ok(Some(balance_movement)) => {
                    if let Transaction::DataRequest(dr_tx) = &txn.transaction {
                        dr_balance_movements.insert(
                            dr_tx.hash().to_string(),
                            (block_info.block_hash, block_balance_movements.len()),
                        );
                    }
                    block_balance_movements.push(balance_movement);
                }
                Ok(None) => {}
                e @ Err(_) => {
                    log::error!("Error while indexing transaction: {:?}", e);
                    e?;
                }
            }
            if let Transaction::Tally(tally) = &txn.transaction {
                // The DR transaction is in pending state
                if let Some((pending_block_hash, index)) = state
                    .pending_dr_movements
                    .get(&tally.dr_pointer.to_string())
                    .cloned()
                {
                    log::debug!(
                        "Found a tally for data request {:?} that was in pending state",
                        tally.dr_pointer.to_string()
                    );
                    let dr_movement = state
                        .pending_movements
                        .get(&pending_block_hash.to_string())
                        .unwrap()[index]
                        .clone();

                    match &dr_movement.transaction.data.clone() {
                        model::TransactionData::DataRequest(dr_data) => {
                            let mut updated_dr_movement = dr_movement;
                            updated_dr_movement.transaction.data = build_updated_dr_transaction_data(dr_data, tally, &txn.metadata)?;
                            state.pending_movements.get_mut(&pending_block_hash.to_string()).unwrap()[index] = updated_dr_movement;
                            state.pending_dr_movements.remove(&tally.dr_pointer.to_string());
                        }
                        _ => log::warn!("data request tally update failed because wrong transaction type (txn: {})", tally.dr_pointer),
                    }
                }
                // The DR transaction was confirmed but the tally wasn't. Fetch the dr from DB.
                else if let Ok((dr_movement, txn_id)) = self
                    .db
                    .get(&keys::transactions_index(tally.dr_pointer.as_ref()))
                    .and_then(|txn_id| {
                        self.db
                            .get(&keys::transaction_movement(state.account, txn_id))
                            .map(|dr_movement| (dr_movement, txn_id))
                    })
                {
                    log::debug!("Found a tally for data request {:?} that was in DB", txn_id);
                    match &dr_movement.transaction.data.clone() {
                        model::TransactionData::DataRequest(dr_data) => {
                            let mut dr_movement_to_update = dr_movement;
                            dr_movement_to_update.transaction.data = build_updated_dr_transaction_data(dr_data, tally, &txn.metadata)?;
                            dr_movement_to_update.db_key = txn_id;
                            db_movements_to_update.push(dr_movement_to_update);
                        }
                        _ => log::warn!("data request tally update failed because wrong transaction type (txn: {})", tally.dr_pointer),
                    }
                } else {
                    log::debug!(
                        "data request tally update not required it was not found (txn: {})",
                        tally.dr_pointer
                    )
                }
            }
        }

        let timestamp = convert_block_epoch_to_timestamp(state.epoch_constants, block_info.epoch);
        state.balance.unconfirmed = state
            .utxo_set
            .iter()
            .map(|(_, balance)| (balance.amount, balance.time_lock))
            .fold(
                model::BalanceInfo::default(),
                |mut acc, (amount, time_lock)| {
                    if timestamp > time_lock {
                        acc.available =
                            acc.available.checked_add(amount).expect("balance overflow");
                    } else {
                        acc.locked = acc.locked.checked_add(amount).expect("balance overflow");
                    }

                    acc
                },
            );

        let addresses = addresses.into_iter().map(|(_k, v)| v).collect();

        // Persist into database
        if confirmed {
            let mut balance_movements_to_persist = block_balance_movements.clone();
            balance_movements_to_persist.extend_from_slice(&db_movements_to_update);

            self._persist_block_txns(
                balance_movements_to_persist.clone(),
                addresses,
                state.transaction_next_id,
                state.utxo_set.clone(),
                &state.balance.unconfirmed,
                block_info,
            )?;
            // At this point state.utxo_set will only have confirmed utxos, so we can clear the
            // pending transactions
            state.pending_transactions.clear();

            // Update pending DR movements if they were persisted
            // balance_movements_to_persist.
            balance_movements_to_persist.iter().for_each(|x| {
                state.pending_dr_movements.remove(&x.transaction.hash);
            });

            // If everything was OK, update `last_confirmed` beacon
            state.last_confirmed = CheckpointBeacon {
                checkpoint: block_info.epoch,
                hash_prev_block: block_info.block_hash,
            };
            state.balance.confirmed = state.balance.unconfirmed;
        } else {
            for address in &addresses {
                let path = address.path.clone();
                state
                    .pending_addresses_by_path
                    .insert(path, address.clone());
            }

            // Build wallet state after block index
            let block_state = state::StateSnapshot {
                balance: state.balance.unconfirmed,
                beacon: block_info.clone(),
                transaction_next_id: state.transaction_next_id,
                utxo_set: state.utxo_set.clone(),
            };

            state
                .pending_blocks
                .insert(block_info.block_hash.to_string(), block_state);

            state.pending_movements.insert(
                block_info.block_hash.to_string(),
                block_balance_movements.clone(),
            );
            state.pending_dr_movements.extend(dr_balance_movements);
            state
                .db_movements_to_update
                .insert(block_info.block_hash.to_string(), db_movements_to_update);
            state
                .pending_addresses_by_block
                .insert(block_info.block_hash.to_string(), addresses);

            for balance_movement in &block_balance_movements {
                state
                    .pending_transactions
                    .insert(balance_movement.transaction.hash.parse().unwrap());
            }
        }

        Ok(block_balance_movements)
    }

    fn _persist_block_txns(
        &self,
        balance_movements: Vec<model::BalanceMovement>,
        addresses: Vec<Arc<model::Address>>,
        transaction_next_id: u32,
        utxo_set: model::UtxoSet,
        balance: &model::BalanceInfo,
        block_info: &model::Beacon,
    ) -> Result<()> {
        log::debug!(
            "Persisting block #{} changes: {} balance movements and {} address changes",
            block_info.epoch,
            balance_movements.len(),
            addresses.len(),
        );

        let account = 0;
        let mut batch = self.db.batch();

        // Write transactional data (index, hash and balance movement)
        for mut movement in balance_movements {
            let txn_hash = Hash::from_str(&movement.transaction.hash)?;
            movement.transaction.confirmed = true;
            batch.put(
                &keys::transactions_index(txn_hash.as_ref()),
                &movement.db_key,
            )?;
            batch.put(
                &keys::transaction_hash(account, movement.db_key),
                txn_hash.as_ref().to_vec(),
            )?;
            batch.put(
                &keys::transaction_movement(account, movement.db_key),
                &movement,
            )?;
        }

        // Write account state
        batch.put(&keys::transaction_next_id(account), transaction_next_id)?;
        batch.put(&keys::account_utxo_set(account), utxo_set)?;
        batch.put(&keys::account_balance(account), balance)?;

        // Persist addresses
        for address in addresses {
            batch.put(
                &keys::address_info(account, address.keychain, address.index),
                &address.info,
            )?;
            batch.put(
                &keys::address(account, address.keychain, address.index),
                &address.address,
            )?;
            batch.put(
                &keys::address_path(account, address.keychain, address.index),
                &address.path,
            )?;
            batch.put(
                &keys::address_pkh(account, address.keychain, address.index),
                &address.pkh,
            )?;
        }

        // Update the last_sync in the database (which corresponds with the last_confirmed in the state)
        batch.put(
            &keys::wallet_last_sync(),
            CheckpointBeacon {
                checkpoint: block_info.epoch,
                hash_prev_block: block_info.block_hash,
            },
        )?;

        self.db.write(batch)?;

        Ok(())
    }

    /// Retrieve the balance for the current wallet account.
    pub fn balance(&self) -> Result<model::WalletBalance> {
        let state = self.state.read()?;
        let balance = state.balance;

        Ok(balance)
    }

    /// Retrieve the utxo information for the current wallet account.
    pub fn get_utxo_info(&self) -> Result<model::UtxoSet> {
        let state = self.state.read()?;
        let utxo_info = state.utxo_set.clone();

        Ok(utxo_info)
    }

    /// Create a new value transfer transaction using available UTXOs.
    pub fn create_vtt(
        &self,
        types::VttParams {
            fee,
            outputs,
            fee_type,
            utxo_strategy,
            selected_utxos,
        }: types::VttParams,
    ) -> Result<VTTransaction> {
        let mut state = self.state.write()?;
        let (inputs, outputs) = self.create_vt_transaction_components(
            &mut state,
            outputs,
            fee,
            fee_type,
            &utxo_strategy,
            selected_utxos,
        )?;

        let body = VTTransactionBody::new(inputs.clone(), outputs);
        let sign_data = body.hash();
        let signatures = self.create_signatures_from_inputs(inputs, sign_data, &mut state);

        Ok(VTTransaction::new(body, signatures?))
    }

    /// Create a new data request transaction using available UTXOs.
    pub fn create_data_req(
        &self,
        types::DataReqParams {
            fee,
            request,
            fee_type,
        }: types::DataReqParams,
    ) -> Result<DRTransaction> {
        let mut state = self.state.write()?;
        let (inputs, outputs) =
            self.create_dr_transaction_components(&mut state, request.clone(), fee, fee_type)?;

        let body = DRTransactionBody::new(inputs.clone(), outputs, request);
        let sign_data = body.hash();
        let signatures = self.create_signatures_from_inputs(inputs, sign_data, &mut state);

        Ok(DRTransaction::new(body, signatures?))
    }

    /// Create signatures from inputs
    fn create_signatures_from_inputs(
        &self,
        inputs: Vec<Input>,
        sign_data: Hash,
        state: &mut State,
    ) -> Result<Vec<KeyedSignature>> {
        let mut keyed_signatures = vec![];

        for input in inputs {
            let key_balance = state.utxo_set.get(&input.output_pointer().into()).unwrap();

            let model::Path {
                keychain, index, ..
            } = self.db.get(&keys::pkh(&key_balance.pkh))?;
            let parent_key = state
                .keychains
                .get(keychain as usize)
                .expect("could not get keychain");

            let extended_sign_key =
                parent_key.derive(&self.engine, &KeyPath::default().index(index))?;

            let sign_key = extended_sign_key.into();

            let public_key = From::from(PK::from_secret_key(&self.engine, &sign_key));
            let signature =
                From::from(signature::sign(&self.engine, sign_key, sign_data.as_ref())?);

            keyed_signatures.push(KeyedSignature {
                signature,
                public_key,
            });
        }

        Ok(keyed_signatures)
    }

    fn create_vt_transaction_components(
        &self,
        state: &mut State,
        outputs: Vec<ValueTransferOutput>,
        fee: u64,
        fee_type: FeeType,
        utxo_strategy: &UtxoSelectionStrategy,
        selected_utxos: HashSet<model::OutPtr>,
    ) -> Result<(Vec<Input>, Vec<ValueTransferOutput>)> {
        let timestamp = u64::try_from(get_timestamp()).unwrap();

        let (inputs, outputs) = self.build_inputs_outputs_wallet(
            outputs,
            None,
            fee,
            fee_type,
            state,
            timestamp,
            None,
            utxo_strategy,
            self.params.max_vt_weight,
            selected_utxos,
        )?;

        Ok((inputs, outputs))
    }

    fn create_dr_transaction_components(
        &self,
        state: &mut State,
        request: DataRequestOutput,
        fee: u64,
        fee_type: FeeType,
    ) -> Result<(Vec<Input>, Vec<ValueTransferOutput>)> {
        let utxo_strategy = UtxoSelectionStrategy::Random { from: None };
        let timestamp = u64::try_from(get_timestamp()).unwrap();

        let (inputs, outputs) = self.build_inputs_outputs_wallet(
            vec![],
            Some(&request),
            fee,
            fee_type,
            state,
            timestamp,
            None,
            &utxo_strategy,
            self.params.max_dr_weight,
            HashSet::default(),
        )?;

        Ok((inputs, outputs))
    }

    /// Function that returns an address for the change ValueTransferOutput
    fn calculate_change_address(
        &self,
        is_vtt: bool,
        inputs: &[Input],
        state: &mut State,
    ) -> Result<PublicKeyHash> {
        let pkh = if is_vtt {
            // In case of VTTransaction, a new internal address is used
            self._gen_internal_address(state, None)?.pkh
        } else {
            // In case of DRTransaction, the first input pkh will be used
            let first_input = inputs.first().unwrap().output_pointer();
            let key_balance = state.utxo_set.get(&first_input.into()).unwrap();

            key_balance.pkh
        };

        Ok(pkh)
    }

    /// This function calls to their equivalent in data_structures 'build_inputs_outputs'
    /// to share the same create transaction logic.
    /// Due to wallet handles many different addresses, the 'insert_change' logic is different
    /// from the node's one
    #[allow(clippy::too_many_arguments)]
    fn build_inputs_outputs_wallet(
        &self,
        outputs: Vec<ValueTransferOutput>,
        dr_output: Option<&DataRequestOutput>,
        fee: u64,
        fee_type: FeeType,
        state: &mut State,
        timestamp: u64,
        // The block number must be lower than this limit
        block_number_limit: Option<u32>,
        utxo_strategy: &UtxoSelectionStrategy,
        max_weight: u32,
        selected_utxos: HashSet<model::OutPtr>,
    ) -> Result<(Vec<Input>, Vec<ValueTransferOutput>)> {
        let empty_hashset = HashSet::default();
        let unconfirmed_transactions = if self.params.use_unconfirmed_utxos {
            &empty_hashset
        } else {
            &state.pending_transactions
        };

        let mut wallet_utxos = WalletUtxos {
            utxo_set: &state.utxo_set,
            used_outputs: &mut state.used_outputs,
            unconfirmed_transactions,
            selected_utxos,
        };

        let tx_info = wallet_utxos.build_inputs_outputs(
            outputs,
            dr_output,
            fee,
            fee_type,
            timestamp,
            block_number_limit,
            utxo_strategy,
            max_weight,
        )?;

        let change_pkh =
            self.calculate_change_address(dr_output.is_none(), &tx_info.inputs, state)?;

        let mut outputs = tx_info.outputs;
        insert_change_output(
            &mut outputs,
            change_pkh,
            tx_info.input_value - tx_info.output_value - tx_info.fee,
        );

        Ok((tx_info.inputs, outputs))
    }

    fn _gen_internal_address(
        &self,
        state: &mut State,
        label: Option<String>,
    ) -> Result<Arc<model::Address>> {
        let keychain = constants::INTERNAL_KEYCHAIN;
        let account = state.account;
        let index = state.next_internal_index;
        let parent_key = &state.keychains[keychain as usize];

        let (address, next_index) =
            self.derive_and_persist_address(label, parent_key, account, keychain, index, true)?;

        state.next_internal_index = next_index;

        Ok(address)
    }

    fn _index_transaction(
        &self,
        state: &mut State,
        addresses: &mut HashMap<PublicKeyHash, Arc<model::Address>>,
        txn: &model::ExtendedTransaction,
        block_info: &model::Beacon,
        confirmed: bool,
    ) -> Result<Option<model::BalanceMovement>> {
        // Wallet's account mutation (utxo set changes + balance movement)
        let account_mutation =
            match self._get_account_mutation(state, txn, block_info, confirmed)? {
                // If UTXO set has not changed, then there is no balance movement derived from the transaction being processed
                None => return Ok(None),
                Some(account_mutation) => account_mutation,
            };

        // If exists, remove transaction from local pending movements
        let txn_hash = txn.transaction.hash();
        if let Some(local_movement) = state.local_movements.remove(&txn_hash) {
            log::debug!(
                "Updating local pending movement (txn id: {}) because it has been included in block #{}",
                txn_hash,
                block_info.epoch,
            );
            state.balance.local = state
                .balance
                .local
                .checked_sub(local_movement.amount)
                .ok_or(Error::TransactionValueOverflow)?;
        }

        // Update memory state: `utxo_set`
        for pointer in &account_mutation.utxo_removals {
            state.utxo_set.remove(pointer);
            state.used_outputs.remove(pointer);
        }
        for (pointer, key_balance) in &account_mutation.utxo_inserts {
            state.utxo_set.insert(pointer.clone(), key_balance.clone());
        }

        // Update `transaction_next_id`
        state.transaction_next_id = state
            .transaction_next_id
            .checked_add(1)
            .ok_or(Error::TransactionIdOverflow)?;

        // Update addresses (externals/internals) and their information if there were payments (new UTXOs)
        //
        // - Data Request and Tally transactions are ignored as they only contain refunds to data request
        // creators. By protocol the tally output can only be set to the first used input of the DR.
        // - Commit and Reveal transactions are ignored as they only contain miners addresses.
        match txn.transaction {
            Transaction::ValueTransfer(_) | Transaction::Mint(_) => {
                for (output_pointer, key_balance) in account_mutation.utxo_inserts {
                    // Retrieve previous address information
                    let old_address = match addresses.entry(key_balance.pkh) {
                        Entry::Occupied(e) => e.into_mut(),
                        Entry::Vacant(e) => {
                            let path = self.db.get(&keys::pkh(&key_balance.pkh))?;
                            // Get address from memory or DB
                            let old_address =
                                self._get_address(state, path.account, path.keychain, path.index)?;
                            e.insert(old_address)
                        }
                    };

                    // Build the new address information
                    let info = &old_address.info;
                    let mut received_payments = info.received_payments.clone();
                    received_payments.push(output_pointer.to_string());
                    let current_timestamp =
                        convert_block_epoch_to_timestamp(state.epoch_constants, block_info.epoch);
                    let first_payment_date =
                        Some(info.first_payment_date.unwrap_or(current_timestamp));
                    let updated_address = model::Address {
                        info: model::AddressInfo {
                            label: info.label.clone(),
                            received_payments,
                            received_amount: info.received_amount + key_balance.amount,
                            first_payment_date,
                            last_payment_date: Some(current_timestamp),
                        },
                        ..(**old_address).clone()
                    };

                    log::trace!(
                        "Updating address:\nOld: {:?}\nNew: {:?}",
                        old_address,
                        updated_address
                    );

                    *old_address = Arc::new(updated_address);
                }
            }
            _ => {}
        }

        Ok(Some(account_mutation.balance_movement))
    }

    // TODO: notify client of new local pending transaction
    /// Add local pending balance movement submitted by wallet client
    pub fn add_local_movement(
        &self,
        txn: &model::ExtendedTransaction,
    ) -> Result<Option<model::BalanceMovement>> {
        let mut state = self.state.write()?;
        // This line is needed because of this error:
        // - Cannot borrow `state` as mutable because it is also borrowed as immutable
        let mut state = &mut *state;

        // Mark UTXOs as used so we don't double spend
        // Save the timestamp to after which the UTXO can be spent again
        let tx_pending_timeout = u64::from(state.epoch_constants.checkpoints_period) * 10;
        let timestamp = u64::try_from(get_timestamp()).unwrap();

        let inputs = match &txn.transaction {
            Transaction::ValueTransfer(tx) => Some(&tx.body.inputs),
            Transaction::DataRequest(tx) => Some(&tx.body.inputs),
            Transaction::Commit(tx) => Some(&tx.body.collateral),
            Transaction::Reveal(_) => None,
            Transaction::Tally(_) => None,
            Transaction::Mint(_) => None,
        };

        let empty_hashset = HashSet::default();
        let unconfirmed_transactions = if self.params.use_unconfirmed_utxos {
            &empty_hashset
        } else {
            &state.pending_transactions
        };
        let mut wallet_utxos = WalletUtxos {
            utxo_set: &state.utxo_set,
            used_outputs: &mut state.used_outputs,
            unconfirmed_transactions,
            selected_utxos: HashSet::default(),
        };
        if let Some(inputs) = inputs {
            wallet_utxos.set_used_output_pointer(inputs, timestamp + tx_pending_timeout);
        }

        if let Some(mut account_mutation) =
            self._get_account_mutation(state, txn, &model::Beacon::default(), false)?
        {
            account_mutation.balance_movement.transaction.timestamp =
                u64::try_from(get_timestamp())
                    .expect("Get timestamp should return a positive value");
            let txn_hash = txn.transaction.hash();
            state
                .local_movements
                .insert(txn_hash, account_mutation.balance_movement.clone());
            log::debug!(
                "Local pending movement added for transaction id: {})",
                txn_hash
            );
            state.balance.local = state
                .balance
                .local
                .checked_add(account_mutation.balance_movement.amount)
                .ok_or(Error::TransactionValueOverflow)?;

            return Ok(Some(account_mutation.balance_movement));
        }

        Ok(None)
    }

    // During wallet synchronization, generate external and internal addresses
    // if transaction outputs are pointing to transient addresses
    pub fn _sync_address_generation(&self, txns: impl Iterator<Item = Transaction>) -> Result<()> {
        let mut state = self.state.write()?;

        // Exit if not syncing
        if state.transient_internal_addresses.is_empty()
            && state.transient_external_addresses.is_empty()
        {
            return Ok(());
        }

        let outputs = txns
            .flat_map(|txn| match txn {
                Transaction::ValueTransfer(vt) => vt.body.outputs,
                Transaction::DataRequest(dr) => dr.body.outputs,
                Transaction::Commit(commit) => commit.body.outputs,
                Transaction::Tally(tally) => tally.outputs,
                Transaction::Mint(mint) => mint.outputs,
                _ => vec![],
            })
            .collect_vec();

        loop {
            let (new_external_index, new_internal_index) = outputs.iter().fold(
                (state.next_external_index, state.next_internal_index),
                |mut acc, output| {
                    if let Some(address) = state.transient_external_addresses.get(&output.pkh) {
                        if address.keychain == constants::EXTERNAL_KEYCHAIN
                            && address.index >= state.next_external_index
                        {
                            acc.0 = address.index + 1;
                        }
                    } else if let Some(address) =
                        state.transient_internal_addresses.get(&output.pkh)
                    {
                        if address.keychain == constants::INTERNAL_KEYCHAIN
                            && address.index >= state.next_internal_index
                        {
                            acc.1 = address.index + 1;
                        }
                    }

                    acc
                },
            );

            if new_external_index == state.next_external_index
                && new_internal_index == state.next_internal_index
            {
                break;
            }

            // Generate and persist addresses that need to be indexed
            log::debug!(
                "Generating external addresses from index {} to {}",
                state.next_external_index,
                new_external_index
            );
            log::debug!(
                "Generating internal addresses from index {} to {}",
                state.next_internal_index,
                new_internal_index
            );
            for _ in state.next_external_index..new_external_index {
                let addr = self._gen_external_address(&mut state, None)?;
                state.transient_external_addresses.remove(&addr.pkh);
            }
            for _ in state.next_internal_index..new_internal_index {
                let addr = self._gen_internal_address(&mut state, None)?;
                state.transient_internal_addresses.remove(&addr.pkh);
            }

            // Generate new transient addresses if needed
            let transient_external_range = state.next_external_index
                ..state.next_external_index + u32::from(self.params.sync_address_batch_length);
            let transient_internal_range = state.next_internal_index
                ..state.next_internal_index + u32::from(self.params.sync_address_batch_length);
            self._generate_transient_address_ranges(
                &mut state,
                transient_external_range,
                transient_internal_range,
            )?;
        }

        Ok(())
    }

    // Returns the account mutation in terms of changes to the UTXO set and balance
    fn _get_account_mutation(
        &self,
        state: &State,
        txn: &model::ExtendedTransaction,
        block_info: &model::Beacon,
        confirmed: bool,
    ) -> Result<Option<AccountMutation>> {
        // Inputs and outputs from different transaction types
        let (inputs, outputs) = extract_inputs_and_outputs(&txn.transaction)?;

        let mut utxo_removals: Vec<model::OutPtr> = vec![];
        let mut utxo_inserts: Vec<(model::OutPtr, model::OutputInfo)> = vec![];

        let mut input_amount: u64 = 0;
        for input in inputs.iter() {
            let out_ptr: model::OutPtr = input.output_pointer().into();

            if let Some(model::OutputInfo { amount, .. }) = state.utxo_set.get(&out_ptr) {
                input_amount = input_amount
                    .checked_add(*amount)
                    .ok_or(Error::TransactionBalanceOverflow)?;
                utxo_removals.push(out_ptr);
            }
        }

        let mut output_amount: u64 = 0;
        let mut own_outputs: HashMap<PublicKeyHash, model::OutputType> = HashMap::new();
        for (index, output) in outputs.iter().enumerate() {
            if let Some(path) = self.db.get_opt(&keys::pkh(&output.pkh))? {
                match path.keychain {
                    x if x == constants::EXTERNAL_KEYCHAIN => {
                        own_outputs.insert(output.pkh, model::OutputType::External);
                    }
                    x if x == constants::INTERNAL_KEYCHAIN => {
                        own_outputs.insert(output.pkh, model::OutputType::Internal);
                    }
                    _ => {
                        log::warn!(
                            "Output found in DB but keychain is not known: {}",
                            output.pkh
                        );
                    }
                }
            }
            if own_outputs.contains_key(&output.pkh) {
                let out_ptr = model::OutPtr {
                    txn_hash: txn.transaction.hash().as_ref().to_vec(),
                    output_index: u32::try_from(index).unwrap(),
                };
                let output_info = model::OutputInfo {
                    amount: output.value,
                    pkh: output.pkh,
                    time_lock: output.time_lock,
                };
                output_amount = output_amount
                    .checked_add(output.value)
                    .ok_or(Error::TransactionBalanceOverflow)?;

                let address = output.pkh.bech32(if self.params.testnet {
                    Environment::Testnet
                } else {
                    Environment::Mainnet
                });
                log::warn!(
                    "Found transaction to our address {}! Amount: +{} nanowits",
                    address,
                    output.value
                );
                utxo_inserts.push((out_ptr, output_info));
            }
        }

        // If UTXO set has not changed, then there is no balance movement derived from the transaction being processed
        if utxo_inserts.is_empty() && utxo_removals.is_empty() {
            return Ok(None);
        }

        let (amount, kind) = if output_amount >= input_amount {
            (output_amount - input_amount, model::MovementType::Positive)
        } else {
            (input_amount - output_amount, model::MovementType::Negative)
        };

        // Build the balance movement, first computing the miner fee
        let miner_fee: u64 = match &txn.metadata {
            Some(model::TransactionMetadata::InputValues(input_values)) => {
                let total_input_amount = input_values.iter().fold(0, |acc, x| acc + x.value);

                // Genesis block (no inputs) or empty block with only `MintTransaction`
                if total_input_amount == 0 {
                    0u64
                } else {
                    let total_output_amount = outputs.iter().fold(0, |acc, x| acc + x.value);

                    total_input_amount
                        .checked_sub(total_output_amount)
                        .unwrap_or_else(|| {
                            log::warn!("Miner fee below 0 in a transaction of type value transfer or data request: {}", txn.transaction.hash().to_string());

                            0
                        })
                }
            }
            _ => 0,
        };

        let balance_movement = build_balance_movement(
            state.transaction_next_id,
            txn,
            miner_fee,
            kind,
            amount,
            block_info,
            convert_block_epoch_to_timestamp(state.epoch_constants, block_info.epoch),
            confirmed,
            own_outputs,
        )?;

        Ok(Some(AccountMutation {
            balance_movement,
            utxo_inserts,
            utxo_removals,
        }))
    }

    fn _gen_external_address(
        &self,
        state: &mut State,
        label: Option<String>,
    ) -> Result<Arc<model::Address>> {
        let keychain = constants::EXTERNAL_KEYCHAIN;
        let account = state.account;
        let index = state.next_external_index;
        let parent_key = state.keychains[keychain as usize].clone();

        let (address, next_index) =
            self.derive_and_persist_address(label, &parent_key, account, keychain, index, true)?;
        state.next_external_index = next_index;

        Ok(address)
    }

    /// Get previously created Transaction by its hash.
    pub fn get_db_transaction(&self, hex_hash: &str) -> Result<Option<Transaction>> {
        let txn = self.db.get_opt(&keys::transaction(hex_hash))?;

        Ok(txn)
    }

    /// Sign data using the wallet master key.
    pub fn sign_data(
        &self,
        data: &str,
        extended_pk: bool,
    ) -> Result<model::ExtendedKeyedSignature> {
        let state = self.state.read()?;
        let keychain = constants::EXTERNAL_KEYCHAIN;
        let parent_key = &state.keychains[keychain as usize];

        let chaincode = if extended_pk {
            hex::encode(parent_key.chain_code())
        } else {
            "".to_string()
        };
        let public_key = ExtendedPK::from_secret_key(&self.engine, parent_key)
            .key
            .to_string();

        let hashed_data = calculate_sha256(data.as_bytes());
        let signature =
            signature::sign(&self.engine, parent_key.secret_key, hashed_data.as_ref())?.to_string();

        Ok(model::ExtendedKeyedSignature {
            signature,
            public_key,
            chaincode,
        })
    }

    /// Update which was the epoch of the last block that was processed by this wallet.
    pub fn update_sync_state(&self, beacon: CheckpointBeacon, confirmed: bool) -> Result<()> {
        log::debug!(
            "Setting {} tip of the chain for wallet {} to {:?}",
            if confirmed { "confirmed" } else { "pending " },
            self.id,
            beacon,
        );

        if let Ok(mut write_guard) = self.state.write() {
            write_guard.last_sync = beacon;
            if confirmed {
                write_guard.last_confirmed = beacon;
            }
        }

        Ok(())
    }

    /// Handle superblock in wallet by confirming pending block changes
    pub fn handle_superblock(&self, block_hashes: &[String]) -> Result<()> {
        if let Some(last_confirmed_hash) = block_hashes.last() {
            let state = self.state.read()?;
            if last_confirmed_hash == &state.last_confirmed.hash_prev_block.to_string() {
                log::debug!(
                    "Superblock notification was previously handled (Block #{}: {} is already confirmed)",
                    state.last_confirmed.checkpoint,
                    last_confirmed_hash
                );

                return Ok(());
            }
        }

        block_hashes.iter().try_for_each(|block_hash| {
            // Genesis block is always confirmed
            if block_hash == &self.params.genesis_hash.to_string() {
                Ok(())
            } else {
                self.try_consolidate_block(block_hash)
            }
        })
    }

    /// Try to consolidate a block by persisting all changes into the database.
    pub fn try_consolidate_block(&self, block_hash: &str) -> Result<()> {
        let mut state = self.state.write()?;

        // Retrieve and remove pending changes of the block
        let block_state = state.pending_blocks.remove(block_hash).ok_or_else(|| {
            Error::BlockConsolidation(format!("beacon not found for pending block {}", block_hash))
        })?;
        let mut movements = state.pending_movements.remove(block_hash).ok_or_else(|| {
            Error::BlockConsolidation(format!(
                "balance movements not found for pending block {}",
                block_hash
            ))
        })?;
        movements.extend(
            state
                .db_movements_to_update
                .remove(block_hash)
                .ok_or_else(|| {
                    Error::BlockConsolidation(format!(
                        "balance movements not found for pending block {}",
                        block_hash
                    ))
                })?,
        );

        let addresses = state
            .pending_addresses_by_block
            .remove(block_hash)
            .ok_or_else(|| {
                Error::BlockConsolidation(format!(
                    "address infos not found for pending block {}",
                    block_hash
                ))
            })?;

        // Try to persist block transaction changes
        self._persist_block_txns(
            movements.clone(),
            addresses,
            block_state.transaction_next_id,
            block_state.utxo_set.clone(),
            &block_state.balance,
            &block_state.beacon,
        )?;

        // Update pending DR movements if they were persisted
        // balance_movements_to_persist.
        movements.iter().for_each(|x| {
            state.pending_dr_movements.remove(&x.transaction.hash);
            state
                .pending_transactions
                .remove(&x.transaction.hash.parse().unwrap());
        });

        // If everything was OK, update `last_confirmed` beacon
        state.last_confirmed = CheckpointBeacon {
            checkpoint: block_state.beacon.epoch,
            hash_prev_block: block_state.beacon.block_hash,
        };
        state.balance.confirmed = block_state.balance;

        log::debug!(
            "Block #{} ({}) was successfully consolidated",
            state.last_confirmed.checkpoint,
            state.last_confirmed.hash_prev_block,
        );

        Ok(())
    }

    /// Clear all chain data for a wallet in memory and resets synchronization status in database.
    ///
    /// Proceed with caution, as this wipes the following data entirely on memory:
    /// - Synchronization status
    /// - Balances
    /// - Movements
    /// - Addresses and their metadata
    ///
    /// Resets synchronization status in database:
    /// - Last synchronization status set to the genesis block
    /// - Transaction index set to zero
    /// - External and internal address indices set to zero
    pub fn clear_chain_data(&self) -> Result<()> {
        let mut state = self.state.write()?;
        state.clear_chain_data();

        let mut batch = self.db.batch();
        batch.put(&keys::wallet_last_sync(), state.birth_date)?;
        batch.put(&keys::transaction_next_id(0), 0)?;
        batch.put(
            &keys::account_next_index(0, constants::EXTERNAL_KEYCHAIN),
            0,
        )?;
        batch.put(
            &keys::account_next_index(0, constants::INTERNAL_KEYCHAIN),
            0,
        )?;
        self.db.write(batch)?;

        Ok(())
    }

    /// Run a predicate on the state of a wallet in a thread safe manner, thanks to a read lock.
    pub fn lock_and_read_state<P, O>(&self, predicate: P) -> Result<O>
    where
        P: FnOnce(RwLockReadGuard<'_, State>) -> O,
    {
        Ok(predicate(self.state.read()?))
    }

    /// Tell whether a wallet is synchronizing.
    pub fn is_syncing(&self) -> Result<bool> {
        let state = self.state.read()?;

        Ok(!(state.transient_internal_addresses.is_empty()
            && state.transient_external_addresses.is_empty()))
    }

    pub fn export_master_key(&self, password: types::Password) -> Result<String> {
        let state = self.state.read()?;
        let (tag, key) = if let Some(master_key) = self.db.get_opt(&keys::master_key())? {
            let master_key_string = match master_key.to_slip32(&KeyPath::default()) {
                Ok(x) => x,
                Err(_e) => return Err(Error::KeySerialization),
            };
            ("xprv", master_key_string)
        } else {
            let internal_parent_key = &state.keychains[constants::INTERNAL_KEYCHAIN as usize];
            let external_parent_key = &state.keychains[constants::EXTERNAL_KEYCHAIN as usize];
            let internal_secret_key = internal_parent_key.to_slip32(&KeyPath::default());
            let mut internal_secret_key_hex = match internal_secret_key {
                Ok(x) => x,
                Err(_e) => return Err(Error::KeySerialization),
            };
            let external_secret_key = external_parent_key.to_slip32(&KeyPath::default());
            let external_secret_key_hex = match external_secret_key {
                Ok(x) => x,
                Err(_e) => return Err(Error::KeySerialization),
            };
            internal_secret_key_hex.push_str(&external_secret_key_hex);
            ("xprvdouble", internal_secret_key_hex)
        };
        let encrypted_final_key =
            crypto::encrypt_cbc(key.as_ref(), password.as_ref()).map_err(Error::Crypto)?;
        let final_key =
            bech32::encode(tag, encrypted_final_key.to_base32()).map_err(Error::Bech32)?;
        Ok(final_key)
    }
}

fn convert_block_epoch_to_timestamp(epoch_constants: EpochConstants, epoch: Epoch) -> u64 {
    // In case of error, return timestamp 0
    u64::try_from(epoch_constants.epoch_timestamp(epoch).unwrap_or(0))
        .expect("Epoch timestamp should return a positive value")
}

// Extract inputs and output from a transaction
fn extract_inputs_and_outputs(
    transaction: &Transaction,
) -> Result<(Vec<Input>, Vec<ValueTransferOutput>)> {
    // Inputs and outputs from different transaction types
    let (inputs, outputs) = match transaction {
        Transaction::ValueTransfer(vt) => (vt.body.inputs.clone(), vt.body.outputs.clone()),
        Transaction::DataRequest(dr) => (dr.body.inputs.clone(), dr.body.outputs.clone()),
        Transaction::Commit(commit) => {
            (commit.body.collateral.clone(), commit.body.outputs.clone())
        }
        Transaction::Tally(tally) => (vec![], tally.outputs.clone()),
        Transaction::Mint(mint) => (vec![], mint.outputs.clone()),
        _ => {
            return Err(Error::UnsupportedTransactionType(format!(
                "{:?}",
                transaction
            )));
        }
    };

    Ok((inputs, outputs))
}

// Balance Movement Factory
#[allow(clippy::too_many_arguments)]
fn build_balance_movement(
    identifier: u32,
    txn: &model::ExtendedTransaction,
    miner_fee: u64,
    kind: model::MovementType,
    amount: u64,
    block_info: &model::Beacon,
    timestamp: u64,
    confirmed: bool,
    own_outputs: HashMap<PublicKeyHash, model::OutputType>,
) -> Result<model::BalanceMovement> {
    // Input values with their ValueTransferOutput data
    let transaction_inputs = match &txn.metadata {
        Some(model::TransactionMetadata::InputValues(inputs)) => inputs
            .iter()
            .map(|output| model::Input {
                address: output.pkh.to_string(),
                value: output.value,
            })
            .collect::<Vec<model::Input>>(),
        _ => vec![],
    };

    // Transaction Data
    let transaction_data = match &txn.transaction {
        Transaction::ValueTransfer(vtt) => model::TransactionData::ValueTransfer(model::VtData {
            inputs: transaction_inputs,
            outputs: vtt_to_outputs(&vtt.body.outputs, &own_outputs),
        }),
        Transaction::DataRequest(dr) => model::TransactionData::DataRequest(model::DrData {
            inputs: transaction_inputs,
            outputs: vtt_to_outputs(&dr.body.outputs, &own_outputs),
            tally: None,
        }),
        Transaction::Commit(commit) => model::TransactionData::Commit(model::VtData {
            inputs: transaction_inputs,
            outputs: vtt_to_outputs(&commit.body.outputs, &own_outputs),
        }),
        Transaction::Mint(mint) => model::TransactionData::Mint(model::MintData {
            outputs: vtt_to_outputs(&mint.outputs, &own_outputs),
        }),
        Transaction::Tally(tally) => model::TransactionData::Tally(model::TallyData {
            request_transaction_hash: tally.dr_pointer.to_string(),
            outputs: vtt_to_outputs(&tally.outputs, &own_outputs),
            tally: build_tally_report(tally, &txn.metadata)?,
        }),
        _ => {
            return Err(Error::UnsupportedTransactionType(format!(
                "{:?}",
                txn.transaction
            )));
        }
    };

    Ok(model::BalanceMovement {
        db_key: identifier,
        kind,
        amount,
        transaction: model::WalletTransaction {
            block: Some(block_info.clone()),
            confirmed,
            data: transaction_data,
            hash: hex::encode(txn.transaction.hash()),
            miner_fee,
            timestamp,
        },
    })
}

fn build_tally_report(
    tally: &TallyTransaction,
    metadata: &Option<model::TransactionMetadata>,
) -> Result<model::TallyReport> {
    let reveals = match metadata {
        Some(model::TransactionMetadata::Tally(report)) => {
            // List of reveals extracted from Data Request Report
            let mut reveals: HashMap<PublicKeyHash, model::Reveal> = report
                .reveals
                .iter()
                .map(|(pkh, reveal_txn)| {
                    RadonTypes::try_from(reveal_txn.body.reveal.as_slice())
                        .map(|x| {
                            (
                                *pkh,
                                model::Reveal {
                                    value: x.to_string(),
                                    in_consensus: true,
                                },
                            )
                        })
                        .map_err(|err| Error::RevealRadDecode(err.to_string()))
                })
                .collect::<Result<HashMap<PublicKeyHash, model::Reveal>>>()?;

            // Set not `in_consensus` reveals
            for pkh in &tally.out_of_consensus {
                let outlier = reveals.get_mut(pkh).cloned();
                if let Some(mut reveal) = outlier {
                    reveal.in_consensus = false;
                } else {
                    reveals.insert(
                        *pkh,
                        model::Reveal {
                            value: RadonTypes::from(
                                RadonError::try_from(RadError::NoReveals).unwrap(),
                            )
                            .to_string(),
                            in_consensus: false,
                        },
                    );
                }
            }

            Ok(reveals.values().cloned().collect::<Vec<model::Reveal>>())
        }
        _ => Err(Error::WrongMetadataType(format!("{:?}", tally))),
    }?;

    Ok(model::TallyReport {
        result: RadonTypes::try_from(tally.tally.as_slice())
            .map_err(|err| Error::TallyRadDecode(err.to_string()))?
            .to_string(),
        reveals,
    })
}

// Update DR balance movement with tally
fn build_updated_dr_transaction_data(
    dr_data: &model::DrData,
    tally: &TallyTransaction,
    txn_metadata: &Option<model::TransactionMetadata>,
) -> Result<model::TransactionData> {
    Ok(model::TransactionData::DataRequest(model::DrData {
        inputs: dr_data.inputs.clone(),
        outputs: dr_data.outputs.clone(),
        tally: Some(build_tally_report(tally, txn_metadata)?),
    }))
}

#[allow(clippy::type_complexity)]
fn calculate_transaction_ranges(
    offset: usize,
    limit: usize,
    total_local: usize,
    total_pending: usize,
    total_db: usize,
) -> (
    Option<Range<usize>>,
    Option<Range<usize>>,
    Option<Range<usize>>,
) {
    let mut limit = limit;

    let max_local = std::cmp::min(limit, total_local);
    let local = std::cmp::min(total_local.saturating_sub(offset), max_local);

    limit = limit.saturating_sub(local);

    let max_pending = min(limit, total_pending);
    let total_local_pending = total_local + total_pending;
    let pending = min(total_local_pending.saturating_sub(offset), max_pending);
    limit = limit.saturating_sub(pending);

    let max_db = min(limit, total_db);
    let total = total_local + total_pending + total_db;
    let db = min(total.saturating_sub(offset), max_db);

    log::debug!(
        "Will retrieve {} from local, {} from pending and {} from DB",
        local,
        pending,
        db
    );

    let local_range = if local > 0 {
        let init = total_local - offset;
        let end = init - local;

        Some(end..init)
    } else {
        None
    };

    let pending_range = if pending > 0 {
        let init = total_local_pending - local - offset;
        let end = init - pending;

        Some(end..init)
    } else {
        None
    };

    let db_range = if db > 0 {
        let init = total - local - pending - offset;
        let end = init - db;

        Some(end..init)
    } else {
        None
    };

    (local_range, pending_range, db_range)
}

// Map vtt to output vec
fn vtt_to_outputs(
    vtt: &[ValueTransferOutput],
    own_outputs: &HashMap<PublicKeyHash, model::OutputType>,
) -> Vec<model::Output> {
    vtt.iter()
        .map(|output| model::Output {
            address: output.pkh.to_string(),
            time_lock: output.time_lock,
            value: output.value,
            output_type: *own_outputs
                .get(&output.pkh)
                .unwrap_or(&model::OutputType::Other),
        })
        .collect::<Vec<model::Output>>()
}

#[cfg(test)]
impl<T> Wallet<T>
where
    T: Database,
{
    pub fn utxo_set(&self) -> Result<model::UtxoSet> {
        let state = self.state.read()?;

        Ok(state.utxo_set.clone())
    }
}

#[test]
fn test_get_tx_ranges_inner_range() {
    let local_total = 10;
    let pending_total = 10;
    let db_total = 10;

    let (local_range, pending_range, db_range) =
        calculate_transaction_ranges(5, 4, local_total, pending_total, db_total);
    assert_eq!(local_range, Some(1..5));
    assert_eq!(pending_range, None);
    assert_eq!(db_range, None);

    let (local_range, pending_range, db_range) =
        calculate_transaction_ranges(15, 4, local_total, pending_total, db_total);
    assert_eq!(local_range, None);
    assert_eq!(pending_range, Some(1..5));
    assert_eq!(db_range, None);

    let (local_range, pending_range, db_range) =
        calculate_transaction_ranges(25, 4, local_total, pending_total, db_total);
    assert_eq!(local_range, None);
    assert_eq!(pending_range, None);
    assert_eq!(db_range, Some(1..5));
}

#[test]
fn test_get_tx_ranges_overlap() {
    let local_total = 10;
    let pending_total = 10;
    let db_total = 10;

    let (local_range, pending_range, db_range) =
        calculate_transaction_ranges(5, 10, local_total, pending_total, db_total);
    assert_eq!(local_range, Some(0..5));
    assert_eq!(pending_range, Some(5..10));
    assert_eq!(db_range, None);

    let (local_range, pending_range, db_range) =
        calculate_transaction_ranges(15, 10, local_total, pending_total, db_total);
    assert_eq!(local_range, None);
    assert_eq!(pending_range, Some(0..5));
    assert_eq!(db_range, Some(5..10));

    let (local_range, pending_range, db_range) =
        calculate_transaction_ranges(5, 20, local_total, pending_total, db_total);
    assert_eq!(local_range, Some(0..5));
    assert_eq!(pending_range, Some(0..10));
    assert_eq!(db_range, Some(5..10));
}

#[test]
fn test_get_tx_ranges_exceed() {
    let local_total = 10;
    let pending_total = 10;
    let db_total = 10;

    let (local_range, pending_range, db_range) =
        calculate_transaction_ranges(5, 40, local_total, pending_total, db_total);
    assert_eq!(local_range, Some(0..5));
    assert_eq!(pending_range, Some(0..10));
    assert_eq!(db_range, Some(0..10));

    let (local_range, pending_range, db_range) =
        calculate_transaction_ranges(15, 40, local_total, pending_total, db_total);
    assert_eq!(local_range, None);
    assert_eq!(pending_range, Some(0..5));
    assert_eq!(db_range, Some(0..10));

    let (local_range, pending_range, db_range) =
        calculate_transaction_ranges(25, 40, local_total, pending_total, db_total);
    assert_eq!(local_range, None);
    assert_eq!(pending_range, None);
    assert_eq!(db_range, Some(0..5));
}
