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
                    signature::sign(self.secp.as_ref().unwrap(), secret.secret_key, &data);
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

        storage_mngr::get::<_, ExtendedSecretKey>(&MASTER_KEY)
            .and_then(move |master_key_from_storage| {
                master_key_from_storage.map_or_else(create_master_key, |master_key| {
                    let master_key: ExtendedSK = master_key.into();
                    let fut = futures::future::ok(master_key);

                    Box::new(fut)
                })
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
