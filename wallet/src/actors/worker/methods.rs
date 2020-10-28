use std::convert::TryFrom;

use futures_util::compat::Compat01As03;
use jsonrpc_core as rpc;
use serde_json::{json, Value};

use witnet_data_structures::chain::ValueTransferOutput;
use witnet_rad::script::RadonScriptExecutionSettings;

use crate::types::{ChainEntry, CheckpointBeacon, DynamicSink, GetBlockChainParams, Hashable};
use crate::{account, constants, crypto, db::Database as _, model, params};

use super::*;

pub enum IndexTransactionQuery {
    InputTransactions(Vec<types::OutputPointer>),
    DataRequestReport(String),
}

impl Worker {
    pub fn start(
        concurrency: usize,
        db: Arc<rocksdb::DB>,
        node: params::NodeParams,
        params: params::Params,
    ) -> Addr<Self> {
        let engine = types::CryptoEngine::new();
        let wallets = Arc::new(repository::Wallets::new(db::PlainDb::new(db.clone())));

        SyncArbiter::start(concurrency, move || Self {
            db: db.clone(),
            wallets: wallets.clone(),
            node: node.clone(),
            params: params.clone(),
            rng: rand::rngs::OsRng,
            engine: engine.clone(),
        })
    }

    pub fn run_rad_request(&self, request: types::RADRequest) -> types::RADRequestExecutionReport {
        witnet_rad::try_data_request(&request, RadonScriptExecutionSettings::enable_all(), None)
    }

    pub fn gen_mnemonic(&self, length: types::MnemonicLength) -> String {
        let mnemonic = types::MnemonicGen::new().with_len(length).generate();
        let words = mnemonic.words();

        words.to_string()
    }

    pub fn flush_db(&self) -> Result<()> {
        self.wallets.flush_db()?;

        Ok(())
    }

    pub fn wallet_infos(&self) -> Result<Vec<model::Wallet>> {
        let wallets = self.wallets.infos()?;

        Ok(wallets)
    }

    pub fn create_wallet(
        &mut self,
        name: Option<String>,
        description: Option<String>,
        password: &[u8],
        source: &types::SeedSource,
        overwrite: bool,
    ) -> Result<String> {
        let master_key = crypto::gen_master_key(
            self.params.seed_password.as_ref(),
            self.params.master_key_salt.as_ref(),
            source,
        )?;
        let id = crypto::gen_wallet_id(
            &self.params.id_hash_function,
            &master_key,
            self.params.master_key_salt.as_ref(),
            self.params.id_hash_iterations,
        );

        // Return error if `overwrite=false` and wallet already exists
        if !overwrite
            && self
                .wallets
                .infos()?
                .into_iter()
                .any(|wallet| wallet.id == id)
        {
            return Err(Error::WalletAlreadyExists(id));
        }

        let default_account_index = 0;
        let default_account =
            account::gen_account(&self.engine, default_account_index, &master_key)?;

        // This is for storage encryption
        let prefix = id.as_bytes().to_vec();
        let salt = crypto::salt(&mut self.rng, self.params.db_salt_length);
        let iv = crypto::salt(&mut self.rng, self.params.db_iv_length);
        let key = crypto::key_from_password(password, &salt, self.params.db_hash_iterations);

        let wallet_db = db::EncryptedDb::new(self.db.clone(), prefix, key, iv.clone());
        wallet_db.put(
            &constants::ENCRYPTION_CHECK_KEY,
            constants::ENCRYPTION_CHECK_VALUE,
        )?; // used when unlocking to check if the password is correct

        self.wallets.create(
            &wallet_db,
            types::CreateWalletData {
                name,
                description,
                iv,
                salt,
                id: &id,
                account: &default_account,
            },
        )?;

        Ok(id)
    }

    /// Check if wallet with given seed source already exists
    pub fn check_wallet_seed(&self, seed: types::SeedSource) -> Result<(bool, String)> {
        let master_key = crypto::gen_master_key(
            self.params.seed_password.as_ref(),
            self.params.master_key_salt.as_ref(),
            &seed,
        )?;
        let id = crypto::gen_wallet_id(
            &self.params.id_hash_function,
            &master_key,
            self.params.master_key_salt.as_ref(),
            self.params.id_hash_iterations,
        );

        Ok((
            self.wallets
                .infos()?
                .into_iter()
                .any(|wallet| wallet.id == id),
            id,
        ))
    }

