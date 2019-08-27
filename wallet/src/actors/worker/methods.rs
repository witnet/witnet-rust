use std::convert::TryFrom;

use jsonrpc_core as rpc;
use rayon::prelude::*;
use serde_json::json;

use super::*;
use crate::{account, crypto, db::Database as _, model, params};

impl Worker {
    pub fn start(concurrency: usize, db: Arc<rocksdb::DB>, params: params::Params) -> Addr<Self> {
        let engine = types::SignEngine::signing_only();
        let wallets = Arc::new(repository::Wallets::new(db::PlainDb::new(db.clone())));

        SyncArbiter::start(concurrency, move || Self {
            db: db.clone(),
            wallets: wallets.clone(),
            params: params.clone(),
            rng: rand_os::OsRng,
            engine: engine.clone(),
        })
    }

    pub fn run_rad_request(&self, request: types::RADRequest) -> Result<types::RadonTypes> {
        let value = request
            .retrieve
            .par_iter()
            .map(witnet_rad::run_retrieval)
            .collect::<result::Result<Vec<_>, _>>()
            .and_then(|retrievals| {
                witnet_rad::run_aggregation(retrievals, &request.aggregate)
                    .map_err(Into::into)
                    .and_then(|aggregated| {
                        types::RadonTypes::try_from(aggregated.as_slice())
                            .and_then(|aggregation_result| {
                                witnet_rad::run_consensus(
                                    vec![aggregation_result],
                                    &request.consensus,
                                )
                                .and_then(|consensus_result| {
                                    types::RadonTypes::try_from(consensus_result.as_slice())
                                })
                            })
                            .map_err(Into::into)
                    })
            })?;

        Ok(value)
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

        let prefix = id.as_bytes().to_vec();
        let salt = crypto::salt(&mut self.rng, self.params.db_salt_length);
        let iv = crypto::salt(&mut self.rng, self.params.db_iv_length);
        let key = crypto::key_from_password(password, &salt, self.params.db_hash_iterations);

        let wallet_db = db::EncryptedDb::new(self.db.clone(), prefix, key, iv.clone());
        wallet_db.put("", ())?; // used when unlocking to check if the password is correct

        self.wallets.create(
            wallet_db,
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

    pub fn unlock_wallet(
        &mut self,
        wallet_id: &str,
        password: &[u8],
    ) -> Result<types::UnlockedSessionWallet> {
        let (salt, iv) = self
            .wallets
            .wallet_salt_and_iv(wallet_id)
            .map_err(|err| match err {
                repository::Error::Db(db::Error::DbKeyNotFound) => Error::WalletNotFound,
                err => Error::Repository(err),
            })?;
        let key = crypto::key_from_password(password, &salt, self.params.db_hash_iterations);
        let session_id = crypto::gen_session_id(
            &mut self.rng,
            &self.params.id_hash_function,
            &key,
            &salt,
            self.params.id_hash_iterations,
        );
        let prefix = wallet_id.as_bytes().to_vec();
        let wallet_db = db::EncryptedDb::new(self.db.clone(), prefix, key, iv);
        wallet_db.get::<_, ()>("").map_err(|err| match err {
            db::Error::DbKeyNotFound => Error::WrongPassword,
            err => Error::Db(err),
        })?;

        let wallet = repository::Wallet::new(wallet_db, self.params.clone(), self.engine.clone());

        let data = wallet.unlock()?;

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
        let address = wallet.gen_address(label)?;

        Ok(address)
    }

    pub fn addresses(
        &mut self,
        wallet: &types::Wallet,
        offset: u32,
        limit: u32,
    ) -> Result<model::Addresses> {
        let addresses = wallet.addresses(offset, limit)?;

        Ok(addresses)
    }

    pub fn transactions(
        &mut self,
        _wallet: &types::Wallet,
        _offset: u32,
        _limit: u32,
    ) -> Result<model::Transactions> {
        let transactions = vec![
            model::Transaction {
                hash: "4f369107485dd195d477818a27d27027b758572cce82078f6789aa6df7d1f295"
                    .to_string(),
                value: 341_958,
                kind: model::TransactionKind::Debit,
            },
            model::Transaction {
                hash: "16c447832f337f78ae282a2e0143368d95ba83f1bf7829b52a853fd0c126b434"
                    .to_string(),
                value: 2349,
                kind: model::TransactionKind::Credit,
            },
            model::Transaction {
                hash: "67086e92250362daeb114ceacc0cbee5fbdd2cb40c2718a6b0b6879702d52d43"
                    .to_string(),
                value: 12,
                kind: model::TransactionKind::Debit,
            },
            model::Transaction {
                hash: "36a50cf934f58255c748e6f1d12f572c5c426a186387f806a1be55ff8fe1b171"
                    .to_string(),
                value: u64::max_value(),
                kind: model::TransactionKind::Credit,
            },
            model::Transaction {
                hash: "ea5d0f4187403bf085937ff8d1fba862923b1b40d4ae188bc52006d895c334df"
                    .to_string(),
                value: 1,
                kind: model::TransactionKind::Debit,
            },
            model::Transaction {
                hash: "4f369107485dd195d477818a27d27027b758572cce82078f6789aa6df7d1f295"
                    .to_string(),
                value: 3958,
                kind: model::TransactionKind::Credit,
            },
        ];
        let total = 20;

        Ok(model::Transactions {
            transactions,
            total,
        })
    }

    pub fn get(&self, wallet: &types::Wallet, key: &str) -> Result<Option<String>> {
        let value = wallet.db_get(key)?;

        Ok(value)
    }

    pub fn set(&self, wallet: &types::Wallet, key: &str, value: &str) -> Result<()> {
        wallet.db_set(key, value)?;

        Ok(())
    }

    pub fn index_txns(
        &self,
        wallet: &types::Wallet,
        txns: &[types::VTTransactionBody],
    ) -> Result<()> {
        wallet.index_txns(txns)?;

        Ok(())
    }

    pub fn notify_balance(&self, wallet: &types::Wallet, sink: &types::Sink) -> Result<()> {
        let (account, balance) = wallet.balance()?;
        let payload = json!({
            "accountBalance": {
                "account": account,
                "balance": balance
            }
        });
        let send = sink.notify(rpc::Params::Array(vec![payload]));

        send.wait()?;

        Ok(())
    }
}
