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

    pub fn flush_db(&self, db: &Db<'_>) -> Result<()> {
        db.flush()
    }

    pub fn wallet_infos(&self, db: &Db<'_>) -> Result<Vec<model::WalletInfo>> {
        db.get_or_default(&wallets_key())
    }

    pub fn create_wallet(
        &mut self,
        db: Db<'_>,
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
        let db = db.with_key(&key, &self.params);
        let mut batch = db.batch();

        batch.merge(&wallets_key(), &info)?;
        batch.put(&wallet_info_key(&id), &info)?;
        batch.put(&salt_key(&id), salt)?;

        batch.put_enc(&accounts_key(&id), &accounts)?;
        batch.put_enc(&account_key(&id, account.index), &account)?;

        db.write(batch)?;

        Ok(())
    }

    pub fn gen_address(
        &mut self,
        db: Db<'_>,
        wallet: &types::WalletUnlocked,
        label: Option<String>,
    ) -> Result<types::Address> {
        let db = db.with_key(&wallet.enc_key, &self.params);
        let mut batch = db.batch();
        let index_key = &address_index_key(&wallet.info.id, wallet.account.index);

        // FIXME: This update should be done atomic using a transaction
        let index = db.get_or_default_dec::<u32>(index_key)?;
        let new_index = index.checked_add(1).ok_or_else(|| Error::IndexOverflow)?;
        batch.put_enc(index_key, &new_index)?;

        let keypath = &types::KeyPath::new().index(index);
        let extended_sk = wallet.account.external.key.derive(&self.engine, keypath)?;
        let types::ExtendedPK { key, .. } =
            types::ExtendedPK::from_secret_key(&self.engine, &extended_sk);

        match calculate_sha256(&key.serialize_uncompressed()) {
            Sha256(hash) => {
                let pkh = hash[..20].to_vec();
                let address = bech32::encode(
                    if self.params.testnet { "twit" } else { "wit" },
                    pkh.to_base32(),
                )?;

                let mut pkhs =
                    db.get_or_default_dec::<Vec<_>>(&wallet_pkhs_key(&wallet.info.id))?;

                pkhs.push(pkh.clone());

                batch.put_enc(&wallet_pkhs_key(&wallet.info.id), &pkhs)?;
                batch.put_enc(
                    &address_key(&wallet.info.id, wallet.account.index, index),
                    &model::ReceiveKey { pkh, index, label },
                )?;

                db.write(batch)?;

                Ok(types::Address {
                    address,
                    path: format!("{}/{}", wallet.account.external.path, index),
                })
            }
        }
    }

    pub fn unlock_wallet(
        &mut self,
        db: Db<'_>,
        wallet_id: &str,
        password: &[u8],
    ) -> Result<types::WalletUnlocked> {
        let salt = &db.get::<Vec<u8>>(&salt_key(wallet_id))?;
        let enc_key = self.key_from_password(password, salt);
        let db = db.with_key(&enc_key, &self.params);
        let session_id = self.gen_session_id(&enc_key, salt);

        let info = db.get::<model::WalletInfo>(&wallet_info_key(wallet_id))?;
        let accounts = db.get_dec::<model::Accounts>(&accounts_key(wallet_id))?;
        let account = db.get_dec::<model::Account>(&account_key(wallet_id, accounts.current))?;

        Ok(types::WalletUnlocked {
            session_id,
            account,
            info,
            enc_key,
            accounts: accounts.accounts,
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

    pub fn gen_account(&self, master_key: &ExtendedSK) -> Result<model::Account> {
        let account_index = 0;
        let account_keypath = KeyPath::new()
            .hardened(3)
            .hardened(4919)
            .hardened(account_index);

        let account_key = master_key.derive(&self.engine, &account_keypath)?;

        let external_key = {
            let keypath = KeyPath::new().index(0);

            account_key.derive(&self.engine, &keypath)?
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
}