    /// Update a wallet details.
    pub fn update_wallet(
        &self,
        wallet: &types::Wallet,
        name: Option<String>,
        description: Option<String>,
    ) -> Result<()> {
        wallet.update(name, description)?;

        Ok(())
    }

    /// Update the wallet information in the infos database.
    pub fn update_wallet_info(&self, wallet_id: &str, name: Option<String>) -> Result<()> {
        self.wallets.update_info(wallet_id, name)?;

        Ok(())
    }

    pub fn unlock_wallet(
        &mut self,
        wallet_id: &str,
        password: &[u8],
    ) -> Result<types::UnlockedSessionWallet> {
        let (salt, iv) = self
            .wallets
            .wallet_salt_and_iv(wallet_id)
            .map_err(|err| match err {
                repository::Error::Db(db::Error::DbKeyNotFound { .. }) => Error::WalletNotFound,
                err => Error::Repository(err),
            })?;
        let key = crypto::key_from_password(password, &salt, self.params.db_hash_iterations);
        let session_id: types::SessionId = From::from(crypto::gen_session_id(
            &mut self.rng,
            &self.params.id_hash_function,
            &key,
            &salt,
            self.params.id_hash_iterations,
        ));
        let prefix = wallet_id.as_bytes().to_vec();
        let wallet_db = db::EncryptedDb::new(self.db.clone(), prefix, key, iv);

        // Check if password-derived key is able to read the special stored value
        wallet_db
            .get(&constants::ENCRYPTION_CHECK_KEY)
            .map_err(|err| match err {
                db::Error::DbKeyNotFound { .. } => Error::WrongPassword,
                err => Error::Db(err),
            })?;

        let wallet = Arc::new(repository::Wallet::unlock(
            wallet_id,
            session_id.clone(),
            wallet_db,
            self.params.clone(),
            self.engine.clone(),
        )?);
        let data = wallet.public_data()?;

        Ok(types::UnlockedSessionWallet {
            wallet,
            data,
            session_id,
        })
    }

    /// Generate a wallet's address specifying if it should be external or internal
    pub fn gen_address(
        &mut self,
        wallet: &types::Wallet,
        external: bool,
        label: Option<String>,
    ) -> Result<Arc<model::Address>> {
        let address = if external {
            wallet.gen_external_address(label)?
        } else {
            wallet.gen_internal_address(label)?
        };

        Ok(address)
    }

    pub fn addresses(
        &mut self,
        wallet: &types::Wallet,
        offset: u32,
        limit: u32,
        external: bool,
    ) -> Result<model::Addresses> {
        let addresses = if external {
            wallet.external_addresses(offset, limit)?
        } else {
            wallet.internal_addresses(offset, limit)?
        };

        Ok(addresses)
    }

    pub fn balance(&mut self, wallet: &types::Wallet) -> Result<model::WalletBalance> {
        let balance = wallet.balance()?;

        Ok(balance)
    }

    pub fn transactions(
        &mut self,
        wallet: &types::Wallet,
        offset: u32,
        limit: u32,
    ) -> Result<model::Transactions> {
        let transactions = wallet.transactions(offset, limit)?;

        Ok(transactions)
    }

    pub fn get(&self, wallet: &types::Wallet, key: &str) -> Result<Option<String>> {
        let value = wallet.kv_get(key)?;

        Ok(value)
    }

    pub fn set(&self, wallet: &types::Wallet, key: &str, value: &str) -> Result<()> {
        wallet.kv_set(key, value)?;

        Ok(())
    }

    pub fn index_txns(
        &self,
        wallet: &types::Wallet,
        block_info: &model::Beacon,
        txns: &[types::Transaction],
        confirmed: bool,
        resynchronizing: bool,
    ) -> Result<Vec<model::BalanceMovement>> {
        // If syncing, then re-generate transient addresses if needed
        // Note: this code can be further refactored by only updating the transient addresses
        wallet._sync_address_generation(txns)?;

        let filtered_txns = wallet.filter_wallet_transactions(txns)?;
        log::debug!(
            "Indexing block #{} ({}) with {} transactions ({})",
            block_info.epoch,
            block_info.block_hash,
            &filtered_txns.len(),
            if confirmed { "confirmed" } else { "pending" },
        );
        // Extending transactions with metadata queried from the node
        let extended_txns = self.extend_transactions_data(filtered_txns)?;
        let balance_movements = wallet.index_block_transactions(
            block_info,
            &extended_txns,
            confirmed,
            resynchronizing,
        )?;

        Ok(balance_movements)
    }

