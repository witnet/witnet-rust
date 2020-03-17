//! # Signature Manager
//!
//! This module provides a Signature Manager, which, after being
//! initialized with a key, can be used repeatedly to sign data with
//! that key.
use actix::prelude::*;
use failure::{bail, format_err};
use futures::future::Future;
use log;

use crate::{actors::storage_keys::MASTER_KEY, config_mngr, storage_mngr};

use std::path::PathBuf;
use witnet_crypto::{
    key::{CryptoEngine, ExtendedPK, ExtendedSK, MasterKeyGen, SignEngine},
    mnemonic::MnemonicGen,
    signature,
};
use witnet_data_structures::{
    chain::{
        ExtendedSecretKey, Hash, Hashable, KeyedSignature, PublicKey, PublicKeyHash, SecretKey,
        Signature, SignaturesToVerify,
    },
    vrf::{VrfCtx, VrfMessage, VrfProof},
};
use witnet_protected::ProtectedString;
use witnet_validations::validations;

/// Start the signature manager
pub fn start() {
    let addr = SignatureManagerAdapter::start_default();
    actix::SystemRegistry::set(addr);
}

/// Sign a piece of (Hashable) data with the stored key.
///
/// This might fail if the manager has not been initialized with a key
pub fn sign<T>(data: &T) -> impl Future<Item = KeyedSignature, Error = failure::Error>
where
    T: Hashable,
{
    let Hash::SHA256(data_hash) = data.hash();

    sign_data(data_hash)
}

/// Sign a piece of data with the stored key.
///
/// This might fail if the manager has not been initialized with a key
pub fn sign_data(data: [u8; 32]) -> impl Future<Item = KeyedSignature, Error = failure::Error> {
    let addr = SignatureManagerAdapter::from_registry();
    addr.send(Sign(data.to_vec())).flatten()
}

/// Get the public key hash.
///
/// This might fail if the manager has not been initialized with a key
pub fn pkh() -> impl Future<Item = PublicKeyHash, Error = failure::Error> {
    let addr = SignatureManagerAdapter::from_registry();
    addr.send(GetPkh).flatten()
}

/// Get the public key.
///
/// This might fail if the manager has not been initialized with a key
pub fn public_key() -> impl Future<Item = PublicKey, Error = failure::Error> {
    let addr = SignatureManagerAdapter::from_registry();
    addr.send(GetPublicKey).flatten()
}

/// Get the public key and secret key.
///
/// This might fail if the manager has not been initialized with a key
pub fn key_pair() -> impl Future<Item = (ExtendedPK, ExtendedSK), Error = failure::Error> {
    let addr = SignatureManagerAdapter::from_registry();
    addr.send(GetKeyPair).flatten()
}

/// Create a VRF proof for the provided message with the stored key
pub fn vrf_prove(
    message: VrfMessage,
) -> impl Future<Item = (VrfProof, Hash), Error = failure::Error> {
    let addr = SignatureManagerAdapter::from_registry();
    addr.send(VrfProve(message)).flatten()
}

/// Verify signatures async
pub fn verify_signatures(
    message: Vec<SignaturesToVerify>,
) -> impl Future<Item = (), Error = failure::Error> {
    let addr = SignatureManagerAdapter::from_registry();
    addr.send(VerifySignatures(message)).flatten()
}

#[derive(Debug, Default)]
struct SignatureManager {
    /// Secret and public key
    keypair: Option<(ExtendedSK, ExtendedPK)>,
    /// VRF context
    vrf_ctx: Option<VrfCtx>,
    /// Secp256k1 context
    secp: Option<CryptoEngine>,
}

impl SignatureManager {
    fn set_key(&mut self, key: ExtendedSK) {
        let public_key = ExtendedPK::from_secret_key(&SignEngine::signing_only(), &key);
        self.keypair = Some((key, public_key));
        log::debug!("Signature Manager received a key and is ready to sign");
    }
}

struct SetKey(ExtendedSK);

struct Sign(Vec<u8>);

struct GetPkh;

