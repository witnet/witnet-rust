//! # Storage Manager
//!
//! This module provides a Storage Manager
use std::{
    any::{Any, TypeId},
    future,
    sync::Arc,
    time::Duration,
};

use actix::prelude::*;
use bincode::{deserialize, serialize};
use futures::Future;
use futures_util::FutureExt;

use crate::{
    config_mngr,
    utils::{stop_system_if_panicking, FlattenResult},
};
use witnet_config::{config, config::Config};
use witnet_data_structures::{
    chain::ChainState,
    utxo_pool::{CacheUtxosByPkh, UtxoDb, UtxoDbWrapStorage, UtxoWriteBatch},
};
use witnet_storage::{backends, storage::Storage};

pub use node_migrations::*;

mod node_migrations;

macro_rules! as_failure {
    ($e:expr) => {
        failure::Error::from_boxed_compat(Box::new($e))
    };
}

/// Start the storage manager
pub fn start() {
    let addr = StorageManagerAdapter::start_default();
    actix::SystemRegistry::set(addr);
}

/// Start the storage manager from config
pub fn start_from_config(config: Config) {
    let addr = StorageManagerAdapter::from_config(config).start();
    actix::SystemRegistry::set(addr);
}

/// Get value associated to key
pub fn get<K, T>(key: &K) -> impl Future<Output = Result<Option<T>, failure::Error>>
where
    K: serde::Serialize,
    T: serde::de::DeserializeOwned + 'static,
{
    // Check that we don't accidentally use this function with some certain special types
    if TypeId::of::<T>() == TypeId::of::<ChainState>() {
        panic!("Please use get_chain_state instead");
    }

    let addr = StorageManagerAdapter::from_registry();

    let key_bytes = match serialize(key) {
        Ok(x) => x,
        Err(e) => return futures::future::Either::Left(future::ready(Err(e.into()))),
    };

    let fut = async move {
        let opt = addr.send(Get(key_bytes)).await??;

        match opt {
            Some(bytes) => match deserialize(bytes.as_slice()) {
                Ok(v) => Ok(Some(v)),
                Err(e) => Err(as_failure!(e)),
            },
            None => Ok(None),
        }
    };

    futures::future::Either::Right(fut)
}

/// Put a value associated to the key into the storage
pub fn put<K, V>(key: &K, value: &V) -> impl Future<Output = Result<(), failure::Error>>
where
    K: serde::Serialize,
    V: serde::Serialize + Any,
{
    // Check that we don't accidentally use this function with some certain special types
    if value.type_id() == TypeId::of::<ChainState>() {
        panic!("Please use put_chain_state instead");
    }

    let addr = StorageManagerAdapter::from_registry();

    let key_bytes = match serialize(key) {
        Ok(x) => x,
        Err(e) => {
            return futures::future::Either::Left(futures::future::Either::Left(future::ready(
                Err(e.into()),
            )))
        }
    };

    let value_bytes = match serialize(value) {
        Ok(x) => x,
        Err(e) => {
            return futures::future::Either::Left(futures::future::Either::Right(future::ready(
                Err(e.into()),
            )))
        }
    };

    futures::future::Either::Right(async move { addr.send(Put(key_bytes, value_bytes)).await? })
}

/// Put a batch of values into the storage
pub fn put_batch<K, V>(kv: &[(K, V)]) -> impl Future<Output = Result<(), failure::Error>>
where
    K: serde::Serialize,
    V: serde::Serialize,
{
    let addr = StorageManagerAdapter::from_registry();

    let kv_bytes: Result<Vec<_>, failure::Error> = kv
        .iter()
        .map(|(k, v)| Ok((serialize(k)?, serialize(v)?)))
        .collect();

    async move {
        match kv_bytes {
            Ok(kv_bytes) if kv_bytes.is_empty() => Ok(()),
            Ok(kv_bytes) => addr.send(PutBatch(kv_bytes)).await?,
            Err(e) => Err(e),
        }
    }
}

/// Delete value associated to key
pub fn delete<K>(key: &K) -> impl Future<Output = Result<(), failure::Error>>
where
    K: serde::Serialize,
{
    let addr = StorageManagerAdapter::from_registry();

    let key_bytes = match serialize(key) {
        Ok(x) => x,
        Err(e) => return futures::future::Either::Left(future::ready(Err(e.into()))),
    };

    let fut = async move { addr.send(Delete(key_bytes)).await? };

    futures::future::Either::Right(fut)
}

/// Get an atomic reference to the storage backend
pub fn get_backend(
) -> impl Future<Output = Result<Arc<dyn NodeStorage + Send + Sync>, failure::Error>> {
    let addr = StorageManagerAdapter::from_registry();

    async move { addr.send(GetBackend).await? }
}