    pub fn notify_client(
        &self,
        wallet: &types::Wallet,
        sink: types::DynamicSink,
        events: Option<Vec<types::Event>>,
    ) -> Result<()> {
        // Make a grab of the sink, cloning immediately so as to drop the read lock ASAP.
        let sink = sink
            .read()
            .expect("Read locks should only fail if poisoned")
            .clone();

        if let Some(sink) = sink.as_ref() {
            log::debug!("Notifying status of wallet {}", wallet.id);

            let balance = wallet.balance()?;
            let wallet_data = wallet.public_data()?;
            let client = self.node.get_client();
            let payload = json!({
                "events": events.unwrap_or_default(),
                "status": {
                    "account": {
                        "id": wallet_data.current_account,
                        "balance": balance,
                    },
                    "node": {
                        "address": client.url,
                        "network": self.node.network,
                        "last_beacon": self.node.get_last_beacon(),
                    },
                    "session": wallet.session_id,
                    "wallet": {
                        "id": wallet_data.id,
                        "last_sync": wallet_data.last_sync,
                    },
                },
            });
            let send = sink.notify(rpc::Params::Array(vec![payload]));
            send.wait()?;
        } else {
            log::debug!("No sinks need to be notified for wallet {}", wallet.id);
        }

        Ok(())
    }

    pub fn create_vtt(
        &self,
        wallet: &types::Wallet,
        params: types::VttParams,
    ) -> Result<types::Transaction> {
        let txn = wallet.create_vtt(params)?;

        Ok(types::Transaction::ValueTransfer(txn))
    }

    pub fn get_transaction(
        &self,
        wallet: &types::Wallet,
        transaction_id: String,
    ) -> Result<Option<types::Transaction>> {
        let vtt = wallet.get_db_transaction(&transaction_id)?;

        Ok(vtt)
    }

    pub fn create_data_req(
        &self,
        wallet: &types::Wallet,
        params: types::DataReqParams,
    ) -> Result<types::Transaction> {
        let txn = wallet.create_data_req(params)?;

        Ok(types::Transaction::DataRequest(txn))
    }

    pub fn sign_data(
        &self,
        wallet: &types::Wallet,
        data: &str,
        extended_pk: bool,
    ) -> Result<model::ExtendedKeyedSignature> {
        let signed_data = wallet.sign_data(&data, extended_pk)?;

        Ok(signed_data)
    }

    /// Extend transactions with metadata requested to the node through JSON-RPC queries.
    pub fn extend_transactions_data(
        &self,
        txns: Vec<types::Transaction>,
    ) -> Result<Vec<model::ExtendedTransaction>> {
        let queries: Vec<Option<IndexTransactionQuery>> = txns
            .iter()
            .map(|txn| match txn {
                types::Transaction::ValueTransfer(vt) => {
                    Some(IndexTransactionQuery::InputTransactions(
                        vt.body
                            .inputs
                            .iter()
                            .map(|input| input.output_pointer().clone())
                            .collect(),
                    ))
                }
                types::Transaction::DataRequest(dr) => {
                    Some(IndexTransactionQuery::InputTransactions(
                        dr.body
                            .inputs
                            .iter()
                            .map(|input| input.output_pointer().clone())
                            .collect(),
                    ))
                }
                types::Transaction::Commit(commit) => {
                    Some(IndexTransactionQuery::InputTransactions(
                        commit
                            .body
                            .collateral
                            .iter()
                            .map(|input| input.output_pointer().clone())
                            .collect(),
                    ))
                }
                types::Transaction::Tally(tally) => Some(IndexTransactionQuery::DataRequestReport(
                    tally.dr_pointer.to_string(),
                )),
                _ => None,
            })
            .collect();

        let query_results: Result<Vec<model::ExtendedTransaction>> = txns
            .into_iter()
            .zip(queries)
            .map(|(transaction, opt_query)| match opt_query {
                Some(query) => match query {
                    IndexTransactionQuery::InputTransactions(pointers) => {
                        Ok(model::ExtendedTransaction {
                            transaction,
                            metadata: Some(model::TransactionMetadata::InputValues(
                                self.get_vt_outputs_from_pointers(&pointers)?,
                            )),
                        })
                    }
                    IndexTransactionQuery::DataRequestReport(dr_id) => {
                        let retrieve_responses =
                            async { self.query_data_request_report(dr_id).await };
                        let report = futures03::executor::block_on(retrieve_responses)?;

                        Ok(model::ExtendedTransaction {
                            transaction,
                            metadata: Some(model::TransactionMetadata::Tally(Box::new(report))),
                        })
                    }
                },
                None => Ok(model::ExtendedTransaction {
                    transaction,
                    metadata: None,
                }),
            })
            .collect();

        Ok(query_results?)
    }

