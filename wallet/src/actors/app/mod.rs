//! # Application actor.
//!
//! See [`App`](App) actor for more information.
use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use actix::prelude::*;
use futures::future;
use jsonrpc_core as rpc;
use jsonrpc_pubsub as pubsub;
use serde_json::json;

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
    sessions: HashMap<wallet::SessionId, HashSet<wallet::WalletId>>,
    unlocked_wallets: HashMap<wallet::WalletId, HashSet<wallet::SessionId>>,
    wallet_keys: HashMap<wallet::WalletId, wallet::Key>,
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
            subscriptions: Default::default(),
            sessions: Default::default(),
            unlocked_wallets: Default::default(),
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
        id: wallet::WalletId,
        session_id: wallet::SessionId,
        password: ProtectedString,
    ) -> ResponseActFuture<Self, (), app::Error> {
        // check if the wallet has already being unlocked by another session
        match self.unlocked_wallets.get(&id).cloned() {
            Some(mut owner_sessions) => {
                log::debug!(
                    "Wallet {} already unlocked. Appending {} to its list of active sessions.",
                    &id,
                    &session_id
                );
                owner_sessions.insert(id);
                Box::new(fut::ok(()))
            }
            None => {
                let f = self
                    .storage
                    .send(storage::UnlockWallet(self.db.clone(), id, password))
                    .map_err(app::Error::StorageFailed)
                    .and_then(|result| result.map_err(app::Error::Storage))
                    .into_actor(self)
                    .and_then(move |unlocked_wallet, _slf, ctx| {
                        ctx.notify(handlers::WalletUnlocked {
                            session_id,
                            unlocked_wallet,
                        });

                        fut::ok(())
                    });

                Box::new(f)
            }
        }
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

    /// Save wallet in the list of unlocked wallets for the given session.
    fn assoc_wallet_to_session(
        &mut self,
        wallet: wallet::UnlockedWallet,
        session_id: wallet::SessionId,
    ) {
        let id = wallet.id;

        let session_wallets = self
            .sessions
            .entry(session_id.clone())
            .or_insert_with(HashSet::new);
        let wallet_sessions = self
            .unlocked_wallets
            .entry(id.clone())
            .or_insert_with(HashSet::new);

        session_wallets.insert(id.clone());
        wallet_sessions.insert(session_id.clone());
        self.wallet_keys.insert(id.clone(), wallet.key);

        log::debug!("Associated wallet: {} to session: {}", &id, session_id);
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
