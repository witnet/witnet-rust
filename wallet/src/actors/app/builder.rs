use std::{path::PathBuf, sync::Arc};

use actix::prelude::*;
use failure::Error;

use witnet_net::client::tcp::JsonRpcClient;

use crate::{
    actors::{Crypto, RadExecutor, Storage},
    storage,
};

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

        let mut db_opts = rocksdb::Options::default();
        db_opts.create_if_missing(true);
        db_opts.set_merge_operator("merge operator", storage::storage_merge_operator, None);
        // From rocksdb docs: every store to stable storage will issue a fsync. This parameter
        // should be set to true while storing data to filesystem like ext3 that can lose files
        // after a reboot.
        db_opts.set_use_fsync(true);

        let db = Arc::new(
            rocksdb::DB::open(&db_opts, self.db_path.join("witnet_wallet.db"))
                .map_err(storage::Error::OpenDbFailed)?,
        );
        let storage = Storage::build().start()?;
        let crypto = Crypto::build().start();
        let rad_executor = RadExecutor::start();

        let app = super::App::new(db, storage, rad_executor, crypto, node_client);

        Ok(app.start())
    }
}