    /// Retrieve Value Transfer Outputs of a list of Output Pointers (aka input fields in transactions).
    pub fn get_vt_outputs_from_pointers(
        &self,
        output_pointers: &[types::OutputPointer],
    ) -> Result<Vec<ValueTransferOutput>> {
        // Query the node for the required Transactions
        let txn_futures = output_pointers
            .iter()
            .map(|output| self.query_transaction(output.transaction_id.to_string()));
        let retrieve_responses = async { futures03::future::try_join_all(txn_futures).await };
        let transactions: Vec<types::Transaction> =
            futures03::executor::block_on(retrieve_responses)?;
        log::debug!(
            "Retrieved value transfer output information from node (queried {} wallet transactions)",
            transactions.len()
        );

        let result: Result<Vec<ValueTransferOutput>> = transactions
            .iter()
            .zip(output_pointers)
            .map(|(txn, output)| match txn {
                types::Transaction::ValueTransfer(vt) => vt
                    .body
                    .outputs
                    .get(output.output_index as usize)
                    .map(ValueTransferOutput::clone)
                    .ok_or_else(|| {
                        Error::OutputIndexNotFound(output.output_index, format!("{:?}", txn))
                    }),
                types::Transaction::DataRequest(dr) => dr
                    .body
                    .outputs
                    .get(output.output_index as usize)
                    .map(ValueTransferOutput::clone)
                    .ok_or_else(|| {
                        Error::OutputIndexNotFound(output.output_index, format!("{:?}", txn))
                    }),
                types::Transaction::Tally(tally) => tally
                    .outputs
                    .get(output.output_index as usize)
                    .map(ValueTransferOutput::clone)
                    .ok_or_else(|| {
                        Error::OutputIndexNotFound(output.output_index, format!("{:?}", txn))
                    }),
                types::Transaction::Mint(mint) => mint
                    .outputs
                    .get(output.output_index as usize)
                    .map(ValueTransferOutput::clone)
                    .ok_or_else(|| {
                        Error::OutputIndexNotFound(output.output_index, format!("{:?}", txn))
                    }),
                types::Transaction::Commit(commit) => commit
                    .body
                    .outputs
                    .get(output.output_index as usize)
                    .map(ValueTransferOutput::clone)
                    .ok_or_else(|| {
                        Error::OutputIndexNotFound(output.output_index, format!("{:?}", txn))
                    }),
                _ => Err(Error::TransactionTypeNotSupported),
            })
            .collect();

        Ok(result?)
    }

    /// Ask a Witnet node for the contents of a transaction
    pub async fn query_transaction(&self, txn_hash: String) -> Result<types::Transaction> {
        log::debug!("Getting transaction with hash {} ", txn_hash);
        let method = String::from("getTransaction");
        let params = txn_hash;

        let req = types::RpcRequest::method(method)
            .timeout(self.node.requests_timeout)
            .params(params)
            .expect("params failed serialization");
        let f = self
            .node
            .get_client()
            .actor
            .send(req)
            .flatten()
            .map(|json| {
                serde_json::from_value::<types::GetTransactionResponse>(json).map_err(node_error)
            })
            .flatten()
            .map(|txn_output| txn_output.transaction)
            .map_err(|err| {
                log::error!("getTransaction request failed: {}", &err);
                err
            });

        Compat01As03::new(f).await
    }