struct GetPublicKey;

struct GetKeyPair;

struct VrfProve(VrfMessage);

struct VerifySignatures(Vec<SignaturesToVerify>);

fn persist_master_key(master_key: ExtendedSK) -> impl Future<Item = (), Error = failure::Error> {
    let master_key = ExtendedSecretKey::from(master_key);

    storage_mngr::put(&MASTER_KEY, &master_key).inspect(|_| {
        log::trace!("Successfully persisted the extended secret key into storage");
    })
}

fn create_master_key() -> Box<dyn Future<Item = ExtendedSK, Error = failure::Error>> {
    log::info!("Generating and persisting a new master key for this node");

    // Create a new master key
    let mnemonic = MnemonicGen::new().generate();
    let seed = mnemonic.seed(&ProtectedString::new(""));
    match MasterKeyGen::new(seed).generate() {
        Ok(master_key) => {
            let fut = persist_master_key(master_key.clone()).map(move |_| master_key);

            Box::new(fut)
        }
        Err(e) => {
            let fut = futures::future::err(e.into());

            Box::new(fut)
        }
    }
}

impl Actor for SignatureManager {
    type Context = SyncContext<Self>;

    fn started(&mut self, ctx: &mut Self::Context) {
        log::debug!("Signature Manager actor has been started!");

        self.vrf_ctx = VrfCtx::secp256k1()
            .map_err(|e| {
                log::error!("Failed to initialize VRF context: {}", e);
                // Stop the node
                ctx.stop();
            })
            .ok();

        self.secp = Some(CryptoEngine::new());
    }
}

impl Message for SetKey {
    type Result = Result<(), failure::Error>;
}

impl Message for Sign {
    type Result = Result<KeyedSignature, failure::Error>;
}

impl Message for GetPkh {
    type Result = Result<PublicKeyHash, failure::Error>;
}

impl Message for GetPublicKey {
    type Result = Result<PublicKey, failure::Error>;
}

impl Message for GetKeyPair {
    type Result = Result<(ExtendedPK, ExtendedSK), failure::Error>;
}

impl Message for VrfProve {
    type Result = Result<(VrfProof, Hash), failure::Error>;
}

impl Message for VerifySignatures {
    type Result = Result<(), failure::Error>;
}

impl Handler<SetKey> for SignatureManager {
    type Result = <SetKey as Message>::Result;

    fn handle(&mut self, SetKey(secret_key): SetKey, _ctx: &mut Self::Context) -> Self::Result {
        self.set_key(secret_key);

        Ok(())
    }
}

impl Handler<Sign> for SignatureManager {
    type Result = <Sign as Message>::Result;

    fn handle(&mut self, Sign(data): Sign, _ctx: &mut Self::Context) -> Self::Result {
        match &self.keypair {
            Some((secret, public)) => {
                let signature =
                    signature::sign(self.secp.as_ref().unwrap(), secret.secret_key, &data)?;
                let keyed_signature = KeyedSignature {
                    signature: Signature::from(signature),
                    public_key: PublicKey::from(public.key),
                };

                Ok(keyed_signature)
            }
            None => bail!("Signature Manager cannot sign because it contains no key"),
        }
    }
}

impl Handler<GetPkh> for SignatureManager {
    type Result = <GetPkh as Message>::Result;

    fn handle(&mut self, _msg: GetPkh, _ctx: &mut Self::Context) -> Self::Result {
        match &self.keypair {
            Some((_secret, public)) => Ok(PublicKeyHash::from_public_key(&public.key.into())),
            None => bail!("Tried to retrieve the public key hash for node's main keypair from Signature Manager, but it contains none (looks like it was not initialized properly)"),
        }
    }
}

impl Handler<GetPublicKey> for SignatureManager {
    type Result = <GetPublicKey as Message>::Result;

    fn handle(&mut self, _msg: GetPublicKey, _ctx: &mut Self::Context) -> Self::Result {
        match &self.keypair {
            Some((_secret, public)) => Ok(public.key.into()),
            None => bail!("Tried to retrieve the public key hash for node's main keypair from Signature Manager, but it contains none (looks like it was not initialized properly)"),
        }
    }
}

