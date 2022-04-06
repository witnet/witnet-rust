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
use witnet_data_structures::chain::ChainState;
use witnet_storage::{
    backends,
    storage::{self, Storage, WriteBatch},
};

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
pub fn get_backend() -> impl Future<Output = Result<Arc<dyn Storage + Send + Sync>, failure::Error>>
{
    let addr = StorageManagerAdapter::from_registry();

    async move { addr.send(GetBackend).await? }
}

struct StorageManager {
    backend: Arc<dyn Storage + Send + Sync>,
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
            backend: Arc::new(backends::nobackend::Backend),
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
        self.backend.put(key, value)
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
            self.backend.put(key, value)?;
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
        self.backend.get(key.as_ref())
    }
}

struct Delete(Vec<u8>);

impl Message for Delete {
    type Result = Result<(), failure::Error>;
}

impl Handler<Delete> for StorageManager {
    type Result = <Delete as Message>::Result;

    fn handle(&mut self, Delete(key): Delete, _ctx: &mut Self::Context) -> Self::Result {
        self.backend.delete(key.as_ref())
    }
}

struct Batch(WriteBatch);

impl Message for Batch {
    type Result = Result<(), failure::Error>;
}

impl Handler<Batch> for StorageManager {
    type Result = <Batch as Message>::Result;

    fn handle(&mut self, msg: Batch, _ctx: &mut Self::Context) -> Self::Result {
        self.backend.write(msg.0)
    }
}

struct GetBackend;

impl Message for GetBackend {
    type Result = Result<Arc<dyn Storage + Send + Sync>, failure::Error>;
}

impl Handler<GetBackend> for StorageManager {
    type Result = <GetBackend as Message>::Result;

    fn handle(&mut self, _msg: GetBackend, _ctx: &mut Self::Context) -> Self::Result {
        Ok(self.backend.clone())
    }
}

/// Create storage backend according to provided config
pub fn create_appropriate_backend(
    conf: &config::Storage,
) -> Result<Arc<dyn storage::Storage + Send + Sync>, failure::Error> {
    match conf.backend {
        config::StorageBackend::HashMap => Ok(Arc::new(backends::hashmap::Backend::default())),
        config::StorageBackend::RocksDB => {
            let path = conf.db_path.as_path();

            backends::rocksdb::Backend::open_default(path)
                .map(|backend| -> Arc<dyn storage::Storage + Send + Sync> { Arc::new(backend) })
                .map_err(|e| as_failure!(e))
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