    /// Ask a Witnet node for the report of a data request.
    pub async fn query_data_request_report(
        &self,
        data_request_id: String,
    ) -> Result<types::DataRequestInfo> {
        log::debug!("Getting data request report with id {} ", data_request_id);
        let method = String::from("dataRequestReport");
        let params = data_request_id;

        let req = types::RpcRequest::method(method)
            .timeout(self.node.requests_timeout)
            .params(params)
            .expect("params failed serialization");
        let f = self
            .node
            .get_client()
            .actor
            .send(req)
            .flatten()
            .map(|json| {
                log::trace!("dataRequestReport request result: {:?}", json);
                serde_json::from_value::<types::DataRequestInfo>(json).map_err(node_error)
            })
            .flatten()
            .map_err(|err| {
                log::warn!("dataRequestReport request failed: {}", &err);
                err
            });

        Compat01As03::new(f).await
    }

    /// Sync wrapper in order to clear transient addresses in case of errors
    pub fn sync(
        &self,
        wallet_id: &str,
        wallet: &types::SessionWallet,
        sink: types::DynamicSink,
        resynchronizing: bool,
    ) -> Result<()> {
        let sync_start = wallet.lock_and_read_state(|state| state.last_sync.checkpoint)?;

        // Generate transient addresses for sync purposes
        wallet.initialize_transient_addresses(
            self.params.sync_address_batch_length,
            self.params.sync_address_batch_length,
        )?;

        let sync_result = self.sync_inner(wallet_id, wallet, sink.clone(), resynchronizing);

        // Clear transient created addresses
        wallet.clear_transient_addresses()?;

        // Notify client if error occurred while syncing
        if let Err(ref e) = sync_result {
            let sync_end = wallet.lock_and_read_state(|state| state.last_sync.checkpoint)?;
            log::error!(
                "Error while synchronizing (start: {}, end:{}): {}",
                sync_start,
                sync_end,
                e
            );

            let events = Some(vec![types::Event::SyncError(sync_start, sync_end)]);
            self.notify_client(&wallet, sink, events).ok();
        }

        sync_result
    }

    /// Try to synchronize the information for a wallet to whatever the world state is in a Witnet
    /// chain.
    pub fn sync_inner(
        &self,
        wallet_id: &str,
        wallet: &types::SessionWallet,
        sink: types::DynamicSink,
        resynchronizing: bool,
    ) -> Result<()> {
        let limit = i64::from(self.params.node_sync_batch_size);

        let wallet_data = wallet.public_data()?;
        let first_beacon = wallet_data.last_confirmed;
        let mut since_beacon = first_beacon;
        let mut latest_beacon = first_beacon;

        // Synchronization bootstrap process to query the last received `last_block`
        // Note: if first sync, the queried block will be the genesis (epoch #0)
        if wallet_data.last_confirmed.checkpoint == 0
            && wallet_data.last_confirmed.hash_prev_block == wallet.get_bootstrap_hash()
        {
            let gen_fut = self.get_block_chain(0, 1);
            let gen_res: Vec<ChainEntry> = futures03::executor::block_on(gen_fut)?;
            let gen_entry = gen_res
                .get(0)
                .expect("A Witnet chain should always have a genesis block");

            let get_gen_future = self.get_block(gen_entry.1.clone());
            let (block, _confirmed) = futures03::executor::block_on(get_gen_future)?;
            log::debug!(
                "[SU] Got block #{}: {:?}",
                block.block_header.beacon.checkpoint,
                block
            );

            // Process genesis block (transactions indexed as confirmed)
            self.handle_block(
                block,
                true,
                resynchronizing,
                wallet.clone(),
                DynamicSink::default(),
            )?;
        }

        // Query the node for the latest block in the chain
        let tip_fut = self.get_block_chain(0, -1);
        let tip_res: Vec<ChainEntry> = futures03::executor::block_on(tip_fut)?;
        let tip = CheckpointBeacon::try_from(
            tip_res
                .get(0)
                .expect("A Witnet chain should always have at least one block"),
        )
        .expect("A Witnet node should present block hashes as 64 hexadecimal characters");

        // Store the tip into the worker in form of beacon
        self.node.update_last_beacon(tip);

        // Return error if the node's tip of the chain is behind ours
        if tip.checkpoint < since_beacon.checkpoint {
            return Err(Error::NodeBehindLocalTip(
                tip.checkpoint,
                since_beacon.checkpoint,
            ));
        }

        // Notify wallet about initial synchronization status (the wallet most likely has an old
        // chain tip)
        let events = Some(vec![types::Event::SyncStart(
            since_beacon.checkpoint,
            tip.checkpoint,
        )]);
        self.notify_client(&wallet, sink.clone(), events).ok();

        log::info!(
            "[SU] Starting synchronization of wallet {}.\n\t[Local beacon] {:?}\n\t[Node height ]   BlockInfo {{ checkpoint: {:?}, block_hash: {:?} }}",
            wallet_id,
            since_beacon,
            tip.checkpoint,
            tip.hash_prev_block
        );

        loop {
            // Ask a Witnet node for epochs and ids for all the blocks that happened AFTER the last
            // one we processed — hence `since_beacon.checkpoint + 1`
            let get_block_chain_future =
                self.get_block_chain(i64::from(since_beacon.checkpoint + 1), limit);

            let block_chain: Vec<ChainEntry> =
                futures03::executor::block_on(get_block_chain_future)?;

            let batch_size = i128::try_from((&block_chain).len()).unwrap();
            log::debug!("[SU] Received chain: {:?}", block_chain);

            // For each of the blocks we have been informed about, ask a Witnet node for its contents
            for ChainEntry(_epoch, id) in block_chain {
                let get_block_future = self.get_block(id.clone());
                let (block, confirmed) = futures03::executor::block_on(get_block_future)?;

                // Process each block and update latest beacon
                self.handle_block(
                    block.clone(),
                    confirmed,
                    resynchronizing,
                    wallet.clone(),
                    DynamicSink::default(),
                )?;
                latest_beacon = block.block_header.beacon;
            }

            let events = Some(vec![types::Event::SyncProgress(
                first_beacon.checkpoint,
                latest_beacon.checkpoint,
                tip.checkpoint,
            )]);
            self.notify_client(&wallet, sink.clone(), events).ok();

            // Keep asking for new batches of blocks until we get less than expected, which signals
            // that there are no more blocks to process.
            if batch_size < i128::from(limit) {
                break;
            } else {
                log::info!(
                    "[SU] Wallet {} is now synced up to beacon {:?}, looking for more blocks...",
                    wallet_id,
                    latest_beacon
                );
                since_beacon = latest_beacon;
            }
        }

        let events = Some(vec![types::Event::SyncFinish(
            first_beacon.checkpoint,
            latest_beacon.checkpoint,
        )]);
        self.notify_client(&wallet, sink, events).ok();

        log::info!(
            "[SU] Wallet {} is now synced up to latest beacon ({:?})",
            wallet_id,
            latest_beacon
        );

        Ok(())
    }

