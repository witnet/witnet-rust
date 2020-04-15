use std::convert::TryFrom;
use std::str::FromStr;

use futures_util::compat::Compat01As03;
use jsonrpc_core as rpc;
use serde_json::json;

use witnet_data_structures::chain::{CheckpointBeacon, Hash, Hashable};

use crate::types::{ChainEntry, GetBlockChainParams};
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

    pub fn run_rad_request(
        &self,
        request: types::RADRequest,
    ) -> types::RadonReport<types::RadonTypes> {
        // Block on data request retrieval because the wallet was designed with a blocking run retrieval in mind.
        // This can be made non-blocking by returning a future here and updating.
        futures03::executor::block_on(witnet_rad::try_data_request(&request))
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
        caption: Option<String>,
        password: &[u8],
        source: &types::SeedSource,
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
            constants::ENCRYPTION_CHECK_KEY,
            constants::ENCRYPTION_CHECK_VALUE,
        )?; // used when unlocking to check if the password is correct

        self.wallets.create(
            &wallet_db,
            types::CreateWalletData {
                name,
                caption,
                iv,
                salt,
                id: &id,
                account: &default_account,
            },
        )?;

        Ok(id)
    }

    /// Update a wallet details.
    pub fn update_wallet(
        &self,
        wallet: &types::Wallet,
        name: Option<String>,
        caption: Option<String>,
    ) -> Result<()> {
        wallet.update(name, caption)?;

        Ok(())
    }

    /// Update the wallet information in the infos database.
    pub fn update_wallet_info(
        &self,
        wallet_id: &str,
        name: Option<String>,
        caption: Option<String>,
    ) -> Result<()> {
        self.wallets.update_info(wallet_id, name, caption)?;

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
            .get(constants::ENCRYPTION_CHECK_KEY)
            .map_err(|err| match err {
                db::Error::DbKeyNotFound { .. } => Error::WrongPassword,
                err => Error::Db(err),
            })?;

        let wallet = repository::Wallet::unlock(
            wallet_id,
            session_id.clone(),
            wallet_db,
            self.params.clone(),
            self.engine.clone(),
        )?;
        let data = wallet.public_data()?;

        Ok(types::UnlockedSessionWallet {
            wallet,
            data,
            session_id,
        })
    }

    pub fn gen_address(
        &mut self,
        wallet: &types::Wallet,
        label: Option<String>,
    ) -> Result<model::Address> {
        let address = wallet.gen_external_address(label)?;

        Ok(address)
    }

    pub fn addresses(
        &mut self,
        wallet: &types::Wallet,
        offset: u32,
        limit: u32,
    ) -> Result<model::Addresses> {
        let addresses = wallet.external_addresses(offset, limit)?;

        Ok(addresses)
    }

    pub fn balance(&mut self, wallet: &types::Wallet) -> Result<model::Balance> {
        let balance = wallet.balance()?;

        Ok(model::Balance {
            available: 0.to_string(),
            confirmed: 0.to_string(),
            unconfirmed: 0.to_string(),
            total: balance.amount.to_string(),
        })
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
        block: &model::Beacon,
        txns: &[types::Transaction],
    ) -> Result<()> {
        let filtered_txns = wallet.filter_wallet_transactions(txns)?;
        log::info!(
            "Indexing {} wallet transactions from epoch {}",
            &filtered_txns.len(),
            block.epoch
        );
        // Extending transactions with metadata queried from the node
        let extended_txns = self.extend_transactions_data(filtered_txns)?;
        wallet.index_transactions(block, &extended_txns)?;
        wallet.update_last_sync(CheckpointBeacon {
            checkpoint: block.epoch,
            hash_prev_block: block.block_hash,
        })?;

        Ok(())
    }

    pub fn notify_status(&self, wallet: &types::Wallet, sink: &types::Sink) -> Result<()> {
        let balance = wallet.balance()?;
        let wallet_data = wallet.public_data()?;
        let payload = json!({
            "account": {
                "id": balance.account,
                "balance": balance.amount,
            },
            "node": {
                "address": "XXX.XXX.XXX.XXX:XXXXX",
                "network": "TX",
                "last_block": {
                    "checkpoint": 1234,
                    "hashPrevBlock": ""
                },
            },
            "wallet": {
                "id": wallet_data.id,
                "last_sync": wallet_data.last_sync,
            },
        });
        let send = sink.notify(rpc::Params::Array(vec![payload]));

        send.wait()?;

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
    ) -> Result<Vec<types::VttOutput>> {
        // Query the node for the required Transactions
        let txn_futures = output_pointers
            .iter()
            .map(|output| self.query_transaction(output.transaction_id.to_string()));
        let retrieve_responses = async { futures03::future::try_join_all(txn_futures).await };
        let transactions: Vec<types::Transaction> =
            futures03::executor::block_on(retrieve_responses)?;
        log::info!(
            "Received {} wallet transactions from node",
            transactions.len()
        );

        let result: Result<Vec<types::VttOutput>> = transactions
            .iter()
            .zip(output_pointers)
            .map(|(txn, output)| match txn {
                types::Transaction::ValueTransfer(vt) => vt
                    .body
                    .outputs
                    .get(output.output_index as usize)
                    .map(types::VttOutput::clone)
                    .ok_or_else(|| {
                        Error::OutputIndexNotFound(output.output_index, format!("{:?}", txn))
                    }),
                types::Transaction::DataRequest(dr) => dr
                    .body
                    .outputs
                    .get(output.output_index as usize)
                    .map(types::VttOutput::clone)
                    .ok_or_else(|| {
                        Error::OutputIndexNotFound(output.output_index, format!("{:?}", txn))
                    }),
                types::Transaction::Tally(tally) => tally
                    .outputs
                    .get(output.output_index as usize)
                    .map(types::VttOutput::clone)
                    .ok_or_else(|| {
                        Error::OutputIndexNotFound(output.output_index, format!("{:?}", txn))
                    }),
                types::Transaction::Mint(mint) => mint
                    .outputs
                    .get(output.output_index as usize)
                    .map(types::VttOutput::clone)
                    .ok_or_else(|| {
                        Error::OutputIndexNotFound(output.output_index, format!("{:?}", txn))
                    }),
                types::Transaction::Commit(commit) => commit
                    .body
                    .outputs
                    .get(output.output_index as usize)
                    .map(types::VttOutput::clone)
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
            .address
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
            .address
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

    /// Try to synchronize the information for a wallet to whatever the world state is in a Witnet
    /// chain.
    pub fn sync(
        &self,
        wallet_id: &str,
        wallet: types::SessionWallet,
        since_beacon: CheckpointBeacon,
        sink: Option<types::Sink>,
    ) -> Result<()> {
        let limit = i64::from(self.params.node_sync_batch_size);
        let mut latest_beacon = since_beacon;
        let mut since_beacon = since_beacon;

        log::info!(
            "[SU] Starting synchronization of wallet {}. Last known beacon is {:?}",
            wallet_id,
            since_beacon
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
            for ChainEntry(epoch, id) in block_chain {
                let get_block_future = self.get_block(id.clone());
                let block: types::ChainBlock = futures03::executor::block_on(get_block_future)?;
                log::debug!("[SU] For epoch {}, got block: {:?}", epoch, block);

                // Process each block
                self.handle_block(block.clone(), wallet.clone(), None)?;
                latest_beacon = CheckpointBeacon {
                    checkpoint: epoch,
                    hash_prev_block: Hash::from_str(&id)?,
                };
            }

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

        if let Some(sink) = sink.as_ref() {
            log::trace!("[SU] Notifying balance of wallet {}", wallet_id,);
            self.notify_status(&wallet, sink).ok();
        } else {
            log::trace!("[SU] No sinks need to be notified for wallet {}", wallet_id);
        }

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
            .address
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
                log::warn!("getBlockChain request failed: {}", &err);
                err
            });

        Compat01As03::new(f).await
    }

    /// Ask a Witnet node for the contents of a single block.
    pub async fn get_block(&self, block_id: String) -> Result<types::ChainBlock> {
        log::debug!("Getting block with id {} ", block_id);
        let method = String::from("getBlock");
        let params = block_id;

        let req = types::RpcRequest::method(method)
            .timeout(self.node.requests_timeout)
            .params(params)
            .expect("params failed serialization");
        let f = self
            .node
            .address
            .send(req)
            .flatten()
            .map(|json| {
                log::trace!("getBlock request result: {:?}", json);
                serde_json::from_value::<types::ChainBlock>(json).map_err(node_error)
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
        wallet: types::SessionWallet,
        sink: Option<types::Sink>,
    ) -> Result<()> {
        // Stop processing the block if it does not directly build on top of our tip of the chain
        let block_previous_beacon = block.block_header.beacon.hash_prev_block;
        let local_chain_tip = wallet.public_data()?.last_sync.hash_prev_block;
        if block_previous_beacon != local_chain_tip {
            log::warn!("Tried to process a block that does not build directly on top of our tip of the chain. ({:?} != {:?})", block_previous_beacon, local_chain_tip );
            return Err(block_error(BlockError::NotConnectedToLocalChainTip {
                block_previous_beacon,
                local_chain_tip,
            }));
        }

        // NOTE: Possible enhancement.
        // Maybe is a good idea to use a shared reference Arc instead of cloning this vector of txns
        // if this vector results to be too big, problem is that doing so conflicts with the internal
        // Cell of the txns type which cannot be shared between threads.
        let block_epoch = block.block_header.beacon.checkpoint;
        let block_hash = block.hash();

        // Block transactions to be indexed.
        // NOTE: only `VttTransaction` and `DRTransaction` are currently supported.
        let dr_txns = block
            .txns
            .data_request_txns
            .into_iter()
            .map(types::Transaction::from);
        let vtt_txns = block
            .txns
            .value_transfer_txns
            .into_iter()
            .map(types::Transaction::from);
        let block_txns = dr_txns.chain(vtt_txns).collect::<Vec<types::Transaction>>();

        let block_info = model::Beacon {
            epoch: block_epoch,
            block_hash,
        };
        self.index_txns(wallet.as_ref(), &block_info, block_txns.as_ref())?;

        if let Some(sink) = sink.as_ref() {
            log::trace!("[RT] Notifying balance of wallet {}", wallet.id);
            self.notify_status(&wallet, sink).ok();
        } else {
            log::trace!("[RT] No sinks need to be notified for wallet {}", wallet.id);
        }

        Ok(())
    }
}
