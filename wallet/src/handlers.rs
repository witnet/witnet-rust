//! Wallet request handlers.

use futures::future;
use jsonrpc_core::Params;

use super::response::Response;
use super::wallet::*;

/// TODO: doc
pub fn get_wallet_infos(_params: Params) -> impl Response<Vec<WalletInfo>> {
    future::ok(vec![])
}

// TODO: doc
pub fn create_mnemonics(_params: Params) -> impl Response<Mnemonics> {
    future::ok(Mnemonics {})
}

// TODO: doc
pub fn import_seed(_params: Params) -> impl Response<bool> {
    future::ok(true)
}

// TODO: doc
pub fn create_wallet(_params: Params) -> impl Response<Wallet> {
    future::ok(Wallet {
        version: 0,
        info: WalletInfo {
            id: "".to_string(),
            caption: "".to_string(),
        },
        seed: SeedInfo::Wip3(Seed(vec![])),
        epochs: EpochsInfo { last: 0, born: 0 },
        purpose: DerivationPath("m/44'/60'/0'/0".to_string()),
        accounts: vec![],
    })
}

// TODO: doc
pub fn unlock_wallet(_params: Params) -> impl Response<Wallet> {
    future::ok(Wallet {
        version: 0,
        info: WalletInfo {
            id: "".to_string(),
            caption: "".to_string(),
        },
        seed: SeedInfo::Wip3(Seed(vec![])),
        epochs: EpochsInfo { last: 0, born: 0 },
        purpose: DerivationPath("m/44'/60'/0'/0".to_string()),
        accounts: vec![],
    })
}

// TODO: doc
pub fn lock_wallet(_params: Params) -> impl Response<bool> {
    future::ok(true)
}

// TODO: doc
pub fn send_data_request(_params: Params) -> impl Response<Transaction> {
    future::ok(Transaction {})
}

// TODO: doc
pub fn run_data_request(_params: Params) -> impl Response<RadonValue> {
    future::ok(RadonValue {})
}

// TODO: doc
pub fn create_data_request(_params: Params) -> impl Response<DataRequest> {
    future::ok(DataRequest {})
}

// TODO: doc
pub fn generate_address(_params: Params) -> impl Response<Address> {
    future::ok(Address {})
}

// TODO: doc
pub fn send_vtt(_params: Params) -> impl Response<Transaction> {
    future::ok(Transaction {})
}

// TODO: doc
pub fn get_transactions(_params: Params) -> impl Response<Vec<Transaction>> {
    future::ok(vec![])
}
