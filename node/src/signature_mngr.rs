//! # Signature Manager
//!
//! This module provides a Signature Manager, which, after being
//! initialized with a key, can be used repeatedly to sign data with
//! that key.
use actix::prelude::*;
use failure;
use failure::bail;
use futures::future::Future;
use log;

use crate::{actors::storage_keys::MASTER_KEY, storage_mngr};

use witnet_crypto::{
    key::{ExtendedSK, MasterKeyGen, SK},
    mnemonic::MnemonicGen,
    signature,
};

use witnet_data_structures::chain::{ExtendedSecretKey, Hash, Hashable};

/// Start the signature manager
pub fn start() {
    let addr = SignatureManager::start_default();
    actix::System::current().registry().set(addr);
}

/// Set the key used to sign
pub fn set_key(key: SK) -> impl Future<Item = (), Error = failure::Error> {
    let addr = actix::System::current()
        .registry()
        .get::<SignatureManager>();
    addr.send(SetKey(key)).flatten()
}

/// Sign a piece of data with the stored key.
///
/// This might fail if the manager has not been initialized with a key
pub fn sign<T>(data: &T) -> impl Future<Item = signature::Signature, Error = failure::Error>
where
    T: Hashable,
{
    let addr = actix::System::current()
        .registry()
        .get::<SignatureManager>();
    let Hash::SHA256(data_hash) = data.hash();

    addr.send(Sign(data_hash.to_vec())).flatten()
}

#[derive(Debug, Default)]
struct SignatureManager {
    key: Option<SK>,
}

struct SetKey(SK);
struct Sign(Vec<u8>);

fn persist_master_key(master_key: ExtendedSK) -> impl Future<Item = (), Error = failure::Error> {
    let master_key = ExtendedSecretKey::from(master_key);

    storage_mngr::put(&MASTER_KEY, &master_key).inspect(|_| {
        log::debug!("Successfully persisted the extended secret key into storage");
    })
}

fn create_master_key() -> Box<dyn Future<Item = (), Error = failure::Error>> {
    log::info!("Generating and persisting a new master key for this node");

    // Create a new master key
    let mnemonic = MnemonicGen::new().generate();
    let seed = mnemonic.seed("");
    match MasterKeyGen::new(seed).generate() {
        Ok(master_key) => {
            let fut = set_key(master_key.secret_key)
                .join(persist_master_key(master_key))
                .map(|_| ());

            Box::new(fut)
        }
        Err(e) => {
            let fut = futures::future::err(e.into());

            Box::new(fut)
        }
    }
}

impl Actor for SignatureManager {
    type Context = Context<Self>;

    fn started(&mut self, ctx: &mut Self::Context) {
        log::debug!("Signature Manager actor has been started!");

        storage_mngr::get::<_, ExtendedSecretKey>(&MASTER_KEY)
            .and_then(move |master_key_from_storage| {
                master_key_from_storage.map_or_else(create_master_key, |master_key| {
                    let master_key: ExtendedSK = master_key.into();
                    let fut = set_key(master_key.secret_key);

                    Box::new(fut)
                })
            })
            .map_err(|e| log::error!("Couldn't initialize Signature Manager: {}", e))
            .into_actor(self)
            .wait(ctx);
    }
}

impl Supervised for SignatureManager {}

impl SystemService for SignatureManager {}

impl Message for SetKey {
    type Result = Result<(), failure::Error>;
}

impl Message for Sign {
    type Result = Result<signature::Signature, failure::Error>;
}

impl Handler<SetKey> for SignatureManager {
    type Result = <SetKey as Message>::Result;

    fn handle(&mut self, SetKey(key): SetKey, _ctx: &mut Self::Context) -> Self::Result {
        self.key = Some(key);
        Ok(())
    }
}

impl Handler<Sign> for SignatureManager {
    type Result = <Sign as Message>::Result;

    fn handle(&mut self, Sign(data): Sign, _ctx: &mut Self::Context) -> Self::Result {
        match self.key {
            Some(key) => Ok(signature::sign(key, &data)),
            None => bail!("Signature Manager cannot sign because it contains no key"),
        }
    }
}
