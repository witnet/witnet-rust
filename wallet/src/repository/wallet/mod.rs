use std::sync::RwLock;

use super::*;
use crate::types::Hashable as _;
use crate::{
    account, constants,
    db::{Database, WriteBatch as _},
    model,
    params::Params,
    types,
};

mod state;
#[cfg(test)]
mod tests;

use state::State;
use witnet_data_structures::chain::Environment;

pub struct Wallet<T> {
    db: T,
    params: Params,
    engine: types::SignEngine,
    state: RwLock<State>,
}

impl<T> Wallet<T>
where
    T: Database,
{
    pub fn unlock(db: T, params: Params, engine: types::SignEngine) -> Result<Self> {
        let name = db.get_opt(keys::wallet_name())?;
        let caption = db.get_opt(keys::wallet_caption())?;
        let account = db.get_or_default(keys::wallet_default_account())?;
        let available_accounts = db
            .get_opt(keys::wallet_accounts())?
            .unwrap_or_else(|| vec![account]);

        let transaction_next_id = db.get_or_default(&keys::transaction_next_id(account))?;
        let balance = db.get_or_default(&keys::account_balance(account))?;
        let utxo_set = db.get_or_default(&keys::account_utxo_set(account))?;
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

        Ok(types::WalletData {
            name: state.name.clone(),
            caption: state.caption.clone(),
            balance,
            current_account,
            available_accounts: state.available_accounts.clone(),
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
            Environment::Testnet1
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
        let label = self.db.get_opt(&keys::transaction_label(account, index))?;
        let fee = self.db.get_opt(&keys::transaction_fee(account, index))?;
        let block = self.db.get_opt(&keys::transaction_block(account, index))?;

        Ok(model::Transaction {
            value,
            kind,
            hex_hash: hex::encode(hash),
            label,
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

    /// Index transactions in a block received from a node.
    pub fn index_transactions(
        &self,
        block: &model::BlockInfo,
        txns: &[types::VTTransactionBody],
    ) -> Result<()> {
        // FIXME: Handle multiple accounts when indexing transactions
        let account = 0;
        let mut state = self.state.write()?;

        for txn in txns {
            let hash = txn.hash().as_ref().to_vec();

            match self.db.get_opt(&keys::transactions_index(&hash))? {
                Some(txn_id) => {
                    self.db
                        .put(&keys::transaction_block(account, txn_id), block)?;
                }
                None => {
                    for input in &txn.inputs {
                        self._index_transaction_input(&mut state, &hash, &input, block)?;
                    }

                    for (index, output) in txn.outputs.iter().enumerate() {
                        let out_ptr = model::OutPtr {
                            txn_hash: hash.clone(),
                            output_index: index as u32,
                        };
                        self._index_transaction_output(&mut state, &hash, out_ptr, &output, block)?;
                    }
                }
            }
        }

        // TODO: persist changes to db
        // self.db.write(batch)?;

        // TODO: persist new values to state

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
            label,
            time_lock,
        }: types::VttParams,
    ) -> Result<types::VTTransaction> {
        // Gather all the required components for creating the VTT
        let components = self.create_transaction_components(value, fee, Some((pkh, time_lock)))?;

        let body = types::VTTransactionBody::new(components.inputs, components.outputs);
        let sign_data = body.hash();
        let signatures = components
            .sign_keys
            .into_iter()
            .map(|sign_key| {
                let public_key = From::from(types::PK::from_secret_key(&self.engine, &sign_key));
                let signature = From::from(types::signature::sign(sign_key, sign_data.as_ref()));

                types::KeyedSignature {
                    signature,
                    public_key,
                }
            })
            .collect();

        let transaction = types::VTTransaction { body, signatures };
        let transaction_hash = transaction.hash().as_ref().to_vec();
        let transaction_hex_hash = hex::encode(&transaction_hash);

        // Persist the transaction
        let mut state = self.state.write()?;
        let account = state.account;
        let transaction_id = state.transaction_next_id;
        let transaction_next_id = transaction_id
            .checked_add(1)
            .ok_or_else(|| Error::TransactionIdOverflow)?;

        // FIXME: Remove this clone by using a better mechanism such
        // as STM or a persistent map
        let mut new_utxo_set = state.utxo_set.clone();
        for out_ptr in components.used_utxos {
            new_utxo_set
                .remove(&out_ptr)
                .expect("invariant: remove vtt utxo, not found");
        }

        let mut batch = self.db.batch();

        batch.put(keys::account_utxo_set(account), &new_utxo_set)?;

        batch.put(keys::vtt(&transaction_hex_hash), &transaction)?;
        batch.put(
            keys::transaction_timestamp(account, transaction_id),
            chrono::Local::now().timestamp(),
        )?;
        batch.put(keys::transaction_value(account, transaction_id), value)?;
        batch.put(keys::transaction_fee(account, transaction_id), fee)?;
        batch.put(
            keys::transaction_type(account, transaction_id),
            model::TransactionKind::Debit,
        )?;
        if let Some(label) = label {
            batch.put(keys::transaction_label(account, transaction_id), &label)?;
        }
        batch.put(keys::transactions_index(&transaction_hash), transaction_id)?;
        batch.put(
            keys::transaction_hash(account, transaction_id),
            transaction_hash,
        )?;

        self.db.write(batch)?;

        // update wallet state only after db has been updated
        state.transaction_next_id = transaction_next_id;
        state.utxo_set = new_utxo_set;

        Ok(transaction)
    }

    /// Create a new data request transaction using available UTXOs.
    pub fn create_data_req(
        &self,
        types::DataReqParams { label, request }: types::DataReqParams,
    ) -> Result<types::DRTransaction> {
        let fee = 0; // In the case of Data Requests, the fee is already included in its value.
        let _components = self.create_transaction_components(request.value, fee, None)?;

        unimplemented!()
    }

    ///  Create all the necessary componets such as inputs/outputs
    ///  that conforms a Transaction. This method is not pure and it
    ///  will consume UTXOs from the current UTXO set.
    pub fn create_transaction_components(
        &self,
        value: u64,
        fee: u64,
        recipient: Option<(types::PublicKeyHash, u64)>,
    ) -> Result<types::TransactionComponents> {
        let mut state = self.state.write()?;
        let target = value.saturating_add(fee);
        let mut payment = 0u64;
        let mut inputs = Vec::with_capacity(5);
        let mut outputs = Vec::with_capacity(2);
        let mut sign_keys = Vec::with_capacity(5);
        let mut used_utxos = Vec::with_capacity(5);

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
            let parent_key = &state.keychains[keychain as usize];

            let extended_sign_key =
                parent_key.derive(&self.engine, &types::KeyPath::default().index(index))?;

            payment = payment
                .checked_add(key_balance.amount)
                .ok_or_else(|| Error::TransactionValueOverflow)?;
            inputs.push(input);
            sign_keys.push(extended_sign_key.into());
            used_utxos.push(out_ptr.clone());
        }

        if payment < target {
            Err(Error::InsufficientBalance)
        } else {
            let change = payment - target;

            if change > 0 {
                let change_address = self._gen_internal_address(&mut state, None)?;

                outputs.push(types::VttOutput {
                    pkh: change_address.pkh,
                    value: change,
                    time_lock: 0,
                });
            }

            Ok(types::TransactionComponents {
                value,
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

    fn _index_transaction_input(
        &self,
        state: &mut State,
        txn_hash: &[u8],
        input: &types::TransactionInput,
        block: &model::BlockInfo,
    ) -> Result<()> {
        let out_ptr: model::OutPtr = input.output_pointer().into();
        let account = 0;
        let txn_id = state.transaction_next_id;
        let old_balance = state.balance;
        let mut batch = self.db.batch();

        if let Some(model::KeyBalance { amount, .. }) = state.utxo_set.get(&out_ptr).cloned() {
            let new_balance = old_balance
                .checked_rem(amount)
                .ok_or_else(|| Error::BalanceUnderflow)?;
            let mut db_utxo_set: model::UtxoSet = self.db.get(&keys::account_utxo_set(account))?;
            let txn_next_id = txn_id
                .checked_add(1)
                .ok_or_else(|| Error::TransactionValueOverflow)?;

            db_utxo_set.remove(&out_ptr);

            batch.put(&keys::transaction_value(account, txn_id), amount)?;
            batch.put(
                keys::transaction_type(account, txn_id),
                model::TransactionKind::Debit,
            )?;
            batch.put(keys::transaction_block(account, txn_id), block)?;
            batch.put(keys::account_balance(account), new_balance)?;
            batch.put(keys::account_utxo_set(account), db_utxo_set)?;
            batch.put(keys::transaction_next_id(account), txn_next_id)?;
            batch.put(keys::transactions_index(txn_hash), txn_id)?;

            self.db.write(batch)?;

            state.transaction_next_id = txn_next_id;
            state.balance = new_balance;
            state.utxo_set.remove(&out_ptr);
        }

        Ok(())
    }

    fn _index_transaction_output(
        &self,
        state: &mut State,
        txn_hash: &[u8],
        out_ptr: model::OutPtr,
        output: &types::VttOutput,
        block: &model::BlockInfo,
    ) -> Result<()> {
        let pkh = output.pkh;
        let amount = output.value;
        let txn_id = state.transaction_next_id;
        let old_balance = state.balance;
        let mut batch = self.db.batch();

        if let Some(model::Path { account, .. }) = self.db.get_opt(&keys::pkh(&pkh))? {
            let new_balance = old_balance
                .checked_add(amount)
                .ok_or_else(|| Error::BalanceOverflow)?;
            let txn_next_id = txn_id
                .checked_add(1)
                .ok_or_else(|| Error::TransactionValueOverflow)?;
            let mut db_utxo_set: model::UtxoSet = self
                .db
                .get(&keys::account_utxo_set(account))
                .unwrap_or_default();
            let address = pkh.bech32(if self.params.testnet {
                Environment::Testnet1
            } else {
                Environment::Mainnet
            });
            let key_balance = model::KeyBalance { pkh, amount };

            match db_utxo_set.insert(out_ptr.clone(), key_balance.clone()) {
                None => {
                    log::info!(
                        "Found transaction to our address {}! Amount: +{} satowits",
                        address,
                        amount
                    );
                }
                Some(x) => {
                    if x != key_balance {
                        log::info!(
                            "Found transaction to our address {}! Amount: +{} satowits",
                            address,
                            amount
                        );
                    }
                }
            }

            batch.put(&keys::transaction_value(account, txn_id), amount)?;
            batch.put(
                keys::transaction_type(account, txn_id),
                model::TransactionKind::Credit,
            )?;
            batch.put(keys::transaction_block(account, txn_id), block)?;
            batch.put(keys::account_balance(account), new_balance)?;
            batch.put(keys::account_utxo_set(account), db_utxo_set)?;
            batch.put(keys::transaction_next_id(account), txn_next_id)?;
            batch.put(keys::transactions_index(txn_hash), txn_id)?;

            self.db.write(batch)?;

            state.transaction_next_id = txn_next_id;
            state.balance = new_balance;
            state.utxo_set.insert(out_ptr, key_balance);
        }

        Ok(())
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

    /// Get previously created Value Transfer Transaction by its hash.
    pub fn get_vtt(&self, hex_hash: &str) -> Result<types::Transaction> {
        let vtt = self.db.get(&keys::vtt(hex_hash))?;

        Ok(vtt)
    }
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
