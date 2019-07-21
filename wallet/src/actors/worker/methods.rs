use std::convert::TryFrom;

use bech32::ToBase32 as _;
use rayon::prelude::*;

use witnet_crypto::{
    cipher,
    hash::{calculate_sha256, HashFunction, Sha256},
    key::{ExtendedPK, ExtendedSK, KeyPath, MasterKeyGen, SignEngine},
    pbkdf2::pbkdf2_sha256,
};

use super::*;
use crate::model;

impl Worker {
    pub fn start(params: Params) -> Addr<Self> {
        SyncArbiter::start(num_cpus::get(), move || Self {
            params: params.clone(),
            engine: SignEngine::signing_only(),
            rng: RefCell::new(rand::thread_rng()),
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

    pub fn flush_db(&self, db: &Db) -> Result<()> {
        db.flush()
    }

    pub fn wallet_infos(&self, db: &Db) -> Result<Vec<model::WalletInfo>> {
        db.get_or_default(b"wallets")
    }

    pub fn create_wallet(
        &mut self,
        db: &mut Db,
        name: Option<String>,
        caption: Option<String>,
        password: &[u8],
        source: types::SeedSource,
    ) -> Result<()> {
        let master_key = self.gen_master_key(source)?;
        let account = self.gen_account(&master_key)?;
        let id = self.gen_id(&master_key);
        let info = model::WalletInfo {
            id: id.clone(),
            name,
            caption,
        };
        let accounts = model::Accounts {
            accounts: vec![account.index],
            current: account.index,
        };

        let salt = &self.salt()?;
        let key = &self.key_from_password(password, salt);

        db.merge(&wallets_key(), &info)?;
        db.put(&wallet_info_key(&id), &info)?;
        db.put(&salt_key(&id), salt)?;
        db.put(&accounts_key(&id), &self.encrypt(key, &accounts)?)?;
        db.put(
            &account_key(&id, account.index),
            &self.encrypt(key, &account)?,
        )?;

        db.write()?;

        Ok(())
    }

    pub fn gen_address(
        &mut self,
        db: &Db,
        enc_key: &[u8],
        wallet_id: &str,
        label: Option<&String>,
        parent_key: &types::ExtendedSK,
        account: u32,
        index: u32,
    ) -> Result<String> {
        let extended_secret_key =
            parent_key.derive(&self.engine, &types::KeyPath::new().index(index))?;
        let extended_key = types::ExtendedPK::from_secret_key(&self.engine, &extended_secret_key);
        let key = extended_key.key;

        match calculate_sha256(&key.serialize_uncompressed()) {
            Sha256(hash) => {
                let pkh = hash[..20].to_vec();
                let address = bech32::encode(
                    if &self.params.testnet { "twit" } else { "wit" },
                    pkh.to_base32(),
                )?;

                db.put(
                    &address_key(wallet_id, account, index),
                    &model::Address { pkh, index },
                )?;

                Ok(address)
            }
        }
    }

    pub fn unlock_wallet(
        &mut self,
        db: &Db,
        wallet_id: &str,
        password: &[u8],
    ) -> Result<(String, String, types::Wallet)> {
        let salt = &db.get::<Vec<u8>>(&salt_key(wallet_id))?;
        let key = self.key_from_password(password, salt);

        let model::WalletInfo { id, name, caption } =
            db.get::<model::WalletInfo>(&wallet_info_key(wallet_id))?;
        let accounts =
            self.decrypt::<model::Accounts>(&key, &db.get::<Vec<u8>>(&accounts_key(wallet_id))?)?;
        let account = self.decrypt::<model::Account>(
            &key,
            &db.get::<Vec<u8>>(&account_key(wallet_id, accounts.current))?,
        )?;
        let next_address_index = self.decrypt(
            &key,
            &db.get::<Vec<u8>>(&account_address_key(wallet_id, accounts.current))?,
        )?;
        let session_id = self.gen_session_id(&key, salt);
        let wallet = types::Wallet {
            account,
            name,
            caption,
            accounts: accounts.accounts,
            enc_key: key,
        };

        let addr = actors::WalletWorker::start(key); // TODO: to be continued...

        Ok((session_id, id, wallet))
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

    pub fn gen_account(&self, master_key: &ExtendedSK) -> Result<model::Account> {
        let account_index = 0;
        let account_keypath = KeyPath::new()
            .hardened(3)
            .hardened(4919)
            .hardened(account_index);

        let account_key = master_key.derive(&self.engine, &account_keypath)?;

        let external_key = {
            let keypath = KeyPath::new().index(0);
            let key = account_key.derive(&self.engine, &keypath)?;

            ExtendedPK::from_secret_key(&self.engine, &key)
        };
        let internal_key = {
            let keypath = KeyPath::new().index(1);

            account_key.derive(&self.engine, &keypath)?
        };
        let rad_key = {
            let keypath = KeyPath::new().index(2);

            account_key.derive(&self.engine, &keypath)?
        };

        let account = model::Account {
            index: account_index,
            external: model::AccountKey {
                key: external_key,
                path: format!("{}/0", account_keypath),
            },
            internal: model::AccountKey {
                key: internal_key,
                path: format!("{}/1", account_keypath),
            },
            rad: model::AccountKey {
                key: rad_key,
                path: format!("{}/2", account_keypath),
            },
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

    pub fn iv(&self) -> Result<Vec<u8>> {
        let iv = cipher::generate_random(self.params.db_iv_length)?;

        Ok(iv)
    }

    pub fn encrypt<T>(&self, key: &[u8], value: &T) -> Result<Vec<u8>>
    where
        T: serde::Serialize,
    {
        let bytes = bincode::serialize(value)?;
        let iv = self.iv()?;
        let encrypted = cipher::encrypt_aes_cbc(key, &bytes, &iv)?;
        let data = [iv, encrypted].concat();

        Ok(data)
    }

    pub fn decrypt<T>(&self, key: &[u8], value: &[u8]) -> Result<T>
    where
        T: serde::de::DeserializeOwned,
    {
        let len = value.len();

        if len < self.params.db_iv_length {
            Err(Error::InvalidDataLen)?
        }

        let (iv, data) = value.split_at(self.params.db_iv_length);
        let bytes = cipher::decrypt_aes_cbc(key, data, iv)?;
        let value = bincode::deserialize(&bytes)?;

        Ok(value)
    }
}
