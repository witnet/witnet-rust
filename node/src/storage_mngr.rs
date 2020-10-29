//! # Storage Manager
//!
//! This module provides a Storage Manager
use std::sync::Arc;

use actix::prelude::*;
use bincode::{deserialize, serialize};
use futures::future::{Either, Future};

use crate::config_mngr;
use witnet_config::config;
use witnet_storage::{backends, storage, storage::Storage};

macro_rules! as_failure {
    ($e:expr) => {
        failure::Error::from_boxed_compat(Box::new($e))
    };
}

/// Start the signature manager
pub fn start() {
    let addr = StorageManagerAdapter::start_default();
    actix::SystemRegistry::set(addr);
}

/// Get value associated to key
pub fn get<K, T>(key: &K) -> impl Future<Item = Option<T>, Error = failure::Error>
where
    K: serde::Serialize,
    T: serde::de::DeserializeOwned,
{
    let addr = StorageManagerAdapter::from_registry();

    futures::future::result(serialize(key))
        .map_err(|e| as_failure!(e))
        .and_then(move |key_bytes| addr.send(Get(key_bytes)).flatten())
        .and_then(|opt| match opt {
            Some(bytes) => match deserialize(bytes.as_slice()) {
                Ok(v) => futures::future::ok(Some(v)),
                Err(e) => futures::future::err(as_failure!(e)),
            },
            None => futures::future::ok(None),
        })
}

/// Put a value associated to the key into the storage
pub fn put<K, V>(key: &K, value: &V) -> impl Future<Item = (), Error = failure::Error>
where
    K: serde::Serialize,
    V: serde::Serialize,
{
    let addr = StorageManagerAdapter::from_registry();

    futures::future::result(serialize(key))
        .join(futures::future::result(serialize(value)))
        .map_err(|e| as_failure!(e))
        .and_then(move |(key_bytes, value_bytes)| addr.send(Put(key_bytes, value_bytes)).flatten())
}

/// Put a batch of values into the storage
pub fn put_batch<K, V>(kv: &[(K, V)]) -> impl Future<Item = (), Error = failure::Error>
where
    K: serde::Serialize,
    V: serde::Serialize,
{
    let addr = StorageManagerAdapter::from_registry();

    let kv_bytes: Result<Vec<_>, failure::Error> = kv
        .iter()
        .map(|(k, v)| Ok((serialize(k)?, serialize(v)?)))
        .collect();

    match kv_bytes {
        Ok(kv_bytes) if kv_bytes.is_empty() => Either::B(futures::future::finished(())),
        Ok(kv_bytes) => Either::A(addr.send(PutBatch(kv_bytes)).flatten()),
        Err(e) => Either::B(futures::future::failed(e)),
    }
}

/// Delete value associated to key
pub fn delete<K>(key: &K) -> impl Future<Item = (), Error = failure::Error>
where
    K: serde::Serialize,
{
    let addr = StorageManagerAdapter::from_registry();

    futures::future::result(serialize(key))
        .map_err(|e| as_failure!(e))
        .and_then(move |key_bytes| addr.send(Delete(key_bytes)).flatten())
}

/// Get an atomic reference to the storage backend
pub fn get_backend() -> impl Future<Item = Arc<dyn Storage + Send + Sync>, Error = failure::Error> {
    let addr = StorageManagerAdapter::from_registry();

    addr.send(GetBackend).flatten()
}

struct StorageManager {
    backend: Arc<dyn Storage + Send + Sync>,
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
        if storage_conf.password.is_some() {
            log::info!("Storage backend is using encryption");
        }

        migrations::migrate(&*self.backend)?;

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

macro_rules! encrypted_backend {
    ($backend:expr, $password_opt:expr) => {
        if let Some(password) = $password_opt {
            Arc::new(backends::crypto::Backend::new(password, $backend))
                as Arc<dyn storage::Storage + Send + Sync>
        } else {
            Arc::new($backend) as Arc<dyn storage::Storage + Send + Sync>
        }
    };
}

/// Create storage backend according to provided config
pub fn create_appropriate_backend(
    conf: &config::Storage,
) -> Result<Arc<dyn storage::Storage + Send + Sync>, failure::Error> {
    let passwd = conf.password.clone();

    match conf.backend {
        config::StorageBackend::HashMap => Ok(encrypted_backend!(
            backends::hashmap::Backend::default(),
            passwd
        )),
        config::StorageBackend::RocksDB => {
            let path = conf.db_path.as_path();

            backends::rocksdb::Backend::open_default(path)
                .map(|backend| encrypted_backend!(backend, passwd))
                .map_err(|e| as_failure!(e))
        }
    }
}

struct StorageManagerAdapter {
    storage: Addr<StorageManager>,
}

impl Default for StorageManagerAdapter {
    fn default() -> Self {
        let storage = SyncArbiter::start(1, StorageManager::default);
        Self { storage }
    }
}

impl Actor for StorageManagerAdapter {
    type Context = Context<Self>;

