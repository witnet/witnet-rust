use std::path::PathBuf;

use actix::prelude::*;
use failure::Error;

use witnet_net::client::tcp::JsonRpcClient;

use crate::actors::{storage, Crypto, RadExecutor, Storage};

#[derive(Default)]
pub struct AppBuilder {
    node_url: Option<String>,
    db_path: PathBuf,
}

impl AppBuilder {
    pub fn node_url(mut self, url: Option<String>) -> Self {
        self.node_url = url;
        self
    }

    pub fn db_path(mut self, path: PathBuf) -> Self {
        self.db_path = path;
        self
    }

    /// Start App actor with given addresses for Storage and Rad actors.
    pub fn start(self) -> Result<Addr<super::App>, Error> {
        let node_url = self.node_url;
        let node_client = node_url.clone().map_or_else(
            || Ok(None),
            |url| JsonRpcClient::start(url.as_ref()).map(Some),
        )?;
        let storage = Storage::build()
            .with_path(self.db_path)
            .with_file_name("witnet_wallets.db")
            .with_options({
                let mut db_opts = storage::Options::default();
                db_opts.create_if_missing(true);
                db_opts
            })
            .start()?;
        let crypto = Crypto::build().start();
        let rad_executor = RadExecutor::start();

        let app = super::App {
            storage,
            rad_executor,
            node_client,
            crypto,
            subscriptions: Default::default(),
        };

        Ok(app.start())
    }
}
