use std::sync::Arc;

use super::*;
use crate::types;

#[derive(Clone)]
pub struct Wallets {
    db: Db,
    params: Params,
}

impl Wallets {
    pub fn new(db: rocksdb::DB, params: Params) -> Self {
        Self {
            db: Db::new(Arc::new(db)),
            params,
        }
    }

    pub fn get_wallet_infos(&self) -> Result<Vec<WalletInfo>> {
        self.db.get_or_default(keys::wallet_infos())
    }

    pub fn create_wallet(
        &self,
        name: Option<String>,
        caption: Option<String>,
        password: &[u8],
        source: &types::SeedSource,
    ) -> Result<()> {
        // let master_key = gen_master_key(source, self.seed_password)?;
        // let id = gen_id(&master_key);
        // let salt = salt()?;
        // let iv = iv()?;
        // let key = key_from_password(password, salt);

        // let cf = db.create_cf(id);
        // let wallet_db = db.with_key(&key, iv, &self.params);
        // let mut wallet_batch = db.batch();

        // let wallet_info = model::WalletInfo { name, caption, id, iv, salt };
        // let wallet_default_account = gen_account(&master_key)?;
        // let wallet_accounts = vec![wallet_default_account.index];

        // batch.put_enc(&keys::account_ik(&id, account.index), &account.internal)?;
        // batch.put_enc(&keys::account_rk(&id, account.index), &account.rad)?;

        // db.write(batch)?;

        // let lock = self.wallets_mutex.lock()?;
        // let mut wallet_infos = db.get_or_default::<Vec<String>>(&keys::wallet_infos())?;
        // if !wallet_infos.contains(&wallet_info) {
        //     wallet_infos.push(wallet_info.clone());
        //     db.put(&keys::wallet_ids(), &ids)?;
        // }
        // drop(lock);

        // Ok(id)
        Ok(())
    }

    pub fn unlock_wallet(&self) -> Result<()> {
        Ok(())
    }
}

// fn gen_account(&self, master_key: &ExtendedSK, engine: &SignEngine) -> Result<types::Account> {
//     let account_index = 0;
//     let account_keypath = account_keypath(account_index);

//     let account_key = master_key.derive(engine, &account_keypath)?;

//     let external = {
//         let keypath = KeyPath::default().index(0);

//         account_key.derive(engine, &keypath)?
//     };
//     let internal = {
//         let keypath = KeyPath::default().index(1);

//         account_key.derive(engine, &keypath)?
//     };
//     let rad = {
//         let keypath = KeyPath::default().index(2);

//         account_key.derive(engine, &keypath)?
//     };
//     let balance = 0;

//     let account = Account {
//         index: account_index,
//         external,
//         internal,
//         rad,
//         balance,
//     };

//     Ok(account)
// }

// fn gen_id(master_key: &ExtendedSK) -> String {
//     match self.params.id_hash_function {
//         HashFunction::Sha256 => {
//             let password = master_key.concat();
//             let id_bytes = pbkdf2_sha256(
//                 password.as_ref(),
//                 self.params.master_key_salt.as_ref(),
//                 self.params.id_hash_iterations,
//             );

//             hex::encode(id_bytes)
//         }
//     }
// }

// pub fn salt(&self) -> Result<Vec<u8>> {
//     let salt = cipher::generate_random(self.params.db_salt_length)?;

//     Ok(salt)
// }

// pub fn iv(&self) -> Result<Vec<u8>> {
//     let iv = cipher::generate_random(self.params.db_iv_length)?;

//     Ok(iv)
// }

// pub fn key_from_password(&self, password: &[u8], salt: &[u8]) -> types::Secret {
//     pbkdf2_sha256(password, salt, self.params.db_hash_iterations)
// }