struct StorageManager {
    backend: Arc<dyn NodeStorage + Send + Sync>,
}

impl Drop for StorageManager {
    fn drop(&mut self) {
        log::trace!("Dropping StorageManager");
        stop_system_if_panicking("StorageManager");
        // FIXME(#2008): sometimes rocksdb is not closed correctly, resulting in error
        // pure virtual method called
        // This sleep seems to fix that error, but it doesn't look like a solid fix
        std::thread::sleep(Duration::from_millis(500));
    }
}

impl Default for StorageManager {
    fn default() -> Self {
        StorageManager {
            backend: Arc::new(UtxoDbWrapStorage(backends::nobackend::Backend)),
        }
    }
}

impl Actor for StorageManager {
    type Context = SyncContext<Self>;
}

struct Configure(Arc<config::Config>);

impl Message for Configure {
    type Result = Result<(), failure::Error>;
}

impl Handler<Configure> for StorageManager {
    type Result = <Configure as Message>::Result;

    fn handle(&mut self, Configure(conf): Configure, _ctx: &mut Self::Context) -> Self::Result {
        let storage_conf = &conf.storage;
        let backend = create_appropriate_backend(storage_conf)?;

        self.backend = backend;
        log::info!(
            "Configured {:#?} as the storage backend",
            storage_conf.backend
        );

        Ok(())
    }
}

struct Put(Vec<u8>, Vec<u8>);

impl Message for Put {
    type Result = Result<(), failure::Error>;
}

impl Handler<Put> for StorageManager {
    type Result = <Put as Message>::Result;

    fn handle(&mut self, Put(key, value): Put, _ctx: &mut Self::Context) -> Self::Result {
        self.backend.clone().as_arc_dyn_storage().put(key, value)
    }
}

struct PutBatch(Vec<(Vec<u8>, Vec<u8>)>);

impl Message for PutBatch {
    type Result = Result<(), failure::Error>;
}

impl Handler<PutBatch> for StorageManager {
    type Result = <PutBatch as Message>::Result;

    fn handle(&mut self, PutBatch(kvs): PutBatch, _ctx: &mut Self::Context) -> Self::Result {
        for (key, value) in kvs {
            self.backend.clone().as_arc_dyn_storage().put(key, value)?;
        }

        Ok(())
    }
}

struct Get(Vec<u8>);

impl Message for Get {
    type Result = Result<Option<Vec<u8>>, failure::Error>;
}

impl Handler<Get> for StorageManager {
    type Result = <Get as Message>::Result;

    fn handle(&mut self, Get(key): Get, _ctx: &mut Self::Context) -> Self::Result {
        self.backend.clone().as_arc_dyn_storage().get(key.as_ref())
    }
}

struct Delete(Vec<u8>);

impl Message for Delete {
    type Result = Result<(), failure::Error>;
}

impl Handler<Delete> for StorageManager {
    type Result = <Delete as Message>::Result;

    fn handle(&mut self, Delete(key): Delete, _ctx: &mut Self::Context) -> Self::Result {
        self.backend
            .clone()
            .as_arc_dyn_storage()
            .delete(key.as_ref())
    }
}

struct Batch(UtxoWriteBatch);

impl Message for Batch {
    type Result = Result<(), failure::Error>;
}

impl Handler<Batch> for StorageManager {
    type Result = <Batch as Message>::Result;

    fn handle(&mut self, msg: Batch, _ctx: &mut Self::Context) -> Self::Result {
        self.backend.clone().as_arc_dyn_utxo_db().write(msg.0)
    }
}

struct GetBackend;

impl Message for GetBackend {
    type Result = Result<Arc<dyn NodeStorage + Send + Sync>, failure::Error>;
}

impl Handler<GetBackend> for StorageManager {
    type Result = <GetBackend as Message>::Result;

    fn handle(&mut self, _msg: GetBackend, _ctx: &mut Self::Context) -> Self::Result {
        Ok(self.backend.clone())
    }
}

/// Helper trait to allow casting `Arc<dyn NodeStorage>` to `Arc<dyn Storage>` and `Arc<dyn UtxoDb>`.
#[allow(missing_docs)]
pub trait NodeStorage {
    fn as_arc_dyn_storage(self: Arc<Self>) -> Arc<dyn Storage + Send + Sync>;
    fn as_arc_dyn_utxo_db(self: Arc<Self>) -> Arc<dyn UtxoDb + Send + Sync>;
    fn as_arc_dyn_nodestorage(self: Arc<Self>) -> Arc<dyn NodeStorage + Send + Sync>;
}