    /// Ask a Witnet node for every block that have been written into the chain after a certain
    /// epoch.
    /// The node is free to choose not to deliver all existing blocks but to do it in chunks. Thus
    /// when syncing it is important that this method is called repeatedly until the response is
    /// empty.
    /// A limit can be required on this side, but take into account that the node is not forced to
    /// honor it.
    pub async fn get_block_chain(&self, epoch: i64, limit: i64) -> Result<Vec<types::ChainEntry>> {
        log::debug!(
            "Getting block chain from epoch {} (limit = {})",
            epoch,
            limit
        );

        let method = String::from("getBlockChain");
        let params = GetBlockChainParams { epoch, limit };
        let req = types::RpcRequest::method(method)
            .timeout(self.node.requests_timeout)
            .params(params)
            .expect("params failed serialization");

        let f = self
            .node
            .get_client()
            .actor
            .send(req)
            .flatten()
            .map(|json| {
                log::trace!("getBlockChain request result: {:?}", json);
                match serde_json::from_value::<Vec<types::ChainEntry>>(json).map_err(node_error) {
                    Ok(blocks) => Ok(blocks),
                    Err(e) => Err(e),
                }
            })
            .flatten()
            .map_err(|err| {
                log::error!("getBlockChain request failed: {}", &err);
                err
            });

        Compat01As03::new(f).await
    }