    fn started(&mut self, ctx: &mut Self::Context) {
        log::debug!("Storage Manager actor has been started!");
        let storage = self.storage.clone();

        config_mngr::get()
            .and_then(move |conf| storage.send(Configure(conf)).flatten())
            .map_err(|err| {
                log::error!("Failed to configure backend: {}", err);
                System::current().stop_with_code(1);
            })
            .into_actor(self)
            .wait(ctx);
    }
}

impl Supervised for StorageManagerAdapter {}

impl SystemService for StorageManagerAdapter {}

impl Handler<Get> for StorageManagerAdapter {
    type Result = ResponseFuture<Option<Vec<u8>>, failure::Error>;

    fn handle(&mut self, msg: Get, _ctx: &mut Self::Context) -> Self::Result {
        Box::new(self.storage.send(msg).flatten())
    }
}

impl Handler<Put> for StorageManagerAdapter {
    type Result = ResponseFuture<(), failure::Error>;

    fn handle(&mut self, msg: Put, _ctx: &mut Self::Context) -> Self::Result {
        Box::new(self.storage.send(msg).flatten())
    }
}

impl Handler<PutBatch> for StorageManagerAdapter {
    type Result = ResponseFuture<(), failure::Error>;

    fn handle(&mut self, msg: PutBatch, _ctx: &mut Self::Context) -> Self::Result {
        Box::new(self.storage.send(msg).flatten())
    }
}

impl Handler<Delete> for StorageManagerAdapter {
    type Result = ResponseFuture<(), failure::Error>;

    fn handle(&mut self, msg: Delete, _ctx: &mut Self::Context) -> Self::Result {
        Box::new(self.storage.send(msg).flatten())
    }
}

impl Handler<GetBackend> for StorageManagerAdapter {
    type Result = ResponseFuture<Arc<dyn Storage + Send + Sync>, failure::Error>;

    fn handle(&mut self, msg: GetBackend, _ctx: &mut Self::Context) -> Self::Result {
        Box::new(self.storage.send(msg).flatten())
    }
}

mod migrations {
    use super::*;

    pub fn migrate(db: &(dyn Storage + Send + Sync)) -> Result<(), failure::Error> {
        // Check if the db is empty
        // Migrations are only needed if the database is non-empty
        if db.prefix_iterator(b"")?.next().is_none() {
            update_version(db, 1)
        } else {
            loop {
                let version = detect_version(db)?;

                match version {
                    0 => migrate_v0(db)?,
                    1 => return Ok(()),
                    _ => Err(failure::err_msg(format!("Invalid db version {}", version)))?,
                }
            }
        }
    }

    pub fn detect_version(db: &(dyn Storage + Send + Sync)) -> Result<u32, failure::Error> {
        let version_key = b"WITNET-DB-VERSION";
        let version_bytes = db.get(version_key)?;
        if version_bytes.is_none() {
            // No version key.
            // This can mean version 0, or empty database
            // We assume that the database is not empty at this point, so this is version 0
            return Ok(0);
        }

        let version: u32 = bincode::deserialize(&version_bytes.unwrap()).unwrap();
        Ok(version)
    }

    pub fn update_version(db: &(dyn Storage + Send + Sync), version: u32) -> Result<(), failure::Error> {
        let version_key = b"WITNET-DB-VERSION";
        let version_bytes = bincode::serialize(&version).unwrap();
        db.put(version_key.to_vec(), version_bytes)?;

        Ok(())
    }

    /// The only change from v0 to v1 is in ChainState: in v0 the utxo set is stored in ChainState,
    /// but in v1 the utxo set is independent, and each utxo is stored under its own key.
    pub fn migrate_v0(db: &(dyn Storage + Send + Sync)) -> Result<(), failure::Error> {
        //let old_chain_state: v0::ChainState = bincode::deserialize(&old_chain_state_bytes).unwrap();
        unimplemented!()
    }
}
