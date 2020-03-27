use jsonrpc_core as rpc;
use serde_json::json;

use super::*;
use crate::actors::worker;
use crate::types::{ChainEntry, GetBlockChainParams};
use crate::{account, constants, crypto, db::Database as _, model, params};
use futures_util::compat::Compat01As03;
use witnet_data_structures::chain::Hashable;

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
            own_address: None,
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
        let session_id = From::from(crypto::gen_session_id(
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

        let wallet =
            repository::Wallet::unlock(wallet_db, self.params.clone(), self.engine.clone())?;
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
        block: &model::BlockInfo,
        txns: &[types::Transaction],
    ) -> Result<()> {
        log::debug!("trying to index txns from epoch {}", block.epoch);
        wallet.index_transactions(block, txns)?;

        Ok(())
    }

    pub fn notify_balance(&self, wallet: &types::Wallet, sink: &types::Sink) -> Result<()> {
        let balance = wallet.balance()?;
        let payload = json!({
            "accountBalance": {
                "account": balance.account,
                "amount": balance.amount,
            }
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
        let vtt = wallet.get_node_transaction(&transaction_id)?;

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

    /// Try to synchronize the information for a wallet to whatever the world state is in a Witnet
    /// chain.
    pub fn sync(
        &self,
        wallet_id: &str,
        wallet: types::SessionWallet,
        since_epoch: u32,
    ) -> Result<()> {
        log::info!("Trying to start wallet synchronization");
        // TODO: read the limit from configuration
        let limit = 100i64;
        let mut latest_epoch = since_epoch;
        let mut since_epoch = i64::from(since_epoch);

        loop {
            // Ask a Witnet node for epochs and ids for all the blocks that we have been missing
            let get_block_chain_future = self.get_block_chain(since_epoch, limit);
            let block_chain = futures03::executor::block_on(get_block_chain_future)?;
            log::debug!("Received chain: {:?}", block_chain);

            // For each of the blocks we have been informed about, ask a Witnet node for its contents
            for ChainEntry(epoch, id) in block_chain {
                let get_block_future = self.get_block(id);
                let block = futures03::executor::block_on(get_block_future)?;
                log::debug!("For epoch {}, got block: {:?}", epoch, block);

                // Process each block
                futures03::executor::block_on(self.handle_block(block, wallet_id, wallet.clone()))?;
                latest_epoch = epoch;
            }

            // Persist the epoch of the last synced block. Doing this by batches instead of by block
            // should be safe under the assumption that the same transaction cannot be processed
            // twice by a single wallet.
            wallet.update_last_sync(latest_epoch)?;

            // Keep asking for new batches of blocks until we get less than expected, which signals
            // that there are no more blocks to process.
            if i64::from(latest_epoch) < since_epoch + limit {
                break;
            } else {
                log::info!(
                    "Wallet {} is now synced up to epoch #{}, looking for more blocks...",
                    wallet_id,
                    latest_epoch
                );
                since_epoch = i64::from(latest_epoch);
            }
        }

        log::info!(
            "Wallet {} is now synced up to latest epoch (#{})",
            wallet_id,
            latest_epoch
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
                log::debug!("getBlockChain request result: {:?}", json);
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
                log::debug!("getBlock request result: {:?}", json);
                serde_json::from_value::<types::ChainBlock>(json).map_err(node_error)
            })
            .flatten()
            .map_err(|err| {
                log::warn!("getBlock request failed: {}", &err);
                err
            });

        Compat01As03::new(f).await
    }

    pub async fn handle_block(
        &self,
        block: types::ChainBlock,
        wallet_id: &str,
        wallet: types::SessionWallet,
    ) -> Result<()> {
        // NOTE: Possible enhancement.
        // Maybe is a good idea to use a shared reference Arc instead of cloning this vector of txns
        // if this vector results to be too big, problem is that doing so conflicts with the internal
        // Cell of the txns type which cannot be shared between threads.
        let block_epoch = block.block_header.beacon.checkpoint;
        let block_hash = block.hash().as_ref().to_vec();

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

        for slf in &self.own_address {
            slf.do_send(worker::IndexTxns(
                wallet_id.to_owned(),
                wallet.clone(),
                block_txns.clone(),
                model::BlockInfo {
                    epoch: block_epoch,
                    hash: block_hash.clone(),
                },
            ));
        }

        Ok(())
    }
}
