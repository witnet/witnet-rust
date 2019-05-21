//! # Handlers for App actor
//!
//! Each handler corresponds to a json-rpc message exposed by the wallet server. Take a look at
//! `wallet/src/lib.rs`, the `routes!(...)` macro there matches each json-rpc method name to a
//! handler defined in this module.

mod create_data_request;
mod create_mnemonics;
mod create_wallet;
mod generate_address;
mod get_transactions;
mod get_wallet_infos;
mod import_seed;
mod lock_wallet;
mod run_rad_request;
mod send_data_request;
mod send_vtt;
mod unlock_wallet;

pub use create_data_request::CreateDataRequest;
pub use create_mnemonics::CreateMnemonics;
pub use create_wallet::CreateWallet;
pub use generate_address::GenerateAddress;
pub use get_transactions::GetTransactions;
pub use get_wallet_infos::GetWalletInfos;
pub use import_seed::ImportSeed;
pub use lock_wallet::LockWallet;
pub use run_rad_request::RunRadRequest;
pub use send_data_request::SendDataRequest;
pub use send_vtt::SendVtt;
pub use unlock_wallet::UnlockWallet;
