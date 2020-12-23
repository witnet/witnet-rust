use std::result;
use std::sync::Arc;

use actix::prelude::*;

use crate::{db, params, repository, types};

pub mod error;
pub mod handlers;
pub mod methods;

pub use error::*;
pub use handlers::*;
use witnet_crypto::key::CryptoEngine;

pub type Result<T> = result::Result<T, Error>;

pub struct Worker {
    db: Arc<rocksdb::DB>,
    wallets: Arc<repository::Wallets<db::PlainDb>>,
    node: params::NodeParams,
    params: params::Params,
    engine: CryptoEngine,
    rng: rand::rngs::OsRng,
}

impl Actor for Worker {
    type Context = SyncContext<Self>;
}