    /// Ask a Witnet node for the contents of a single block.
    pub async fn get_block(&self, block_id: String) -> Result<(types::ChainBlock, bool)> {
        log::debug!("Getting block with id {} ", block_id);
        let method = String::from("getBlock");
        let params = vec![Value::String(block_id), Value::Bool(false)];

        let req = types::RpcRequest::method(method)
            .timeout(self.node.requests_timeout)
            .params(params)
            .expect("params failed serialization");
        let f = self
            .node
            .get_client()
            .actor
            .send(req)
            .flatten()
            .map(|json| {
                log::trace!("getBlock request result: {:?}", json);
                // Set confirmed to true if the result contains {"confirmed": true}
                let mut confirmed = false;
                if let Some(obj) = json.as_object() {
                    if let Some(c) = obj.get("confirmed") {
                        if let Some(true) = c.as_bool() {
                            confirmed = true;
                        }
                    }
                }
                serde_json::from_value::<types::ChainBlock>(json)
                    .map(|block| (block, confirmed))
                    .map_err(node_error)
            })
            .flatten()
            .map_err(|err| {
                log::warn!("getBlock request failed: {}", &err);
                err
            });

        Compat01As03::new(f).await
    }

    pub fn handle_block(
        &self,
        block: types::ChainBlock,
        confirmed: bool,
        resynchronizing: bool,
        wallet: types::SessionWallet,
        sink: types::DynamicSink,
    ) -> Result<()> {
        let block_beacon = block.block_header.beacon;
        let wallet_data = wallet.public_data()?;
        let last_sync = wallet_data.last_sync;
        let last_confirmed = wallet_data.last_confirmed;

        let (needs_clear_pending, needs_indexing) = if block_beacon.hash_prev_block
            == last_sync.hash_prev_block
            && (block_beacon.checkpoint == 0 || block_beacon.checkpoint > last_sync.checkpoint)
        {
            log::debug!(
                "Processing block #{} that builds directly on top of our tip of the chain #{}",
                block_beacon.checkpoint,
                last_sync.checkpoint,
            );

            (false, true)
        } else if block_beacon.checkpoint > last_confirmed.checkpoint
            && block_beacon.hash_prev_block == last_confirmed.hash_prev_block
        {
            log::debug!(
                "Processing block #{} that builds directly on top of our confirmed tip of the chain #{} (cleaning pending state)",
                block_beacon.checkpoint,
                last_confirmed.checkpoint,
            );

            // New block does not follow our pending tip of the chain (e.g. reorgs)
            // Wallet pending state should be cleared and new block indexed
            (true, true)
        } else if block_beacon.checkpoint == last_confirmed.checkpoint
            && block.hash() == last_confirmed.hash_prev_block
        {
            log::debug!(
                "Tried to process a block #{} that was already confirmed in our chain #{} (cleaning pending state)",
                block_beacon.checkpoint,
                last_confirmed.checkpoint,
            );

            // Wallet pending state might be invalid and it should be cleared for future blocks
            (true, false)
        } else {
            log::warn!(
                "Tried to process a block #{} that does not build directly on top our local (#{}) or confirmed tip (#{})",
                block_beacon.checkpoint,
                last_sync.checkpoint,
                last_confirmed.checkpoint,
            );

            return Err(block_error(BlockError::NotConnectedToLocalChainTip {
                block_previous_beacon: block_beacon.hash_prev_block,
                local_chain_tip: last_sync.hash_prev_block,
            }));
        };

        if needs_clear_pending {
            // Clears pending state: blocks, movements, addresses, utxo_set, balances and last_sync
            wallet.clear_pending_state()?;
        }

        if needs_indexing {
            // Index incoming block and its transactions
            let new_last_sync =
                self.index_block(block, confirmed, resynchronizing, &wallet, sink)?;

            // Update wallet state with the last indexed epoch and block hash
            wallet.update_sync_state(new_last_sync, confirmed)?;
        }

        Ok(())
    }

    /// Handle superblock notification by confirming the transactions of the consolidated blocks
    pub fn handle_superblock(
        &self,
        notification: types::SuperBlockNotification,
        wallet: types::SessionWallet,
        sink: types::DynamicSink,
    ) -> Result<()> {
        log::info!(
            "Superblock #{} notification received. Consolidating {} pending blocks...",
            notification.superblock.index,
            notification.consolidated_block_hashes.len()
        );

        log::debug!(
            "Superblock #{} consolidated block hashes: {:?}",
            notification.superblock.index,
            notification.consolidated_block_hashes
        );

        if wallet.is_syncing()? {
            log::warn!(
                "Superblock #{} notification received. Ignoring superblock because the wallet is still synchronizing...",
                notification.superblock.index,
            );

            return Ok(());
        }

        let consolidated = wallet
            .handle_superblock(&notification.consolidated_block_hashes)
            .map_err(Error::from);

        match consolidated {
            Ok(_) => {
                // Notify consolidation of the persisted blocks
                self.notify_client(
                    &wallet,
                    sink,
                    Some(vec![types::Event::BlocksConsolidate(
                        notification.consolidated_block_hashes,
                    )]),
                )
                .ok();
            }
            Err(e) => {
                log::error!(
                    "Error while persisting blocks confirmed by superblock #{}... trying to re-sync with node\n{}",
                    notification.superblock.index,
                    e
                );

                // Notify orphaning of the blocks that could not be persisted
                self.notify_client(
                    &wallet,
                    sink.clone(),
                    Some(vec![types::Event::BlocksOrphan(
                        notification.consolidated_block_hashes,
                    )]),
                )
                .ok();

                self.sync(&wallet.id, &wallet, sink, false)?
            }
        }

        Ok(())
    }

