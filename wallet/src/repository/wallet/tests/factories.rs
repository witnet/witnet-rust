use std::{cell::RefCell, rc::Rc};

use witnet_data_structures::chain::Hash;

use super::*;

pub fn wallet(data: Option<HashMap<Vec<u8>, Vec<u8>>>) -> (Wallet<db::HashMapDb>, db::HashMapDb) {
    let id = "example-wallet";
    let params = params::Params::default();
    let mnemonic = types::MnemonicGen::new()
        .with_len(types::MnemonicLength::Words12)
        .generate();
    let source = types::SeedSource::Mnemonics(mnemonic);
    let master_key = crypto::gen_master_key(
        params.seed_password.as_ref(),
        params.master_key_salt.as_ref(),
        &source,
    )
    .unwrap();
    let engine = types::CryptoEngine::new();
    let default_account_index = 0;
    let default_account =
        account::gen_account(&engine, default_account_index, &master_key).unwrap();

    let mut rng = rand::rngs::OsRng;
    let salt = crypto::salt(&mut rng, params.db_salt_length);
    let iv = crypto::salt(&mut rng, params.db_iv_length);

    let storage = Rc::new(RefCell::new(data.unwrap_or_default()));
    let db = db::HashMapDb::new(storage);
    let wallets = Wallets::new(db.clone());

    // Create the initial data required by the wallet
    wallets
        .create(
            &db,
            types::CreateWalletData {
                iv,
                salt,
                id: "test-wallet-id",
                name: None,
                caption: None,
                account: &default_account,
            },
        )
        .unwrap();

    let wallet = Wallet::unlock(id, db.clone(), params, engine).unwrap();

    (wallet, db)
}

pub fn pkh() -> types::PublicKeyHash {
    let bytes: [u8; 20] = rand::random();
    types::PublicKeyHash::from_bytes(&bytes).expect("PKH of 20 bytes failed")
}

pub fn transaction_id() -> types::TransactionId {
    let bytes: [u8; 32] = rand::random();

    types::TransactionId::SHA256(bytes)
}

pub fn vtt_from_body(body: types::VTTransactionBody) -> types::Transaction {
    types::Transaction::ValueTransfer(VTTransaction {
        body,
        signatures: vec![],
    })
}

#[derive(Default)]
pub struct Input {
    transaction_id: Option<types::TransactionId>,
    output_index: Option<u32>,
}

impl Input {
    pub fn with_transaction(mut self, transaction_id: types::TransactionId) -> Self {
        self.transaction_id = Some(transaction_id);
        self
    }

    pub fn with_output_index(mut self, output_index: u32) -> Self {
        self.output_index = Some(output_index);
        self
    }

    pub fn create(self) -> types::TransactionInput {
        let transaction_id = self.transaction_id.unwrap_or_else(transaction_id);
        let output_index = self.output_index.unwrap_or_else(rand::random);

        types::TransactionInput::new(types::OutputPointer {
            transaction_id,
            output_index,
        })
    }
}

#[derive(Default)]
pub struct VttOutput {
    pkh: Option<types::PublicKeyHash>,
    value: Option<u64>,
}

impl VttOutput {
    pub fn with_pkh(mut self, pkh: types::PublicKeyHash) -> Self {
        self.pkh = Some(pkh);
        self
    }

    pub fn with_value(mut self, value: u64) -> Self {
        self.value = Some(value);
        self
    }

    pub fn create(self) -> types::VttOutput {
        let pkh = self.pkh.unwrap_or_else(pkh);
        let value = self.value.unwrap_or_else(rand::random);
        let time_lock = rand::random();

        types::VttOutput {
            pkh,
            value,
            time_lock,
        }
    }
}

#[derive(Default)]
pub struct BlockInfo {
    hash: Option<Vec<u8>>,
    epoch: Option<u32>,
}

impl BlockInfo {
    pub fn create(self) -> model::BlockInfo {
        let hash = Hash::from(
            self.hash
                .unwrap_or_else(|| transaction_id().as_ref().to_vec()),
        );
        let epoch = self.epoch.unwrap_or_else(rand::random);

        model::BlockInfo { hash, epoch }
    }
}
