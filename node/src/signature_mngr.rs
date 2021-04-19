//! # Signature Manager
//!
//! This module provides a Signature Manager, which, after being
//! initialized with a key, can be used repeatedly to sign data with
//! that key.
use actix::prelude::*;
use failure::{bail, format_err};
use futures_util::FutureExt;

use crate::{
    actors::storage_keys::{BN256_SECRET_KEY, MASTER_KEY},
    config_mngr, storage_mngr,
};

use rand::{thread_rng, Rng};
use std::path::Path;
use witnet_crypto::{
    key::{CryptoEngine, ExtendedPK, ExtendedSK, MasterKeyGen, SignEngine},
    mnemonic::MnemonicGen,
    signature,
};
use witnet_data_structures::{
    chain::{
        Bn256KeyedSignature, Bn256PublicKey, Bn256SecretKey, ExtendedSecretKey, Hash, Hashable,
        KeyedSignature, PublicKey, PublicKeyHash, SecretKey, Signature, SignaturesToVerify,
    },
    transaction::MemoizedHashable,
    vrf::{VrfCtx, VrfMessage, VrfProof},
};
use witnet_futures_utils::{ActorFutureExt, TryFutureExt2};
use witnet_protected::ProtectedString;
use witnet_validations::validations;

/// Sign a transaction using this node's private key.
/// This function assumes that all the inputs have the same public key hash:
/// the hash of the public key of the node.
pub fn sign_transaction<T>(
    tx: &T,
    inputs_len: usize,
) -> impl Future<Output = Result<Vec<KeyedSignature>, failure::Error>>
where
    T: MemoizedHashable + Hashable,
{
    let a = sign(tx);
    async move {
        // Assuming that all the inputs have the same pkh
        a.await.map(move |signature| {
            // TODO: do we need to sign:
            // value transfer inputs,
            // data request inputs (for commits),
            // commit inputs (for reveals),
            //
            // We do not need to sign:
            // reveal inputs (for tallies)
            //
            // But currently we just sign everything, hoping that the validations
            // work
            vec![signature; inputs_len]
        })
    }
}

/// Start the signature manager
pub fn start() {
    let addr = SignatureManagerAdapter::start_default();
    actix::SystemRegistry::set(addr);
}

/// Sign a piece of (Hashable) data with the stored key.
///
/// This might fail if the manager has not been initialized with a key
pub fn sign<T>(data: &T) -> impl Future<Output = Result<KeyedSignature, failure::Error>>
where
    T: Hashable,
{
    let Hash::SHA256(data_hash) = data.hash();

    async move { sign_data(data_hash).await }
}

/// Sign a piece of data with the stored key.
///
/// This might fail if the manager has not been initialized with a key
pub async fn sign_data(data: [u8; 32]) -> Result<KeyedSignature, failure::Error> {
    let addr = SignatureManagerAdapter::from_registry();
    addr.send(Sign(data.to_vec())).flatten_err().await
}

/// Sign a piece of (Hashable) data with the stored key.
///
/// This might fail if the manager has not been initialized with a key
pub async fn bn256_sign(message: Vec<u8>) -> Result<Bn256KeyedSignature, failure::Error> {
    let addr = SignatureManagerAdapter::from_registry();
    addr.send(Bn256Sign(message)).flatten_err().await
}

/// Get the public key hash.
///
/// This might fail if the manager has not been initialized with a key
pub async fn pkh() -> Result<PublicKeyHash, failure::Error> {
    let addr = SignatureManagerAdapter::from_registry();
    addr.send(GetPkh).flatten_err().await
}

/// Get the public key.
///
/// This might fail if the manager has not been initialized with a key
pub async fn public_key() -> Result<PublicKey, failure::Error> {
    let addr = SignatureManagerAdapter::from_registry();
    addr.send(GetPublicKey).flatten_err().await
}

/// Get the BN256 public key.
///
/// This might fail if the manager has not been initialized with a key
pub async fn bn256_public_key() -> Result<Bn256PublicKey, failure::Error> {
    let addr = SignatureManagerAdapter::from_registry();
    addr.send(GetBn256PublicKey).flatten_err().await
}

/// Get the public key and secret key.
///
/// This might fail if the manager has not been initialized with a key
pub async fn key_pair() -> Result<(ExtendedPK, ExtendedSK), failure::Error> {
    let addr = SignatureManagerAdapter::from_registry();
    addr.send(GetKeyPair).flatten_err().await
}

