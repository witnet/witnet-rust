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
    key::{ExtendedSK, MasterKeyGen, SignContext, PK, SK},
    mnemonic::MnemonicGen,
    signature,
};
use witnet_data_structures::chain::{
    ExtendedSecretKey, Hash, Hashable, KeyedSignature, PublicKey, Signature,
};

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
pub fn sign<T>(data: &T) -> impl Future<Item = KeyedSignature, Error = failure::Error>
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
    keypair: Option<(SK, PK)>,
}

impl SignatureManager {
    fn set_key(&mut self, key: SK) {
        let public_key = PK::from_secret_key(&SignContext::signing_only(), &key);
        self.keypair = Some((key, public_key));
    }
}

struct SetKey(SK);
struct Sign(Vec<u8>);

fn persist_master_key(master_key: ExtendedSK) -> impl Future<Item = (), Error = failure::Error> {
    let master_key = ExtendedSecretKey::from(master_key);

    storage_mngr::put(&MASTER_KEY, &master_key).inspect(|_| {
        log::debug!("Successfully persisted the extended secret key into storage");
    })
}

fn create_master_key() -> Box<dyn Future<Item = SK, Error = failure::Error>> {
    log::info!("Generating and persisting a new master key for this node");

    // Create a new master key
    let mnemonic = MnemonicGen::new().generate();
    let seed = mnemonic.seed("");
    match MasterKeyGen::new(seed).generate() {
        Ok(master_key) => {
            let fut = persist_master_key(master_key.clone()).map(move |_| master_key.secret_key);

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
                    let fut = futures::future::ok(master_key.secret_key);

                    Box::new(fut)
                })
            })
            .map_err(|e| log::error!("Couldn't initialize Signature Manager: {}", e))
            .into_actor(self)
            .map(|secret_key, _, ctx| {
                ctx.notify(SetKey(secret_key));
            })
            .wait(ctx);
    }
}

impl Supervised for SignatureManager {}

impl SystemService for SignatureManager {}

impl Message for SetKey {
    type Result = Result<(), failure::Error>;
}

impl Message for Sign {
    type Result = Result<KeyedSignature, failure::Error>;
}

impl Handler<SetKey> for SignatureManager {
    type Result = <SetKey as Message>::Result;

    fn handle(&mut self, SetKey(secret_key): SetKey, _ctx: &mut Self::Context) -> Self::Result {
        self.set_key(secret_key);

        log::info!("Signature Manager received a key and is ready to sign");

        Ok(())
    }
}

impl Handler<Sign> for SignatureManager {
    type Result = <Sign as Message>::Result;

    fn handle(&mut self, Sign(data): Sign, _ctx: &mut Self::Context) -> Self::Result {
        match self.keypair {
            Some((secret, public)) => {
                let signature = signature::sign(secret, &data);
                let keyed_signature = KeyedSignature {
                    signature: Signature::from(signature),
                    public_key: PublicKey::from(public),
                };

                Ok(keyed_signature)
            }
            None => bail!("Signature Manager cannot sign because it contains no key"),
        }
    }
}