impl<T> NodeStorage for T
where
    T: Storage + UtxoDb + Send + Sync + 'static,
{
    fn as_arc_dyn_storage(self: Arc<Self>) -> Arc<dyn Storage + Send + Sync> {
        self
    }
    fn as_arc_dyn_utxo_db(self: Arc<Self>) -> Arc<dyn UtxoDb + Send + Sync> {
        self
    }
    fn as_arc_dyn_nodestorage(self: Arc<Self>) -> Arc<dyn NodeStorage + Send + Sync> {
        self
    }
}

/// Create storage backend according to provided config. Wraps a `Storage` implementation to make it
/// implement `NodeStorage`.
fn wrap_storage_as_nodestorage<S: Storage + Send + Sync + 'static>(
    db: S,
    conf: &config::Storage,
) -> Result<Arc<dyn NodeStorage + Send + Sync>, failure::Error> {
    // Log progress of the initialization performed in `CacheUtxosByPkh::new`. Unfortunately we don't
    // know the total number of UTXOs so it is not possible to display a percentage.
    let mut total_utxos = 0;
    let log_progress_cache_utxos_by_pkh = |i: usize| {
        if i > 0 && i % 100_000 == 0 {
            log::debug!("Initializing UTXO cache: {} UTXOs processed", i);
        }

        total_utxos = i;
    };

    if conf.utxos_in_memory {
        log::debug!("Initializing UTXO cache. This may take a few seconds");
        let cache_db = CacheUtxosByPkh::new_with_progress(
            UtxoDbWrapStorage(db),
            log_progress_cache_utxos_by_pkh,
        )?;
        log::info!("Initialized UTXO cache.  {} UTXOs processed", total_utxos);
        Ok(Arc::new(cache_db))
    } else {
        Ok(Arc::new(UtxoDbWrapStorage(db)))
    }
}

/// Create storage backend according to provided config
pub fn create_appropriate_backend(
    conf: &config::Storage,
) -> Result<Arc<dyn NodeStorage + Send + Sync>, failure::Error> {
    match conf.backend {
        config::StorageBackend::HashMap => {
            let db = backends::hashmap::Backend::default();

            wrap_storage_as_nodestorage(db, conf)
        }
        config::StorageBackend::RocksDB => {
            let path = conf.db_path.as_path();
            let mut options = backends::rocksdb::Options::default();
            options.create_if_missing(true);
            options.set_max_open_files(conf.max_open_files);
            let db =
                backends::rocksdb::Backend::open(&options, path).map_err(|e| as_failure!(e))?;

            wrap_storage_as_nodestorage(db, conf)
        }
    }
}

struct StorageManagerAdapter {
    storage: Addr<StorageManager>,
    config: Option<Config>,
}

impl Default for StorageManagerAdapter {
    fn default() -> Self {
        let storage = SyncArbiter::start(1, StorageManager::default);
        Self {
            storage,
            config: None,
        }
    }
}

impl StorageManagerAdapter {
    pub fn from_config(config: Config) -> Self {
        let storage = SyncArbiter::start(1, StorageManager::default);
        Self {
            storage,
            config: Some(config),
        }
    }
}

impl Actor for StorageManagerAdapter {
    type Context = Context<Self>;

    fn started(&mut self, ctx: &mut Self::Context) {
        log::debug!("Storage Manager actor has been started!");
        let storage = self.storage.clone();
        let config = self.config.clone();

        async move {
            if let Some(config) = config {
                storage.send(Configure(Arc::new(config))).await?
            } else {
                let conf = config_mngr::get().await?;
                storage.send(Configure(conf)).await?
            }
        }
        .into_actor(self)
        .map_err(|err, _act, _ctx| {
            log::error!("Failed to configure backend: {}", err);
            System::current().stop_with_code(1);
        })
        .map(|_res: Result<(), ()>, _act, _ctx| ())
        .wait(ctx);
    }
}

impl Supervised for StorageManagerAdapter {}

impl SystemService for StorageManagerAdapter {}

// Delegate all the StorageManager messages to the inner StorageManager
impl<M> Handler<M> for StorageManagerAdapter
where
    M: Message + Send + 'static,
    <M as actix::Message>::Result: Send,
    Result<<M as actix::Message>::Result, actix::MailboxError>:
        FlattenResult<OutputResult = <M as actix::Message>::Result>,
    StorageManager: Handler<M>,
{
    type Result = ResponseFuture<<M as Message>::Result>;

    fn handle(&mut self, msg: M, _ctx: &mut Self::Context) -> Self::Result {
        Box::pin(self.storage.send(msg).map(FlattenResult::flatten_result))
    }
}
