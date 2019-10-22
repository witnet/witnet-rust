use std::fs;

use actix::prelude::*;

use witnet_config::config::Config as WitnetConfig;
use witnet_net::{client::tcp::JsonRpcClient, server::ws::Server};

use crate::*;

mod api;
mod dispatch;
mod executor;
mod handlers;
mod requests;
mod responses;
mod routes;
mod state;
mod validation;

/// Run the Witnet Wallet application server.
pub fn run(conf: WitnetConfig) -> Result<(), failure::Error> {
    let concurrency = conf.wallet.concurrency.unwrap_or_else(num_cpus::get);
    let server_addr = conf.wallet.server_addr;
    let db_path = conf.wallet.db_path;
    let wallets_config = types::WalletsConfig {
        seed_password: conf.wallet.seed_password,
        master_key_salt: conf.wallet.master_key_salt,
    };

    // create database directory if it doesn't exist
    fs::create_dir_all(&db_path)?;

    let db_url = db_path
        .join("wallets.sqlite3")
        .to_str()
        .map(|s| s.to_string())
        .ok_or_else(|| failure::format_err!("db path contains unsupported characters"))?;
    let db = db::Database::open(&db_url)?;
    wallets::migrate_db(&db.get()?)?;

    let system = System::new("witnet-wallet-server");

    let api = api::Api::new(concurrency, db, db_path, wallets_config);
    let server = Server::build()
        .handler(routes::handler(api))
        .addr(server_addr)
        .start()?;

    system.run()?;

    Ok(())
}