/// Get the BN256 public key and secret key.
///
/// This might fail if the manager has not been initialized with a key
pub async fn bn256_key_pair() -> Result<(Bn256PublicKey, Bn256SecretKey), failure::Error> {
    let addr = SignatureManagerAdapter::from_registry();
    addr.send(GetBn256KeyPair).flatten_err().await
}

/// Create a VRF proof for the provided message with the stored key
pub async fn vrf_prove(message: VrfMessage) -> Result<(VrfProof, Hash), failure::Error> {
    let addr = SignatureManagerAdapter::from_registry();
    addr.send(VrfProve(message)).flatten_err().await
}

/// Verify signatures async
pub async fn verify_signatures(message: Vec<SignaturesToVerify>) -> Result<(), failure::Error> {
    let addr = SignatureManagerAdapter::from_registry();
    addr.send(VerifySignatures(message)).flatten_err().await
}

#[derive(Debug, Default)]
struct SignatureManager {
    /// Secret and public key
    keypair: Option<(ExtendedSK, ExtendedPK)>,
    /// BLS secret and public key
    bls_keypair: Option<(Bn256SecretKey, Bn256PublicKey)>,
    /// VRF context
    vrf_ctx: Option<VrfCtx>,
    /// Secp256k1 context
    secp: Option<CryptoEngine>,
}

struct SetKey(ExtendedSK);

struct SetBn256Key(Bn256SecretKey);

// Note: this message is hashed
struct Sign(Vec<u8>);

// Note: this message is not hashed
struct Bn256Sign(Vec<u8>);

struct GetPkh;

struct GetPublicKey;

struct GetBn256PublicKey;

struct GetKeyPair;

struct GetBn256KeyPair;

struct VrfProve(VrfMessage);

struct VerifySignatures(Vec<SignaturesToVerify>);

async fn persist_master_key(master_key: ExtendedSK) -> Result<(), failure::Error> {
    let master_key = ExtendedSecretKey::from(master_key);

    storage_mngr::put(&MASTER_KEY, &master_key)
        .inspect(|_| {
            log::trace!("Successfully persisted the extended secret key into storage");
        })
        .await
}

async fn persist_bn256_key(bn256_secret_key: Bn256SecretKey) -> Result<(), failure::Error> {
    storage_mngr::put(&BN256_SECRET_KEY, &bn256_secret_key)
        .inspect(|_| {
            log::trace!("Successfully persisted the BN256 secret key into storage");
        })
        .await
}

async fn create_master_key() -> Result<ExtendedSK, failure::Error> {
    log::info!("Generating and persisting a new master key for this node");

    // Create a new master key
    let mnemonic = MnemonicGen::new().generate();
    let seed = mnemonic.seed(&ProtectedString::new(""));
    match MasterKeyGen::new(seed).generate() {
        Ok(master_key) => persist_master_key(master_key.clone())
            .await
            .map(move |_| master_key),
        Err(e) => Err(e.into()),
    }
}

