use std::path;

use super::*;

#[derive(Clone)]
pub struct State {
    /// Wallets Info database which contains public info of available
    /// wallets.
    pub db: db::Database,
    /// Path where wallet databases are stored.
    pub db_path: path::PathBuf,
    /// Configuration params used when creating a new wallet.
    pub wallets_config: types::WalletsConfig,
    pub sign_engine: types::SignEngine,
}