impl Handler<GetKeyPair> for SignatureManager {
    type Result = <GetKeyPair as Message>::Result;

    fn handle(&mut self, _msg: GetKeyPair, _ctx: &mut Self::Context) -> Self::Result {
        match &self.keypair {
            Some((secret, public)) => {
                Ok((public.clone(), secret.clone()))
            },
            None => bail!("Tried to retrieve the public key hash for node's main keypair from Signature Manager, but it contains none (looks like it was not initialized properly)"),
        }
    }
}

impl Handler<VrfProve> for SignatureManager {
    type Result = <VrfProve as Message>::Result;

    fn handle(&mut self, VrfProve(message): VrfProve, _ctx: &mut Self::Context) -> Self::Result {
        match self {
            Self {
                keypair: Some((secret, _public)),
                vrf_ctx: Some(vrf),
                ..
            } => {
                // This conversion is cheap, it's just a memcpy
                let sk = SecretKey::from(secret.secret_key);
                VrfProof::create(vrf, &sk, &message)
            }
            Self {
                keypair: None,
                ..
            } => bail!("Signature Manager cannot create VRF proofs because it contains no key"),
            Self {
                vrf_ctx: None,
                ..
            } => bail!("Signature Manager cannot create VRF proofs because it does not contain a vrf context"),
        }
    }
}

impl Handler<VerifySignatures> for SignatureManager {
    type Result = <VerifySignatures as Message>::Result;

    fn handle(&mut self, msg: VerifySignatures, _ctx: &mut Self::Context) -> Self::Result {
        validations::verify_signatures(
            msg.0,
            self.vrf_ctx.as_mut().unwrap(),
            self.secp.as_ref().unwrap(),
        )
        .map(|_| ())
    }
}

struct SignatureManagerAdapter {
    crypto: Addr<SignatureManager>,
}

impl Supervised for SignatureManagerAdapter {}

impl SystemService for SignatureManagerAdapter {}

impl Default for SignatureManagerAdapter {
    fn default() -> Self {
        let crypto = SyncArbiter::start(1, SignatureManager::default);
        Self { crypto }
    }
}

impl Actor for SignatureManagerAdapter {
    type Context = Context<Self>;

    fn started(&mut self, ctx: &mut Self::Context) {
        log::debug!("Signature Manager Adapter actor has been started!");
        let crypto = self.crypto.clone();

        config_mngr::get()
            .and_then(move |config| {
                if let Some(master_key_path) = &config.storage.master_key_import_path {
                    futures::done(master_key_import_from_file(master_key_path).map(Some))
                } else {
                    futures::finished(None)
                }
            })
            .and_then(|master_key_from_file| {
                storage_mngr::get::<_, ExtendedSecretKey>(&MASTER_KEY).map(|master_key| {
                    let master_key_from_storage: Option<ExtendedSK> = master_key.map(Into::into);
                    (master_key_from_file, master_key_from_storage)
                })
            })
            .and_then(move |(master_key_from_file, master_key_from_storage)| {
                match (master_key_from_file, master_key_from_storage) {
                    // Didn't ask to import master key and no master key in storage:
                    // Create new master key
                    (None, None) => create_master_key(),
                    // There is a master key in storage or imported, but not both:
                    // Use that master key
                    (None, Some(from_storage)) => Box::new(futures::finished(from_storage)),
                    (Some(from_file), None) => {
                        // Save the key into the storage
                        Box::new(persist_master_key(from_file.clone()).map(|()| from_file))
                    },
                    // There is a master key in storage and imported:
                    (Some(from_file), Some(from_storage)) => {
                        if from_file == from_storage {
                            // If they are equal, use that master key
                            Box::new(futures::finished(from_file))
                        } else {
                            // Else, throw error to avoid overwriting the old master key in storage
                            let node_public_key = ExtendedPK::from_secret_key(&CryptoEngine::new(), &from_storage);
                            let node_pkh = PublicKey::from(node_public_key.key).pkh();

                            let imported_public_key = ExtendedPK::from_secret_key(&CryptoEngine::new(), &from_file);
                            let imported_pkh = PublicKey::from(imported_public_key.key).pkh();

                            Box::new(futures::failed(format_err!(
                                "Tried to overwrite node master key with a different one.\n\
                                 Node pkh:     {}\n\
                                 Imported pkh: {}\n\
                                 \n\
                                 In order to import a different master key, you first need to export the current master key and delete the storage",
                                 node_pkh,
                                 imported_pkh,
                            )))
                        }
                    }
                }
            })
            .and_then(move |master_key| crypto.send(SetKey(master_key)).flatten())
            .map_err(|err| {
                log::error!("Failed to configure master key: {}", err);
                System::current().stop_with_code(1);
            })
            .into_actor(self)
            .wait(ctx);
    }
}

