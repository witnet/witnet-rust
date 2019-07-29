use std::convert::TryFrom;

use bech32::ToBase32 as _;
use rayon::prelude::*;

use witnet_crypto::{
    cipher,
    hash::{calculate_sha256, HashFunction, Sha256},
    key::{ExtendedSK, KeyPath, MasterKeyGen, SignEngine},
    pbkdf2::pbkdf2_sha256,
};

use super::*;
use crate::model;

impl Worker {
    pub fn start(params: Params) -> Addr<Self> {
        let wallets_mutex = Arc::new(Mutex::new(()));
        let addresses_mutex = Arc::new(Mutex::new(()));

        SyncArbiter::start(num_cpus::get(), move || Self {
            params: params.clone(),
            engine: SignEngine::signing_only(),
            rng: RefCell::new(rand::thread_rng()),
            wallets_mutex: wallets_mutex.clone(),
            addresses_mutex: addresses_mutex.clone(),
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

    pub fn flush_db(&self, db: &Db<'_>) -> Result<()> {
        db.flush()
    }

    pub fn wallet_infos(&self, db: &Db<'_>) -> Result<Vec<model::Wallet>> {
        let ids = db.get_or_default::<Vec<String>>(&keys::wallet_ids())?;
        let mut wallets = Vec::with_capacity(ids.len());

        for id in ids {
            let name = db.get_opt(&keys::wallet_name(&id))?;
            let caption = db.get_opt(&keys::wallet_name(&id))?;

            wallets.push(model::Wallet { id, name, caption })
        }

        Ok(wallets)
    }

    pub fn create_wallet(
        &mut self,
        db: Db<'_>,
        name: Option<String>,
        caption: Option<String>,
        password: &[u8],
        source: types::SeedSource,
    ) -> Result<String> {
        let master_key = self.gen_master_key(source)?;
        let account = self.gen_account(&master_key)?;
        let id = self.gen_id(&master_key);
        let salt = &self.salt()?;
        let key = &self.key_from_password(password, salt);
        let db = db.with_key(&key, &self.params);
        let mut batch = db.batch();

        if let Some(name) = name {
            batch.put(&keys::wallet_name(&id), &name)?;
        }
        if let Some(caption) = caption {
            batch.put(&keys::wallet_caption(&id), &caption)?;
        }
        batch.put(&keys::wallet_salt(&id), salt)?;

        batch.put_enc(&keys::wallet_default_account(&id), &account.index)?;
        batch.put_enc(&keys::wallet_accounts(&id), &vec![account.index])?;

        batch.put_enc(&keys::account_ek(&id, account.index), &account.external)?;
        batch.put_enc(&keys::account_ik(&id, account.index), &account.internal)?;
        batch.put_enc(&keys::account_rk(&id, account.index), &account.rad)?;

        db.write(batch)?;

        // FIXME: Use a rocksdb transaction when available in rocksdb crate
        let lock = self.wallets_mutex.lock()?;
        let mut ids = db.get_or_default::<Vec<String>>(&keys::wallet_ids())?;
        if !ids.contains(&id) {
            ids.push(id.clone());
            db.put(&keys::wallet_ids(), &ids)?;
        }
        drop(lock);

        Ok(id)
    }

    pub fn gen_address(
        &mut self,
        db: Db<'_>,
        wallet: &model::WalletUnlocked,
        label: Option<String>,
    ) -> Result<model::Address> {
        // FIXME: Use a rocksdb transaction when available in rocksdb crate
        let _lock = self.addresses_mutex.lock()?;
        let db = db.with_key(&wallet.enc_key, &self.params);
        let mut batch = db.batch();
        let id = &wallet.id;
        let account = wallet.account.index;
        let mut pkhs = db.get_or_default_dec::<Vec<_>>(&keys::wallet_pkhs(&id))?;

        let index = db.get_or_default_dec::<u32>(&keys::account_next_ek_index(&id, account))?;
        let next_index = index.checked_add(1).ok_or_else(|| Error::IndexOverflow)?;
        db.put_enc(&keys::account_next_ek_index(&id, account), &next_index)?;

        let extended_sk = wallet
            .account
            .external
            .derive(&self.engine, &types::KeyPath::default().index(index))?;
        let types::ExtendedPK { key, .. } =
            types::ExtendedPK::from_secret_key(&self.engine, &extended_sk);

        match calculate_sha256(&key.serialize_uncompressed()) {
            Sha256(hash) => {
                let pkh = hash[..20].to_vec();
                let address = bech32::encode(
                    if self.params.testnet { "twit" } else { "wit" },
                    pkh.to_base32(),
                )?;
                let path = format!("{}/0/{}", self.account_keypath(account), index);

                pkhs.push(pkh.clone());

                batch.put_enc(&keys::wallet_pkhs(id), &pkhs)?;
                batch.put_enc(&keys::address(&id, account, index), &address)?;
                batch.put_enc(&keys::address_path(&id, account, index), &path)?;
                batch.put_enc(&keys::address_label(&id, account, index), &label)?;

                db.write(batch)?;

                Ok(model::Address {
                    address,
                    path,
                    label,
                })
            }
        }
    }

    pub fn addresses(
        &mut self,
        db: Db<'_>,
        wallet: &model::WalletUnlocked,
        offset: u32,
        limit: u32,
    ) -> Result<model::Addresses> {
        let db = db.with_key(&wallet.enc_key, &self.params);
        let id = &wallet.id;
        let account = wallet.account.index;
        let last_index =
            db.get_or_default_dec::<u32>(&keys::account_next_ek_index(&id, account))?;

        let end = last_index.saturating_sub(offset);
        let start = end.saturating_sub(limit);
        let range = start..end;
        let mut addresses = Vec::with_capacity(range.len());

        for index in range.rev() {
            let address = db.get_dec(&keys::address(&id, account, index))?;
            let path = db.get_dec(&keys::address_path(&id, account, index))?;
            let label = db.get_dec(&keys::address_label(&id, account, index))?;

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

    pub fn unlock_wallet(
        &mut self,
        db: Db<'_>,
        wallet_id: &str,
        password: &[u8],
    ) -> Result<model::WalletUnlocked> {
        let salt = &db.get::<Vec<u8>>(&keys::wallet_salt(wallet_id))?;
        let enc_key = self.key_from_password(password, salt);
        let db = db.with_key(&enc_key, &self.params);
        let session_id = self.gen_session_id(&enc_key, salt);

        let name = db.get_opt::<String>(&keys::wallet_name(wallet_id))?;
        let caption = db.get_opt::<String>(&keys::wallet_caption(wallet_id))?;
        let accounts = db.get_dec::<Vec<u32>>(&keys::wallet_accounts(wallet_id))?;
        let account = db.get_dec::<u32>(&keys::wallet_default_account(wallet_id))?;

        let external = db.get_dec(&keys::account_ek(&wallet_id, account))?;
        let internal = db.get_dec(&keys::account_ik(&wallet_id, account))?;
        let rad = db.get_dec(&keys::account_rk(&wallet_id, account))?;

        Ok(model::WalletUnlocked {
            name,
            caption,
            session_id,
            accounts,
            enc_key,
            id: wallet_id.to_string(),
            account: model::Account {
                index: account,
                external,
                internal,
                rad,
            },
        })
    }

    pub fn key_from_password(&self, password: &[u8], salt: &[u8]) -> types::Secret {
        pbkdf2_sha256(password, salt, self.params.db_hash_iterations)
    }

    pub fn gen_master_key(&self, source: types::SeedSource) -> Result<ExtendedSK> {
        let key = match source {
            types::SeedSource::Mnemonics(mnemonic) => {
                let seed = mnemonic.seed(&self.params.seed_password);

                MasterKeyGen::new(seed)
                    .with_key(self.params.master_key_salt.as_ref())
                    .generate()?
            }
            types::SeedSource::Xprv => {
                // TODO: Implement key generation from xprv
                unimplemented!("xprv not implemented yet")
            }
        };

        Ok(key)
    }

    pub fn account_keypath(&self, index: u32) -> KeyPath {
        KeyPath::default()
            .hardened(3)
            .hardened(4919)
            .hardened(index)
    }

    pub fn gen_account(&self, master_key: &ExtendedSK) -> Result<model::Account> {
        let account_index = 0;
        let account_keypath = self.account_keypath(account_index);

        let account_key = master_key.derive(&self.engine, &account_keypath)?;

        let external = {
            let keypath = KeyPath::default().index(0);

            account_key.derive(&self.engine, &keypath)?
        };
        let internal = {
            let keypath = KeyPath::default().index(1);

            account_key.derive(&self.engine, &keypath)?
        };
        let rad = {
            let keypath = KeyPath::default().index(2);

            account_key.derive(&self.engine, &keypath)?
        };

        let account = model::Account {
            index: account_index,
            external,
            internal,
            rad,
        };

        Ok(account)
    }

    pub fn gen_id(&self, master_key: &ExtendedSK) -> String {
        match self.params.id_hash_function {
            HashFunction::Sha256 => {
                let password = master_key.concat();
                let id_bytes = pbkdf2_sha256(
                    password.as_ref(),
                    self.params.master_key_salt.as_ref(),
                    self.params.id_hash_iterations,
                );

                hex::encode(id_bytes)
            }
        }
    }

    pub fn gen_session_id(&self, key: &[u8], salt: &[u8]) -> String {
        match self.params.id_hash_function {
            HashFunction::Sha256 => {
                let rand_bytes: [u8; 32] = self.rng.borrow_mut().gen();
                let password = [key, salt, rand_bytes.as_ref()].concat();
                let id_bytes = pbkdf2_sha256(
                    &password,
                    &self.params.master_key_salt,
                    self.params.id_hash_iterations,
                );

                hex::encode(id_bytes)
            }
        }
    }

    pub fn salt(&self) -> Result<Vec<u8>> {
        let salt = cipher::generate_random(self.params.db_salt_length)?;

        Ok(salt)
    }
}