    pub fn handle_node_status(
        &self,
        status: types::StateMachine,
        wallet: types::SessionWallet,
        sink: types::DynamicSink,
    ) -> Result<()> {
        log::debug!("The node has changed its status into {:?}", status);

        // Notify about the changed node status.
        let events = vec![types::Event::NodeStatus(status)];
        self.notify_client(&wallet, sink, Some(events)).ok();

        Ok(())
    }

    pub fn index_block(
        &self,
        block: types::ChainBlock,
        confirmed: bool,
        resynchronizing: bool,
        wallet: &types::SessionWallet,
        sink: types::DynamicSink,
    ) -> Result<CheckpointBeacon> {
        // NOTE: Possible enhancement.
        // Maybe is a good idea to use a shared reference Arc instead of cloning this vector of txns
        // if this vector results to be too big, problem is that doing so conflicts with the internal
        // Cell of the txns type which cannot be shared between threads.
        let block_hash = block.hash();

        // Immediately update the local reference to the node's last beacon
        let block_own_beacon = CheckpointBeacon {
            checkpoint: block.block_header.beacon.checkpoint,
            hash_prev_block: block_hash,
        };
        self.node.update_last_beacon(block_own_beacon);

        // Block transactions to be indexed.
        // Note: reveal transactions do not change wallet balances
        let vtt_txns = block
            .txns
            .value_transfer_txns
            .into_iter()
            .map(types::Transaction::from);
        let dr_txns = block
            .txns
            .data_request_txns
            .into_iter()
            .map(types::Transaction::from);
        let commit_txns = block
            .txns
            .commit_txns
            .into_iter()
            .map(types::Transaction::from);
        let tally_txns = block
            .txns
            .tally_txns
            .into_iter()
            .map(types::Transaction::from);

        let block_txns = vtt_txns
            .chain(dr_txns)
            .chain(commit_txns)
            .chain(tally_txns)
            .chain(std::iter::once(types::Transaction::Mint(block.txns.mint)))
            .collect::<Vec<types::Transaction>>();

        let block_info = model::Beacon {
            block_hash,
            epoch: block.block_header.beacon.checkpoint,
        };
        let balance_movements = self.index_txns(
            wallet.as_ref(),
            &block_info,
            block_txns.as_ref(),
            confirmed,
            resynchronizing,
        )?;

        // Notify about the new block and every single balance movement found within.
        let mut events = vec![types::Event::Block(block_info)];
        for balance_movement in balance_movements {
            events.push(types::Event::Movement(balance_movement));
        }
        self.notify_client(&wallet, sink, Some(events)).ok();

        Ok(block_own_beacon)
    }

    /// Clear all chain data for a wallet state.
    ///
    /// Proceed with caution, as this wipes the following data entirely:
    /// - Synchronization status
    /// - Balances
    /// - Movements
    /// - Addresses and their metadata
    ///
    /// In order to prevent data race conditions, resyncing is not allowed while a sync or resync
    /// process is already in progress. Accordingly, this function returns whether chain data has
    /// been cleared or not.
    pub fn clear_chain_data_and_resync(
        &self,
        wallet_id: &str,
        wallet: types::SessionWallet,
        sink: DynamicSink,
    ) -> Result<bool> {
        // Do not try to clear chain data and resync if a resynchronization is already in progress
        if !wallet.is_syncing()? {
            wallet.clear_chain_data()?;

            self.sync(wallet_id, &wallet, sink, true).map(|_| true)
        } else {
            Ok(false)
        }
    }
}