impl Handler<SetKey> for SignatureManagerAdapter {
    type Result = ResponseFuture<(), failure::Error>;

    fn handle(&mut self, msg: SetKey, _ctx: &mut Self::Context) -> Self::Result {
        Box::new(self.crypto.send(msg).flatten())
    }
}

impl Handler<Sign> for SignatureManagerAdapter {
    type Result = ResponseFuture<KeyedSignature, failure::Error>;

    fn handle(&mut self, msg: Sign, _ctx: &mut Self::Context) -> Self::Result {
        Box::new(self.crypto.send(msg).flatten())
    }
}

impl Handler<GetPkh> for SignatureManagerAdapter {
    type Result = ResponseFuture<PublicKeyHash, failure::Error>;

    fn handle(&mut self, msg: GetPkh, _ctx: &mut Self::Context) -> Self::Result {
        Box::new(self.crypto.send(msg).flatten())
    }
}

impl Handler<GetPublicKey> for SignatureManagerAdapter {
    type Result = ResponseFuture<PublicKey, failure::Error>;

    fn handle(&mut self, msg: GetPublicKey, _ctx: &mut Self::Context) -> Self::Result {
        Box::new(self.crypto.send(msg).flatten())
    }
}

impl Handler<GetKeyPair> for SignatureManagerAdapter {
    type Result = ResponseFuture<(ExtendedPK, ExtendedSK), failure::Error>;

    fn handle(&mut self, msg: GetKeyPair, _ctx: &mut Self::Context) -> Self::Result {
        Box::new(self.crypto.send(msg).flatten())
    }
}

impl Handler<VrfProve> for SignatureManagerAdapter {
    type Result = ResponseFuture<(VrfProof, Hash), failure::Error>;

    fn handle(&mut self, msg: VrfProve, _ctx: &mut Self::Context) -> Self::Result {
        Box::new(self.crypto.send(msg).flatten())
    }
}

impl Handler<VerifySignatures> for SignatureManagerAdapter {
    type Result = ResponseFuture<(), failure::Error>;

    fn handle(&mut self, msg: VerifySignatures, _ctx: &mut Self::Context) -> Self::Result {
        Box::new(self.crypto.send(msg).flatten())
    }
}

fn master_key_import_from_file(master_key_path: &PathBuf) -> Result<ExtendedSK, failure::Error> {
    let master_key_path_str = master_key_path.display();
    let ser = match std::fs::read_to_string(master_key_path) {
        Ok(x) => x,
        Err(e) => {
            bail!("Failed to read `{}`: {}", master_key_path_str, e);
        }
    };

    match ExtendedSK::from_slip32(ser.trim()) {
        Ok((extended_master_key, key_path)) => {
            if key_path.is_master() {
                log::info!("Successfully imported master key from file");
                Ok(extended_master_key)
            } else {
                bail!(
                    "The private key stored in `{}` is not a master key",
                    master_key_path_str
                );
            }
        }
        Err(e) => {
            bail!(
                "Failed to deserialize SLIP32 master key from file `{}`: {}",
                master_key_path_str,
                e
            );
        }
    }
}
