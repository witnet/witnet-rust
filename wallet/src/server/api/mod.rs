use std::fmt::Debug;

use actix::prelude::*;
use futures::{Future, IntoFuture};
use jsonrpc_core as rpc;

use super::*;

pub mod error;
pub use error::ApiError;

pub type Result<T> = std::result::Result<T, ApiError>;

#[derive(Clone)]
pub struct Api {
    executor: Addr<executor::Executor>,
}

impl Api {
    pub fn new(concurrency: usize, wallets_db: db::Database) -> Self {
        let executor = SyncArbiter::start(concurrency, move || {
            executor::Executor::new(wallets_db.clone())
        });

        Self { executor }
    }

    pub fn dispatch<R, T>(&self, request: R) -> impl Future<Item = rpc::Value, Error = rpc::Error>
    where
        R: Debug,
        R: Message + Send + 'static,
        T: serde::Serialize + Send + 'static,
        <R as Message>::Result: Send + IntoFuture<Item = T, Error = ApiError>,
        executor::Executor: Handler<R>,
    {
        log::trace!("=> Handling Request: {:?}", &request);

        self.executor
            .send(request)
            .map_err(error::internal)
            .flatten()
            .and_then(|ret| serde_json::to_value(ret).map_err(error::internal))
            .map_err(rpc::Error::from)
    }
}
