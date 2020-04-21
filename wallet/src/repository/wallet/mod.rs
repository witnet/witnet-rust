use std::sync::RwLock;

use super::*;
use crate::{
    account, constants,
    db::{Database, WriteBatch as _},
    model,
    params::Params,
    types::{self, Hashable as _},
};

mod state;
#[cfg(test)]
mod tests;

use crate::types::{signature, ExtendedPK, RadonError};
use state::State;
use std::collections::HashMap;
use std::convert::TryFrom;
use witnet_crypto::hash::calculate_sha256;
use witnet_data_structures::chain::{
    CheckpointBeacon, Environment, Epoch, EpochConstants, PublicKeyHash,
};

/// Internal structure used to gather state mutations while indexing block transactions
struct AccountMutation {
    kind: model::MovementType,
    amount: u64,
    utxo_removals: Vec<model::OutPtr>,
    utxo_inserts: Vec<(model::OutPtr, model::KeyBalance)>,
}

pub struct Wallet<T> {
    id: String,
    db: T,
    params: Params,
    engine: types::CryptoEngine,
    state: RwLock<State>,
}

impl<T> Wallet<T>
where
    T: Database,
{
    pub fn unlock(id: &str, db: T, params: Params, engine: types::CryptoEngine) -> Result<Self> {
        let id = id.to_owned();
        let name = db.get_opt(keys::wallet_name())?;
        let caption = db.get_opt(keys::wallet_caption())?;
        let account = db.get_or_default(keys::wallet_default_account())?;
        let available_accounts = db
            .get_opt(keys::wallet_accounts())?
            .unwrap_or_else(|| vec![account]);

        let transaction_next_id = db.get_or_default(&keys::transaction_next_id(account))?;
        let utxo_set: model::UtxoSet = db.get_or_default(&keys::account_utxo_set(account))?;
        let balance = db
            .get_opt(&keys::account_balance(account))?
            .unwrap_or_else(|| {
                // compute balance from utxo set if is not cached in the
                // database, this is mostly used for testing where overflow
                // checks are enabled
                utxo_set
                    .iter()
                    .map(|(_, balance)| balance.amount)
                    .fold(0u64, |acc, amount| {
                        acc.checked_add(amount).expect("balance overflow")
                    })
            });

        let last_sync = db
            .get(&keys::wallet_last_sync())
            .unwrap_or_else(|_| CheckpointBeacon {
                checkpoint: 0,
                hash_prev_block: params.genesis_hash,
            });
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

        let state = RwLock::new(State {
            name,
            caption,
            account,
            keychains,
            next_external_index,
            next_internal_index,
            available_accounts,
            balance,
            transaction_next_id,
            utxo_set,
            epoch_constants,
            last_sync,
        });

        Ok(Self {
            id,
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

        Ok(types::WalletData {
            name: state.name.clone(),
            caption: state.caption.clone(),
            balance,
            current_account,
            available_accounts: state.available_accounts.clone(),
            last_sync,
        })
    }

    /// Generic method for generating an address.
    ///
    /// See `gen_internal_address` and `gen_external_address` for more
    /// concrete implementations.
    pub fn gen_address(
        &self,
        label: Option<String>,
        parent_key: &types::ExtendedSK,
        account: u32,
        keychain: u32,
        index: u32,
    ) -> Result<(model::Address, u32)> {
        let next_index = index.checked_add(1).ok_or_else(|| Error::IndexOverflow)?;

        let extended_sk =
            parent_key.derive(&self.engine, &types::KeyPath::default().index(index))?;
        let types::ExtendedPK { key, .. } =
            types::ExtendedPK::from_secret_key(&self.engine, &extended_sk);

        let pkh = witnet_data_structures::chain::PublicKey::from(key).pkh();
        let address = pkh.bech32(if self.params.testnet {
            Environment::Testnet
        } else {
            Environment::Mainnet
        });
        let path = format!(
            "{}/{}/{}",
            account::account_keypath(account),
            keychain,
            index
        );

        // Persist changes and new address in database
        let mut batch = self.db.batch();

        batch.put(keys::address(account, keychain, index), &address)?;
        batch.put(keys::address_path(account, keychain, index), &path)?;
        batch.put(keys::address_pkh(account, keychain, index), &pkh)?;
        if let Some(label) = &label {
            batch.put(keys::address_label(account, keychain, index), label)?;
        }
        batch.put(
            keys::pkh(&pkh),
            model::Path {
                account,
                keychain,
                index,
            },
        )?;
        batch.put(keys::account_next_index(account, keychain), next_index)?;

        self.db.write(batch)?;

        let address = model::Address {
            address,
            path,
            label,
            index,
            account,
            keychain,
            pkh,
        };

        Ok((address, next_index))
    }

    /// Generate an address in the external keychain (WIP-0001).
    pub fn gen_external_address(&self, label: Option<String>) -> Result<model::Address> {
        let mut state = self.state.write()?;

        self._gen_external_address(&mut state, label)
    }

    /// Generate an address in the internal keychain (WIP-0001).
    #[cfg(test)]
    pub fn gen_internal_address(&self, label: Option<String>) -> Result<model::Address> {
        let mut state = self.state.write()?;

        self._gen_internal_address(&mut state, label)
    }

    /// Return a list of the generated external addresses that.
    pub fn external_addresses(&self, offset: u32, limit: u32) -> Result<model::Addresses> {
        let keychain = constants::EXTERNAL_KEYCHAIN;
        let state = self.state.read()?;
        let account = state.account;
        let total = state.next_external_index;

        let end = total.saturating_sub(offset);
        let start = end.saturating_sub(limit);
        let range = start..end;
        let mut addresses = Vec::with_capacity(range.len());

        for index in range.rev() {
            let address = self.get_address(account, keychain, index)?;
            addresses.push(address);
        }

        Ok(model::Addresses { addresses, total })
    }

    /// Return a list of the transactions.
    pub fn transactions(&self, offset: u32, limit: u32) -> Result<model::Transactions> {
        let state = self.state.read()?;
        let account = state.account;
        let total = state.transaction_next_id;

        let end = total.saturating_sub(offset);
        let start = end.saturating_sub(limit);
        let range = start..end;
        let mut transactions = Vec::with_capacity(range.len());

        for index in range.rev() {
            match self.get_transaction(account, index) {
                Ok(transaction) => {
                    transactions.push(transaction);
                }
                Err(e) => {
                    log::error!("transactions: {}", e);
                }
            }
        }

        Ok(model::Transactions {
            transactions,
            total,
        })
    }

    /// Get and address if exists.
    pub fn get_address(&self, account: u32, keychain: u32, index: u32) -> Result<model::Address> {
        let address = self.db.get(&keys::address(account, keychain, index))?;
        let path = self.db.get(&keys::address_path(account, keychain, index))?;
        let pkh = self.db.get(&keys::address_pkh(account, keychain, index))?;
        let label = self
            .db
            .get_opt(&keys::address_label(account, keychain, index))?;

        Ok(model::Address {
            address,
            path,
            pkh,
            index,
            account,
            keychain,
            label,
        })
    }

    /// Get a transaction if exists.
    pub fn get_transaction(&self, account: u32, index: u32) -> Result<model::BalanceMovement> {
        Ok(self
            .db
            .get::<_, model::BalanceMovement>(&keys::transaction_movement(account, index))?)
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
        self.db.put(&keys::custom(key), value)?;

        Ok(())
    }

    /// Update a wallet's name and/or caption
    pub fn update(&self, name: Option<String>, caption: Option<String>) -> Result<()> {
        let mut batch = self.db.batch();
        let mut state = self.state.write()?;

        state.name = name;
        if let Some(ref name) = state.name {
            batch.put(keys::wallet_name(), name)?;
        }

        state.caption = caption;
        if let Some(ref caption) = state.caption {
            batch.put(keys::wallet_caption(), caption)?;
        }

        self.db.write(batch)?;

        Ok(())
    }

    /// Filter transactions in a block (received from a node) if they belong to wallet accounts.
    pub fn filter_wallet_transactions(
        &self,
        txns: &[types::Transaction],
    ) -> Result<Vec<types::Transaction>> {
        let state = self.state.read()?;

        let mut filtered_txns = vec![];
        for txn in txns {
            // Inputs and outputs from different transaction types
            let (inputs, outputs): (&[types::TransactionInput], &[types::VttOutput]) = match txn {
                types::Transaction::ValueTransfer(vt) => (&vt.body.inputs, &vt.body.outputs),
                types::Transaction::DataRequest(dr) => (&dr.body.inputs, &dr.body.outputs),
                types::Transaction::Commit(commit) => {
                    (&commit.body.collateral, &commit.body.outputs)
                }
                types::Transaction::Tally(tally) => (&[], &tally.outputs),
                types::Transaction::Mint(mint) => (&[], std::slice::from_ref(&mint.output)),
                _ => continue,
            };

            let check_db = |output: &types::VttOutput| {
                self.db
                    .get::<_, model::Path>(&keys::pkh(&output.pkh))
                    .is_ok()
            };
            // Check if any input or output is from the wallet (input is an UTXO or output points to any wallet's pkh)
            if inputs
                .iter()
                .any(|input| state.utxo_set.get(&input.output_pointer().into()).is_some())
                || outputs.iter().any(check_db)
            {
                filtered_txns.push(txn.clone());
            }
        }

        Ok(filtered_txns)
    }

    /// Index transactions in a block received from a node.
    pub fn index_transactions(
        &self,
        block_info: &model::Beacon,
        txns: &[model::ExtendedTransaction],
    ) -> Result<()> {
        let mut state = self.state.write()?;

        for txn in txns {
            // Check if transaction already exists in the database
            let hash = txn.transaction.hash().as_ref().to_vec();
            match self
                .db
                .get_opt::<_, u32>(&keys::transactions_index(&hash))?
            {
                None => self._index_transaction(&mut state, txn, block_info)?,
                Some(_) => log::warn!(
                    "The transaction {} already exists in the database",
                    txn.transaction.hash()
                ),
            }
        }

        Ok(())
    }

    /// Retrieve the balance for the current wallet account.
    pub fn balance(&self) -> Result<types::Balance> {
        let state = self.state.read()?;
        let account = state.account;
        let amount = state.balance;

        Ok(types::Balance { account, amount })
    }

    /// Create a new value transfer transaction using available UTXOs.
    pub fn create_vtt(
        &self,
        types::VttParams {
            pkh,
            value,
            fee,
            time_lock,
        }: types::VttParams,
    ) -> Result<types::VTTransaction> {
        let mut state = self.state.write()?;
        let components =
            self._create_transaction_components(&mut state, value, fee, Some((pkh, time_lock)))?;

        let body = types::VTTransactionBody::new(components.inputs, components.outputs);
        let sign_data = body.hash();
        let signatures: Result<Vec<types::KeyedSignature>> = components
            .sign_keys
            .into_iter()
            .map(|sign_key| {
                let public_key = From::from(types::PK::from_secret_key(&self.engine, &sign_key));
                let signature = From::from(types::signature::sign(
                    &self.engine,
                    sign_key,
                    sign_data.as_ref(),
                )?);

                Ok(types::KeyedSignature {
                    signature,
                    public_key,
                })
            })
            .collect();

        Ok(types::VTTransaction::new(body, signatures?))
    }

    /// Create a new data request transaction using available UTXOs.
    pub fn create_data_req(
        &self,
        types::DataReqParams { fee, request }: types::DataReqParams,
    ) -> Result<types::DRTransaction> {
        let mut state = self.state.write()?;
        let value = request
            .checked_total_value()
            .map_err(|_| Error::TransactionValueOverflow)?;
        let components = self._create_transaction_components(&mut state, value, fee, None)?;

        let body = types::DRTransactionBody::new(components.inputs, components.outputs, request);
        let sign_data = body.hash();
        let signatures: Result<Vec<types::KeyedSignature>> = components
            .sign_keys
            .into_iter()
            .map(|sign_key| {
                let public_key = From::from(types::PK::from_secret_key(&self.engine, &sign_key));
                let signature = From::from(types::signature::sign(
                    &self.engine,
                    sign_key,
                    sign_data.as_ref(),
                )?);

                Ok(types::KeyedSignature {
                    signature,
                    public_key,
                })
            })
            .collect();

        Ok(types::DRTransaction::new(body, signatures?))
    }

    fn _create_transaction_components(
        &self,
        state: &mut State,
        value: u64,
        fee: u64,
        recipient: Option<(types::PublicKeyHash, u64)>,
    ) -> Result<types::TransactionComponents> {
        let target = value.saturating_add(fee);
        let mut payment = 0u64;
        let mut inputs = Vec::with_capacity(5);
        let mut outputs = Vec::with_capacity(2);
        let mut sign_keys = Vec::with_capacity(5);
        let mut used_utxos = Vec::with_capacity(5);
        let mut balance = state.balance;

        if let Some((pkh, time_lock)) = recipient {
            outputs.push(types::VttOutput {
                pkh,
                value,
                time_lock,
            });
        }

        for (out_ptr, key_balance) in state.utxo_set.iter() {
            if payment >= target {
                break;
            }

            let input = types::TransactionInput::new(types::OutputPointer {
                transaction_id: out_ptr.transaction_id(),
                output_index: out_ptr.output_index,
            });
            let model::Path {
                keychain, index, ..
            } = self.db.get(&keys::pkh(&key_balance.pkh))?;
            let parent_key = &state
                .keychains
                .get(keychain as usize)
                .expect("could not get keychain");

            let extended_sign_key =
                parent_key.derive(&self.engine, &types::KeyPath::default().index(index))?;

            payment = payment
                .checked_add(key_balance.amount)
                .ok_or_else(|| Error::TransactionValueOverflow)?;
            balance = balance
                .checked_sub(key_balance.amount)
                .ok_or_else(|| Error::TransactionBalanceUnderflow)?;
            inputs.push(input);
            sign_keys.push(extended_sign_key.into());
            used_utxos.push(out_ptr.clone());
        }

        if payment < target {
            Err(Error::InsufficientBalance)
        } else {
            let change = payment - target;

            if change > 0 {
                let change_address = self._gen_internal_address(state, None)?;

                outputs.push(types::VttOutput {
                    pkh: change_address.pkh,
                    value: change,
                    time_lock: 0,
                });
            }

            Ok(types::TransactionComponents {
                value,
                balance,
                change,
                inputs,
                outputs,
                sign_keys,
                used_utxos,
            })
        }
    }

    fn _gen_internal_address(
        &self,
        state: &mut State,
        label: Option<String>,
    ) -> Result<model::Address> {
        let keychain = constants::INTERNAL_KEYCHAIN;
        let account = state.account;
        let index = state.next_internal_index;
        let parent_key = &state.keychains[keychain as usize];

        let (address, next_index) =
            self.gen_address(label, parent_key, account, keychain, index)?;

        state.next_internal_index = next_index;

        Ok(address)
    }

    fn _index_transaction(
        &self,
        state: &mut State,
        txn: &model::ExtendedTransaction,
        block_info: &model::Beacon,
    ) -> Result<()> {
        // Inputs and outputs from different transaction types
        let (inputs, outputs) = match &txn.transaction {
            types::Transaction::ValueTransfer(vt) => {
                (vt.body.inputs.clone(), vt.body.outputs.clone())
            }
            types::Transaction::DataRequest(dr) => {
                (dr.body.inputs.clone(), dr.body.outputs.clone())
            }
            types::Transaction::Commit(commit) => {
                (commit.body.collateral.clone(), commit.body.outputs.clone())
            }
            types::Transaction::Tally(tally) => (vec![], tally.outputs.clone()),
            types::Transaction::Mint(mint) => (vec![], vec![mint.output.clone()]),
            _ => {
                return Err(Error::UnsupportedTransactionType(format!(
                    "{:?}",
                    txn.transaction
                )));
            }
        };

        // Wallet's account mutation (utxo set + balance changes)
        let account_mutation = self._get_account_mutation(
            &state.utxo_set,
            txn.transaction.hash().as_ref(),
            &inputs,
            &outputs,
        )?;

        // If UTXO set has changed, then update memory state and DB
        if !account_mutation.utxo_inserts.is_empty() || !account_mutation.utxo_removals.is_empty() {
            let account = 0;
            let mut db_utxo_set: model::UtxoSet = self
                .db
                .get(&keys::account_utxo_set(account))
                .unwrap_or_default();

            // New account's balance
            let new_balance = match account_mutation.kind {
                model::MovementType::Positive => state
                    .balance
                    .checked_add(account_mutation.amount)
                    .ok_or_else(|| Error::TransactionBalanceOverflow)?,
                model::MovementType::Negative => state
                    .balance
                    .checked_sub(account_mutation.amount)
                    .ok_or_else(|| Error::TransactionBalanceUnderflow)?,
            };

            // Next transaction ID
            let txn_id = state.transaction_next_id;
            let txn_next_id = txn_id
                .checked_add(1)
                .ok_or_else(|| Error::TransactionIdOverflow)?;

            // Update memory state
            for pointer in &account_mutation.utxo_removals {
                state.utxo_set.remove(pointer);
                db_utxo_set.remove(pointer);
            }
            for (pointer, key_balance) in &account_mutation.utxo_inserts {
                state.utxo_set.insert(pointer.clone(), key_balance.clone());
                db_utxo_set.insert(pointer.clone(), key_balance.clone());
            }
            state.transaction_next_id = txn_next_id;
            state.balance = new_balance;

            // DB write batch
            let mut batch = self.db.batch();
            let txn_hash = txn.transaction.hash();

            // Transaction indexes
            batch.put(keys::transactions_index(txn_hash.as_ref()), txn_id)?;
            batch.put(keys::transaction_hash(account, txn_id), txn_hash.as_ref())?;

            // Miner fee
            let miner_fee: u64 = match &txn.metadata {
                Some(model::TransactionMetadata::InputValues(input_values)) => {
                    let total_input_amount = input_values.iter().fold(0, |acc, x| acc + x.value);
                    let total_output_amount = outputs.iter().fold(0, |acc, x| acc + x.value);

                    total_input_amount
                        .checked_sub(total_output_amount)
                        .unwrap_or_else(|| {
                            log::warn!("Miner fee below 0 in a transaction of type value transfer or data request: {:?}", txn.transaction);

                            0
                        })
                }
                _ => 0,
            };

            // Transaction Movement
            let balance_movement = build_balance_movement(
                &txn,
                miner_fee,
                account_mutation.kind,
                account_mutation.amount,
                &block_info,
                convert_block_epoch_to_timestamp(state.epoch_constants, block_info.epoch),
            )?;
            batch.put(
                keys::transaction_movement(account, txn_id),
                balance_movement,
            )?;

            // Try to update data request tally report if it was previously indexed
            if let types::Transaction::Tally(tally) = &txn.transaction {
                log::debug!(
                    "Updating data request transaction {} with report from tally transaction {}",
                    tally.dr_pointer.to_string(),
                    tally.hash().to_string()
                );
                // Try to retrieve data request balance movement
                let txn_id_opt = self
                    .db
                    .get_opt::<_, u32>(&keys::transactions_index(tally.dr_pointer.as_ref()))
                    .unwrap_or(None);
                let dr_movement_opt = txn_id_opt.and_then(|txn_id| {
                    self.db
                        .get_opt::<_, model::BalanceMovement>(&keys::transaction_movement(
                            account, txn_id,
                        ))
                        .unwrap_or(None)
                });

                // Update data request tally report if previously indexed
                if let Some(dr_movement) = dr_movement_opt {
                    match &dr_movement.transaction.data {
                        model::TransactionData::DataRequest(dr_data) => {
                            let mut new_dr_movement = dr_movement.clone();
                            new_dr_movement.transaction.data = model::TransactionData::DataRequest(model::DrData {
                                inputs: dr_data.inputs.clone(),
                                outputs: dr_data.outputs.clone(),
                                tally: Some(build_tally_report(tally, &txn.metadata)?)
                            });
                            batch.put(
                                keys::transaction_movement(account, txn_id_opt.unwrap()),
                                new_dr_movement,
                            )?;
                        },
                        _ => log::warn!("data request tally update failed because wrong stored data type (txn: {})", tally.dr_pointer),
                    }
                } else {
                    log::debug!(
                        "data request tally update not required it was not indexed (txn: {})",
                        tally.dr_pointer
                    )
                }
            }

            // Account state
            batch.put(keys::account_balance(account), new_balance)?;
            batch.put(keys::account_utxo_set(account), db_utxo_set)?;
            batch.put(keys::transaction_next_id(account), txn_next_id)?;

            self.db.write(batch)?;
        }

        Ok(())
    }

    fn _get_account_mutation(
        &self,
        utxo_set: &model::UtxoSet,
        txn_hash: &[u8],
        inputs: &[types::TransactionInput],
        outputs: &[types::VttOutput],
    ) -> Result<AccountMutation> {
        let mut utxo_removals: Vec<model::OutPtr> = vec![];
        let mut utxo_inserts: Vec<(model::OutPtr, model::KeyBalance)> = vec![];

        let mut input_amount: u64 = 0;
        for input in inputs.iter() {
            let out_ptr: model::OutPtr = input.output_pointer().into();

            if let Some(model::KeyBalance { amount, .. }) = utxo_set.get(&out_ptr) {
                input_amount = input_amount
                    .checked_add(*amount)
                    .ok_or_else(|| Error::TransactionBalanceOverflow)?;
                utxo_removals.push(out_ptr);
            }
        }

        let mut output_amount: u64 = 0;
        for (index, output) in outputs.iter().enumerate() {
            if let Some(model::Path { .. }) = self.db.get_opt(&keys::pkh(&output.pkh))? {
                let out_ptr = model::OutPtr {
                    txn_hash: txn_hash.to_vec(),
                    output_index: u32::try_from(index).unwrap(),
                };
                let key_balance = model::KeyBalance {
                    pkh: output.pkh,
                    amount: output.value,
                };
                output_amount = output_amount
                    .checked_add(output.value)
                    .ok_or_else(|| Error::TransactionBalanceOverflow)?;

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
                utxo_inserts.push((out_ptr, key_balance));
            }
        }

        let (amount, kind) = if output_amount >= input_amount {
            (output_amount - input_amount, model::MovementType::Positive)
        } else {
            (input_amount - output_amount, model::MovementType::Negative)
        };

        Ok(AccountMutation {
            kind,
            amount,
            utxo_removals,
            utxo_inserts,
        })
    }

    fn _gen_external_address(
        &self,
        state: &mut State,
        label: Option<String>,
    ) -> Result<model::Address> {
        let keychain = constants::EXTERNAL_KEYCHAIN;
        let account = state.account;
        let index = state.next_external_index;
        let parent_key = &state.keychains[keychain as usize];

        let (address, next_index) =
            self.gen_address(label, parent_key, account, keychain, index)?;

        state.next_external_index = next_index;

        Ok(address)
    }

    /// Get previously created Transaction by its hash.
    pub fn get_db_transaction(&self, hex_hash: &str) -> Result<Option<types::Transaction>> {
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
        let public_key = ExtendedPK::from_secret_key(&self.engine, &parent_key)
            .key
            .to_string();

        let hashed_data = calculate_sha256(data.as_bytes());
        let signature =
            signature::sign(&self.engine, parent_key.secret_key, hashed_data.as_ref())?.to_string();

        Ok(model::ExtendedKeyedSignature {
            chaincode,
            public_key,
            signature,
        })
    }

    /// Update which was the epoch of the last block that was processed by this wallet.
    pub fn update_last_sync(&self, beacon: CheckpointBeacon) -> Result<()> {
        log::debug!(
            "Setting tip of the chain for wallet {} to {:?}",
            self.id,
            beacon
        );

        if let Ok(mut write_guard) = self.state.write() {
            write_guard.last_sync = beacon;
        }

        self.db
            .put(&keys::wallet_last_sync(), beacon)
            .map_err(Error::from)
    }
}

fn convert_block_epoch_to_timestamp(epoch_constants: EpochConstants, epoch: Epoch) -> i64 {
    // In case of error, return timestamp 0
    epoch_constants.epoch_timestamp(epoch).unwrap_or(0)
}

// Balance Movement Factory
fn build_balance_movement(
    txn: &model::ExtendedTransaction,
    miner_fee: u64,
    kind: model::MovementType,
    amount: u64,
    block_info: &model::Beacon,
    timestamp: i64,
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
        types::Transaction::ValueTransfer(vtt) => {
            model::TransactionData::ValueTransfer(model::VtData {
                inputs: transaction_inputs,
                outputs: vtt
                    .body
                    .outputs
                    .clone()
                    .into_iter()
                    .map(|output| model::Output {
                        address: output.pkh.to_string(),
                        time_lock: output.time_lock,
                        value: output.value,
                    })
                    .collect::<Vec<model::Output>>(),
            })
        }
        types::Transaction::DataRequest(dr) => model::TransactionData::DataRequest(model::DrData {
            inputs: transaction_inputs,
            outputs: dr
                .body
                .outputs
                .clone()
                .into_iter()
                .map(|output| model::Output {
                    address: output.pkh.to_string(),
                    time_lock: output.time_lock,
                    value: output.value,
                })
                .collect::<Vec<model::Output>>(),
            tally: None,
        }),
        types::Transaction::Commit(commit) => model::TransactionData::Commit(model::VtData {
            inputs: transaction_inputs,
            outputs: commit
                .body
                .outputs
                .clone()
                .into_iter()
                .map(|output| model::Output {
                    address: output.pkh.to_string(),
                    time_lock: output.time_lock,
                    value: output.value,
                })
                .collect::<Vec<model::Output>>(),
        }),
        types::Transaction::Mint(mint) => model::TransactionData::Mint(model::MintData {
            output: model::Output {
                address: mint.output.pkh.to_string(),
                time_lock: mint.output.time_lock,
                value: mint.output.value,
            },
        }),
        types::Transaction::Tally(tally) => model::TransactionData::Tally(model::TallyData {
            request_transaction_hash: tally.dr_pointer.to_string(),
            outputs: tally
                .outputs
                .clone()
                .into_iter()
                .map(|output| model::Output {
                    address: output.pkh.to_string(),
                    time_lock: output.time_lock,
                    value: output.value,
                })
                .collect::<Vec<model::Output>>(),
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
        kind,
        amount,
        transaction: model::Transaction {
            hash: hex::encode(txn.transaction.hash()),
            timestamp,
            block: Some(model::Beacon {
                epoch: block_info.epoch,
                block_hash: block_info.block_hash,
            }),
            miner_fee,
            data: transaction_data,
        },
    })
}

fn build_tally_report(
    tally: &types::TallyTransaction,
    metadata: &Option<model::TransactionMetadata>,
) -> Result<model::TallyReport> {
    let reveals = match metadata {
        Some(model::TransactionMetadata::Tally(report)) => {
            // List of reveals extracted from Data Request Report
            let mut reveals: HashMap<PublicKeyHash, model::Reveal> = report
                .reveals
                .iter()
                .map(|(pkh, reveal_txn)| {
                    types::RadonTypes::try_from(reveal_txn.body.reveal.as_slice())
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
            for pkh in &tally.slashed_witnesses {
                let liar = reveals.get_mut(&pkh).cloned();
                if let Some(mut reveal) = liar {
                    reveal.in_consensus = false;
                } else {
                    reveals.insert(
                        pkh.clone(),
                        model::Reveal {
                            value: types::RadonTypes::from(
                                RadonError::try_from(types::RadError::NoReveals).unwrap(),
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
        result: types::RadonTypes::try_from(tally.tally.as_slice())
            .map_err(|err| Error::TallyRadDecode(err.to_string()))?
            .to_string(),
        reveals,
    })
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
