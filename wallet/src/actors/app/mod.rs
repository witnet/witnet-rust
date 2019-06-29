//! # Application actor.
//!
//! See [`App`](App) actor for more information.
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use actix::prelude::*;
use actix::utils::TimerFunc;
use futures::future;
use jsonrpc_core as rpc;
use jsonrpc_pubsub as pubsub;
use serde_json::json;

use witnet_crypto::mnemonic::MnemonicGen;
use witnet_net::client::tcp::{jsonrpc as rpc_client, JsonRpcClient};
use witnet_protected::ProtectedString;
use witnet_rad as rad;

use crate::actors::{crypto, rad_executor, storage, Crypto, RadExecutor, Storage};
use crate::{app, wallet};

pub mod handlers;

/// Expose message to stop application.
pub use handlers::Stop;

/// Application actor.
///
/// The application actor is in charge of managing the state of the application and coordinating the
/// service actors, e.g.: storage, node client, and so on.
pub struct App {
    db: Arc<rocksdb::DB>,
    storage: Addr<Storage>,
    rad_executor: Addr<RadExecutor>,
    crypto: Addr<Crypto>,
    node_client: Option<Addr<JsonRpcClient>>,
    subscriptions: [Option<pubsub::Sink>; 10],
    sessions: HashMap<app::SessionId, wallet::WalletId>,
    session_expiration: Duration,
    wallet_keys: HashMap<wallet::WalletId, Arc<wallet::Key>>,
}

impl App {
    /// Start actor.
    pub fn start(
        db: rocksdb::DB,
        storage: Addr<Storage>,
        crypto: Addr<Crypto>,
        rad_executor: Addr<RadExecutor>,
        node_client: Option<Addr<JsonRpcClient>>,
    ) -> Addr<Self> {
        let slf = Self {
            db: Arc::new(db),
            storage,
            crypto,
            rad_executor,
            node_client,
            session_expiration: Duration::from_secs(3600),
            subscriptions: Default::default(),
            sessions: Default::default(),
            wallet_keys: Default::default(),
        };

        slf.start()
    }

    /// Run a RADRequest and return the computed result.
    pub fn run_rad_request(
        &self,
        req: wallet::RADRequest,
    ) -> ResponseFuture<rad::types::RadonTypes, app::Error> {
        let f = self
            .rad_executor
            .send(rad_executor::Run(req))
            .map_err(app::Error::RadScheduleFailed)
            .and_then(|result| result.map_err(app::Error::RadFailed));

        Box::new(f)
    }

    /// Generate a random BIP39 mnemonics sentence
    pub fn generate_mnemonics(&self, params: app::CreateMnemonics) -> String {
        let mnemonic = MnemonicGen::new().with_len(params.length).generate();
        let words = mnemonic.words();

        words.to_string()
    }

    /// Return an id for a new subscription. If there are no available subscription slots, then
    /// `None` is returned.
    pub fn subscribe(&mut self, subscriber: pubsub::Subscriber) -> Result<usize, app::Error> {
        let (id, slot) = self
            .subscriptions
            .iter_mut()
            .enumerate()
            .find(|(_, slot)| slot.is_none())
            .ok_or_else(|| app::Error::SubscribeFailed("max limit of subscriptions reached"))?;

        *slot = subscriber
            .assign_id(pubsub::SubscriptionId::from(id as u64))
            .ok();

        Ok(id)
    }

    /// Remove a subscription and leave its corresponding slot free.
    pub fn unsubscribe(&mut self, id: pubsub::SubscriptionId) -> Result<(), app::Error> {
        let index = match id {
            pubsub::SubscriptionId::Number(n) => Ok(n as usize),
            _ => Err(app::Error::UnsubscribeFailed(
                "subscription id must be a number",
            )),
        }?;
        let slot = self
            .subscriptions
            .as_mut()
            .get_mut(index)
            .ok_or_else(|| app::Error::UnsubscribeFailed("subscription id not found"))?;

        *slot = None;

        Ok(())
    }

    /// Forward a Json-RPC call to the node.
    pub fn forward(
        &mut self,
        method: String,
        params: rpc::Params,
    ) -> ResponseFuture<serde_json::Value, app::Error> {
        match &self.node_client {
            Some(addr) => {
                let req = rpc_client::Request::method(method)
                    .params(params)
                    .expect("rpc::Params failed serialization");
                let f = addr
                    .send(req)
                    .map_err(app::Error::RequestFailedToSend)
                    .and_then(|result| result.map_err(app::Error::RequestFailed));

                Box::new(f)
            }
            None => {
                let f = future::err(app::Error::NodeNotConnected);

                Box::new(f)
            }
        }
    }

    /// Get id and caption of all the wallets stored in the database.
    fn get_wallet_infos(&self) -> ResponseFuture<Vec<wallet::WalletInfo>, app::Error> {
        let fut = self
            .storage
            .send(storage::GetWalletInfos(self.db.clone()))
            .map_err(app::Error::StorageFailed)
            .and_then(|result| result.map_err(app::Error::Storage));

        Box::new(fut)
    }