async fn create_bn256_key() -> Result<Bn256SecretKey, failure::Error> {
    // The secret key is 32 bytes
    let secret: [u8; 32] = thread_rng().gen();
    let bls_secret = match Bn256SecretKey::from_slice(&secret) {
        Ok(bls_secret) => bls_secret,
        Err(e) => {
            // This should never happen, as any 256-bit integer can be converted into a valid
            // BN256 secret key
            log::error!("Failed to generate BN256 secret key: {}", e);
            return Err(e);
        }
    };

    match Bn256PublicKey::from_secret_key(&bls_secret) {
        Ok(_bls_public) => persist_bn256_key(bls_secret.clone())
            .await
            .map(move |_| bls_secret),
        Err(e) => {
            // This should never happen, as any valid private key can be used to derive a valid
            // BN256 public key
            log::error!("Failed to derive BN256 public key: {}", e);
            Err(e)
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

impl Message for SetBn256Key {
    type Result = Result<(), failure::Error>;
}

impl Message for Sign {
    type Result = Result<KeyedSignature, failure::Error>;
}

impl Message for Bn256Sign {
    type Result = Result<Bn256KeyedSignature, failure::Error>;
}

impl Message for GetPkh {
    type Result = Result<PublicKeyHash, failure::Error>;
}

impl Message for GetPublicKey {
    type Result = Result<PublicKey, failure::Error>;
}

impl Message for GetBn256PublicKey {
    type Result = Result<Bn256PublicKey, failure::Error>;
}

impl Message for GetKeyPair {
    type Result = Result<(ExtendedPK, ExtendedSK), failure::Error>;
}

impl Message for GetBn256KeyPair {
    type Result = Result<(Bn256PublicKey, Bn256SecretKey), failure::Error>;
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
        let public_key = ExtendedPK::from_secret_key(&SignEngine::signing_only(), &secret_key);
        self.keypair = Some((secret_key, public_key));
        log::debug!("Signature Manager received master key and is ready to sign");

        Ok(())
    }
}

impl Handler<SetBn256Key> for SignatureManager {
    type Result = <SetBn256Key as Message>::Result;

    fn handle(
        &mut self,
        SetBn256Key(secret_key): SetBn256Key,
        _ctx: &mut Self::Context,
    ) -> Self::Result {
        let public_key = Bn256PublicKey::from_secret_key(&secret_key)?;
        self.bls_keypair = Some((secret_key, public_key));
        log::debug!("Signature Manager received BN256 key and is ready to sign");

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

impl Handler<Bn256Sign> for SignatureManager {
    type Result = <Bn256Sign as Message>::Result;

    fn handle(&mut self, Bn256Sign(message): Bn256Sign, _ctx: &mut Self::Context) -> Self::Result {
        match &self.bls_keypair {
            Some((secret, public)) => {
                let signature = secret.sign(&message)?;
                let keyed_signature = Bn256KeyedSignature {
                    signature,
                    public_key: public.clone(),
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
            None => bail!("Tried to retrieve the public key for node's main keypair from Signature Manager, but it contains none (looks like it was not initialized properly)"),
        }
    }
}

impl Handler<GetBn256PublicKey> for SignatureManager {
    type Result = <GetBn256PublicKey as Message>::Result;

    fn handle(&mut self, _msg: GetBn256PublicKey, _ctx: &mut Self::Context) -> Self::Result {
        match &self.bls_keypair {
            Some((_secret, public)) => Ok(public.clone()),
            None => bail!("Tried to retrieve the public key for node's BLS keypair from Signature Manager, but it contains none (looks like it was not initialized properly)"),
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
            None => bail!("Tried to retrieve the public and secret key for node's main keypair from Signature Manager, but it contains none (looks like it was not initialized properly)"),
        }
    }
}

impl Handler<GetBn256KeyPair> for SignatureManager {
    type Result = <GetBn256KeyPair as Message>::Result;

    fn handle(&mut self, _msg: GetBn256KeyPair, _ctx: &mut Self::Context) -> Self::Result {
        match &self.bls_keypair {
            Some((secret, public)) => {
                Ok((public.clone(), secret.clone()))
            },
            None => bail!("Tried to retrieve the public and secret key for node's BLS keypair from Signature Manager, but it contains none (looks like it was not initialized properly)"),
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

        async move {
            let config = config_mngr::get().await?;
            let master_key_from_file = if let Some(master_key_path) = &config.storage.master_key_import_path {
                master_key_import_from_file(master_key_path).map(Some)
            } else {
                Ok(None)
            }?;
            let master_key_from_storage: Option<ExtendedSK> = storage_mngr::get::<_, ExtendedSecretKey>(&MASTER_KEY).await?.map(Into::into);

            let master_key = match (master_key_from_file, master_key_from_storage) {
                // Didn't ask to import master key and no master key in storage:
                // Create new master key
                (None, None) => create_master_key().await,
                // There is a master key in storage or imported, but not both:
                // Use that master key
                (None, Some(from_storage)) => Ok(from_storage),
                (Some(from_file), None) => {
                    // Save the key into the storage
                    persist_master_key(from_file.clone()).await.map(|()| from_file)
                },
                // There is a master key in storage and imported:
                (Some(from_file), Some(from_storage)) => {
                    if from_file == from_storage {
                        // If they are equal, use that master key
                        Ok(from_file)
                    } else {
                        // Else, throw error to avoid overwriting the old master key in storage
                        let node_public_key = ExtendedPK::from_secret_key(&CryptoEngine::new(), &from_storage);
                        let node_pkh = PublicKey::from(node_public_key.key).pkh();

                        let imported_public_key = ExtendedPK::from_secret_key(&CryptoEngine::new(), &from_file);
                        let imported_pkh = PublicKey::from(imported_public_key.key).pkh();

                        Err(format_err!(
                            "Tried to overwrite node master key with a different one.\n\
                             Node pkh:     {}\n\
                             Imported pkh: {}\n\
                             \n\
                             In order to import a different master key, you first need to export the current master key and delete the storage",
                             node_pkh,
                             imported_pkh,
                        ))
                    }
                }
            }?;

            crypto.send(SetKey(master_key)).await?
        }.into_actor(self)
            .map_err(|err, _act, _ctx| {
                log::error!("Failed to configure master key: {}", err);
                System::current().stop_with_code(1);
            })
            .map(|_res: Result<(), ()>, _act, _ctx| ())
             .wait(ctx);

        let crypto = self.crypto.clone();
        async move {
            let secret_key = storage_mngr::get::<_, Bn256SecretKey>(&BN256_SECRET_KEY).await?;
            let secret_key = match secret_key {
                None => create_bn256_key().await,
                Some(from_storage) => Ok(from_storage),
            }?;

            crypto.send(SetBn256Key(secret_key)).flatten_err().await
        }
        .into_actor(self)
        .map_err(|err: failure::Error, _act, _ctx| {
            log::warn!("Failed to configure BN256 key: {}", err);
        })
        .map(|_res: Result<(), ()>, _act, _ctx| ())
        .wait(ctx);
    }
}

impl Handler<SetKey> for SignatureManagerAdapter {
    type Result = ResponseFuture<Result<(), failure::Error>>;

    fn handle(&mut self, msg: SetKey, _ctx: &mut Self::Context) -> Self::Result {
        Box::pin(self.crypto.send(msg).flatten_err())
    }
}

impl Handler<SetBn256Key> for SignatureManagerAdapter {
    type Result = ResponseFuture<Result<(), failure::Error>>;

    fn handle(&mut self, msg: SetBn256Key, _ctx: &mut Self::Context) -> Self::Result {
        Box::pin(self.crypto.send(msg).flatten_err())
    }
}

impl Handler<Sign> for SignatureManagerAdapter {
    type Result = ResponseFuture<Result<KeyedSignature, failure::Error>>;

    fn handle(&mut self, msg: Sign, _ctx: &mut Self::Context) -> Self::Result {
        Box::pin(self.crypto.send(msg).flatten_err())
    }
}

impl Handler<Bn256Sign> for SignatureManagerAdapter {
    type Result = ResponseFuture<Result<Bn256KeyedSignature, failure::Error>>;

    fn handle(&mut self, msg: Bn256Sign, _ctx: &mut Self::Context) -> Self::Result {
        Box::pin(self.crypto.send(msg).flatten_err())
    }
}

impl Handler<GetPkh> for SignatureManagerAdapter {
    type Result = ResponseFuture<Result<PublicKeyHash, failure::Error>>;

    fn handle(&mut self, msg: GetPkh, _ctx: &mut Self::Context) -> Self::Result {
        Box::pin(self.crypto.send(msg).flatten_err())
    }
}

impl Handler<GetPublicKey> for SignatureManagerAdapter {
    type Result = ResponseFuture<Result<PublicKey, failure::Error>>;

    fn handle(&mut self, msg: GetPublicKey, _ctx: &mut Self::Context) -> Self::Result {
        Box::pin(self.crypto.send(msg).flatten_err())
    }
}

impl Handler<GetBn256PublicKey> for SignatureManagerAdapter {
    type Result = ResponseFuture<Result<Bn256PublicKey, failure::Error>>;

    fn handle(&mut self, msg: GetBn256PublicKey, _ctx: &mut Self::Context) -> Self::Result {
        Box::pin(self.crypto.send(msg).flatten_err())
    }
}

impl Handler<GetKeyPair> for SignatureManagerAdapter {
    type Result = ResponseFuture<Result<(ExtendedPK, ExtendedSK), failure::Error>>;

    fn handle(&mut self, msg: GetKeyPair, _ctx: &mut Self::Context) -> Self::Result {
        Box::pin(self.crypto.send(msg).flatten_err())
    }
}

impl Handler<GetBn256KeyPair> for SignatureManagerAdapter {
    type Result = ResponseFuture<Result<(Bn256PublicKey, Bn256SecretKey), failure::Error>>;

    fn handle(&mut self, msg: GetBn256KeyPair, _ctx: &mut Self::Context) -> Self::Result {
        Box::pin(self.crypto.send(msg).flatten_err())
    }
}

impl Handler<VrfProve> for SignatureManagerAdapter {
    type Result = ResponseFuture<Result<(VrfProof, Hash), failure::Error>>;

    fn handle(&mut self, msg: VrfProve, _ctx: &mut Self::Context) -> Self::Result {
        Box::pin(self.crypto.send(msg).flatten_err())
    }
}

impl Handler<VerifySignatures> for SignatureManagerAdapter {
    type Result = ResponseFuture<Result<(), failure::Error>>;

    fn handle(&mut self, msg: VerifySignatures, _ctx: &mut Self::Context) -> Self::Result {
        Box::pin(self.crypto.send(msg).flatten_err())
    }
}

fn master_key_import_from_file(master_key_path: &Path) -> Result<ExtendedSK, failure::Error> {
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
