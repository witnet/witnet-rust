//! # Storage Manager
//!
//! This module provides a Storage Manager
use std::sync::Arc;

use actix::prelude::*;
use bincode::{deserialize, serialize};
use futures::future::Future;
use log;
use serde;

use crate::config_mngr;
use witnet_config::config;
use witnet_storage::{backends, storage};

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

struct StorageManager {
    backend: Box<dyn storage::Storage>,
}

impl Default for StorageManager {
    fn default() -> Self {
        StorageManager {
            backend: Box::new(backends::nobackend::Backend),
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

macro_rules! encrypted_backend {
    ($backend:expr, $password_opt:expr) => {
        if let Some(password) = $password_opt {
            Box::new(backends::crypto::Backend::new(password, $backend))
                as Box<dyn storage::Storage>
        } else {
            Box::new($backend) as Box<dyn storage::Storage>
        }
    };
}

fn create_appropriate_backend(
    conf: &config::Storage,
) -> Result<Box<dyn storage::Storage>, failure::Error> {
    let passwd = conf.password.clone();

    match conf.backend {
        config::StorageBackend::HashMap => Ok(encrypted_backend!(
            backends::hashmap::Backend::new(),
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

impl Handler<Delete> for StorageManagerAdapter {
    type Result = ResponseFuture<(), failure::Error>;

    fn handle(&mut self, msg: Delete, _ctx: &mut Self::Context) -> Self::Result {
        Box::new(self.storage.send(msg).flatten())
    }
}