    /// Create an empty HD Wallet.
    fn create_wallet(
        &self,
        params: app::CreateWallet,
    ) -> ResponseActFuture<Self, wallet::WalletId, app::Error> {
        let app::CreateWallet {
            name,
            caption,
            password,
            seed_source,
        } = params;
        let key_spec = wallet::Wip::Wip3;
        let fut = self
            .crypto
            .send(crypto::GenWalletKeys(seed_source))
            .map_err(app::Error::CryptoFailed)
            .and_then(|result| result.map_err(app::Error::Crypto))
            .into_actor(self)
            .and_then(move |(id, master_key), slf, _ctx| {
                // Keypath: m/3'/4919'/0'
                let keypath = wallet::KeyPath::master()
                    .hardened(3)
                    .hardened(4919)
                    .hardened(0);
                let keychains = wallet::KeyChains::new(keypath);
                let account = wallet::Account::new(keychains);
                let content = wallet::WalletContent::new(master_key, key_spec, vec![account]);
                let info = wallet::WalletInfo {
                    id: id.clone(),
                    name,
                    caption,
                };
                let wallet = wallet::Wallet::new(info, content);

                slf.storage
                    .send(storage::CreateWallet(slf.db.clone(), wallet, password))
                    .map_err(app::Error::StorageFailed)
                    .map(move |_| id)
                    .into_actor(slf)
            });

        Box::new(fut)
    }

    /// Unlock a wallet, that is, add its encryption/decryption key to the list of known keys so
    /// further wallet operations can be performed.
    fn unlock_wallet(
        &mut self,
        wallet_id: wallet::WalletId,
        password: ProtectedString,
    ) -> ResponseActFuture<Self, app::SessionId, app::Error> {
        // check if the wallet has already being unlocked by another session
        match self.wallet_keys.get(&wallet_id).cloned() {
            Some(wallet_key) => {
                let f = self
                    .crypto
                    .send(crypto::GenSessionId(wallet_key.clone()))
                    .map_err(app::Error::CryptoFailed)
                    .into_actor(self)
                    .and_then(|id, slf, _ctx| {
                        log::debug!("Wallet already unlocked by another session.");
                        let session_id = Arc::new(id);
                        slf.sessions.insert(session_id.clone(), wallet_id);
                        fut::ok(session_id)
                    });

                Box::new(f)
            }
            None => {
                let f = self
                    .storage
                    .send(storage::UnlockWallet(
                        self.db.clone(),
                        wallet_id.clone(),
                        password,
                    ))
                    .map_err(app::Error::StorageFailed)
                    .and_then(|result| result.map_err(app::Error::Storage))
                    .into_actor(self)
                    .and_then(move |key, slf, _ctx| {
                        let wallet_key = Arc::new(key);
                        slf.crypto
                            .send(crypto::GenSessionId(wallet_key.clone()))
                            .map_err(app::Error::CryptoFailed)
                            .into_actor(slf)
                            .and_then(move |id, slf, _ctx| {
                                log::debug!("Unlocking wallet.");
                                let session_id = Arc::new(id);
                                slf.sessions.insert(session_id.clone(), wallet_id.clone());
                                slf.wallet_keys.insert(wallet_id, wallet_key);

                                fut::ok(session_id)
                            })
                    });

                Box::new(f)
            }
        }
    }

    /// Return a timer function that can be scheduled to expire the session after the configured time.
    fn set_session_to_expire(&self, session_id: app::SessionId) -> TimerFunc<Self> {
        log::debug!(
            "Session {} will expire in {} seconds.",
            session_id.as_ref(),
            self.session_expiration.as_secs()
        );
        TimerFunc::new(self.session_expiration, move |slf: &mut Self, _ctx| {
            slf.close_session(session_id)
        })
    }

    /// Remove a session from the list of active sessions.
    fn close_session(&mut self, session_id: app::SessionId) {
        log::info!("Session {} expired.", session_id.as_ref());
        self.sessions.remove(&session_id);
    }

    /// Perform all the tasks needed to properly stop the application.
    fn stop(&self) -> ResponseFuture<(), app::Error> {
        let fut = self
            .storage
            .send(storage::Flush(self.db.clone()))
            .map_err(app::Error::StorageFailed)
            .and_then(|result| result.map_err(app::Error::Storage));

        Box::new(fut)
    }
}

impl Actor for App {
    type Context = Context<Self>;

    fn started(&mut self, ctx: &mut Self::Context) {
        if let Some(ref client) = self.node_client {
            let recipient = ctx.address().recipient();
            let request =
                rpc_client::Request::method("witnet_subscribe").value(json!(["newBlocks"]));
            client.do_send(rpc_client::SetSubscriber(recipient, request));
        }
    }
}

impl Supervised for App {}
