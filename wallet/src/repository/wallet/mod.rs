use std::sync::RwLock;

use super::*;
use crate::{
    account, constants,
    db::{Database, WriteBatch as _},
    model,
    params::Params,
    repository,
    types::{self, Hashable as _},
};

mod state;
#[cfg(test)]
mod tests;

use crate::types::{signature, ExtendedPK};
use state::State;
use std::convert::TryFrom;
use witnet_crypto::hash::calculate_sha256;
use witnet_data_structures::chain::{Environment, Epoch, EpochConstants};

/// Internal structure used to gather state mutations while indexing block transactions
struct TransactionMutation {
    txn_balance: i64,
    utxo_removals: Vec<model::OutPtr>,
    utxo_inserts: Vec<(model::OutPtr, model::KeyBalance)>,
}

pub struct Wallet<T> {
    db: T,
    params: Params,
    engine: types::CryptoEngine,
    state: RwLock<State>,
}

impl<T> Wallet<T>
where
    T: Database,
{
    pub fn unlock(db: T, params: Params, engine: types::CryptoEngine) -> Result<Self> {
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

        let last_sync = db.get(&keys::wallet_last_sync()).unwrap_or(0);
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
            db,
            params,
            engine,
            state,
        })
    }

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
    pub fn get_transaction(&self, account: u32, index: u32) -> Result<model::Transaction> {
        let value = self.db.get(&keys::transaction_value(account, index))?;
        let kind = self.db.get(&keys::transaction_type(account, index))?;
        let timestamp = self.db.get(&keys::transaction_timestamp(account, index))?;
        let hash: Vec<u8> = self.db.get(&keys::transaction_hash(account, index))?;
        let fee = self.db.get_opt(&keys::transaction_fee(account, index))?;
        let block = self.db.get_opt(&keys::transaction_block(account, index))?;

        Ok(model::Transaction {
            value,
            kind,
            hex_hash: hex::encode(hash),
            fee,
            block,
            timestamp,
        })
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

    /// Index transactions in a block received from a node.
    pub fn index_transactions(
        &self,
        block_info: &model::BlockInfo,
        txns: &[types::Transaction],
    ) -> Result<()> {
        let mut state = self.state.write()?;

        for txn in txns {
            // Check if transaction already exists in the database
            let hash = txn.hash().as_ref().to_vec();
            match self
                .db
                .get_opt::<_, u32>(&keys::transactions_index(&hash))?
            {
                None => self._index_transaction(&mut state, txn, block_info)?,
                Some(_) => log::warn!(
                    "The transaction {} already exists in the database",
                    txn.hash()
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
        txn: &types::Transaction,
        block_info: &model::BlockInfo,
    ) -> Result<()> {
        // For each type of transaction, get mutations and transaction balance
        let transaction_mutation = match txn {
            types::Transaction::DataRequest(txn) => self._check_txn_balance(
                &state.utxo_set,
                txn.hash().as_ref(),
                &txn.body.inputs,
                &txn.body.outputs,
            )?,
            types::Transaction::ValueTransfer(txn) => self._check_txn_balance(
                &state.utxo_set,
                txn.hash().as_ref(),
                &txn.body.inputs,
                &txn.body.outputs,
            )?,
            _ => return Err(Error::UnsupportedTransactionType(format!("{:?}", txn))),
        };

        // If UTXO set changes, then update memory state and DB
        if !transaction_mutation.utxo_inserts.is_empty()
            || !transaction_mutation.utxo_removals.is_empty()
        {
            let account = 0;
            let mut db_utxo_set: model::UtxoSet = self
                .db
                .get(&keys::account_utxo_set(account))
                .unwrap_or_default();

            // New account's balance
            let new_balance = if transaction_mutation.txn_balance > 0 {
                state
                    .balance
                    .checked_add(transaction_mutation.txn_balance.abs() as u64)
                    .ok_or_else(|| Error::TransactionBalanceOverflow)?
            } else {
                state
                    .balance
                    .checked_sub(transaction_mutation.txn_balance.abs() as u64)
                    .ok_or_else(|| Error::TransactionBalanceUnderflow)?
            };

            // Next transaction ID
            let txn_id = state.transaction_next_id;
            let txn_next_id = txn_id
                .checked_add(1)
                .ok_or_else(|| Error::TransactionIdOverflow)?;

            // Update memory state
            transaction_mutation
                .utxo_removals
                .into_iter()
                .for_each(|out_ptr| {
                    state.utxo_set.remove(&out_ptr);
                    db_utxo_set.remove(&out_ptr);
                });
            transaction_mutation
                .utxo_inserts
                .into_iter()
                .for_each(|(out_ptr, key_balance)| {
                    state.utxo_set.insert(out_ptr.clone(), key_balance.clone());
                    db_utxo_set.insert(out_ptr, key_balance);
                });
            state.transaction_next_id = txn_next_id;
            state.balance = new_balance;

            // DB write batch
            let mut batch = self.db.batch();
            let txn_hash = txn.hash();

            // Transaction data
            batch.put(
                &keys::transaction_value(account, txn_id),
                transaction_mutation.txn_balance,
            )?;
            batch.put(
                keys::transaction_type(account, txn_id),
                model::TransactionType::try_from(txn).map_err(|error| {
                    repository::error::Error::UnsupportedTransactionType(error.0)
                })?,
            )?;
            batch.put(
                keys::transaction_timestamp(account, txn_id),
                convert_block_epoch_to_timestamp(state.epoch_constants, block_info.epoch),
            )?;
            batch.put(keys::transaction_block(account, txn_id), block_info)?;

            // Transaction indexes
            batch.put(keys::transactions_index(txn_hash.as_ref()), txn_id)?;
            batch.put(keys::transaction_hash(account, txn_id), txn_hash.as_ref())?;

            // Account state
            batch.put(keys::account_balance(account), new_balance)?;
            batch.put(keys::account_utxo_set(account), db_utxo_set)?;
            batch.put(keys::transaction_next_id(account), txn_next_id)?;

            self.db.write(batch)?;
        }

        Ok(())
    }

    fn _check_txn_balance(
        &self,
        utxo_set: &model::UtxoSet,
        txn_hash: &[u8],
        inputs: &[types::TransactionInput],
        outputs: &[types::VttOutput],
    ) -> Result<TransactionMutation> {
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

        let txn_balance = i128::from(output_amount)
            .checked_sub(i128::from(input_amount))
            .ok_or_else(|| Error::TransactionBalanceUnderflow)?;

        let txn_balance =
            i64::try_from(txn_balance).map_err(|_| Error::TransactionBalanceOverflow)?;

        Ok(TransactionMutation {
            txn_balance,
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
    pub fn get_node_transaction(&self, hex_hash: &str) -> Result<Option<types::Transaction>> {
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
    pub fn update_last_sync(&self, epoch: u32) -> Result<()> {
        self.db
            .put(&keys::wallet_last_sync(), epoch)
            .map_err(Error::from)
    }
}

fn convert_block_epoch_to_timestamp(epoch_constants: EpochConstants, epoch: Epoch) -> i64 {
    // In case of error, return timestamp 0
    epoch_constants.epoch_timestamp(epoch).unwrap_or(0)
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
