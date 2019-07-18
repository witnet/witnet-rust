//! # Application actor.
//!
//! See [`App`](App) actor for more information.
use std::sync::Arc;
use std::time::Duration;

use actix::prelude::*;
use actix::utils::TimerFunc;
use futures::future;
use jsonrpc_core as rpc;
use jsonrpc_pubsub as pubsub;
use serde_json::json;

use witnet_crypto::mnemonic::MnemonicGen;
use witnet_data_structures::chain::Block;
use witnet_net::client::tcp::{jsonrpc as rpc_client, JsonRpcClient};
use witnet_protected::ProtectedString;
use witnet_rad as rad;

use crate::actors::{crypto, rad_executor, storage, Crypto, RadExecutor, Storage};
use crate::{app, types};

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
    requests_timeout: Duration,
    session_expiration: Duration,
    sessions: types::Sessions,
}

impl App {
    /// Start actor.
    pub fn start(
        db: rocksdb::DB,
        storage: Addr<Storage>,
        crypto: Addr<Crypto>,
        rad_executor: Addr<RadExecutor>,
        node_client: Option<Addr<JsonRpcClient>>,
        session_expiration: Duration,
        requests_timeout: Duration,
    ) -> Addr<Self> {
        let slf = Self {
            db: Arc::new(db),
            storage,
            crypto,
            rad_executor,
            node_client,
            session_expiration,
            requests_timeout,
            sessions: Default::default(),
        };

        slf.start()
    }

    /// Return a new subscription id for a session.
    pub fn next_subscription_id(
        &mut self,
        session_id: types::SessionId,
    ) -> Result<types::SubscriptionId, app::Error> {
        if self.sessions.exists(&session_id) {
            // We are re-using the session id as the subscription id, this is because using a number
            // can let any client call the unsubscribe method for any other session.
            Ok(self.sessions.new_subscription_id(&session_id))
        } else {
            Err(app::Error::UnknownSession)
        }
    }

    /// Run a RADRequest and return the computed result.
    pub fn run_rad_request(
        &self,
        req: types::RADRequest,
    ) -> ResponseFuture<rad::types::RadonTypes, app::Error> {
        let f = self
            .rad_executor
            .send(rad_executor::Run(req))
            .map_err(app::Error::RadScheduleFailed)
            .and_then(|result| result.map_err(app::Error::RadFailed));

        Box::new(f)
    }

    /// Generate a random BIP39 mnemonics sentence
    pub fn generate_mnemonics(&self, params: types::CreateMnemonics) -> String {
        let mnemonic = MnemonicGen::new().with_len(params.length).generate();
        let words = mnemonic.words();

        words.to_string()
    }

    /// Try to create a subscription and store it in the session. After subscribing, events related
    /// to wallets unlocked by this session will be sent to the client.
    pub fn subscribe(
        &mut self,
        session_id: types::SessionId,
        subscription_id: types::SubscriptionId,
        sink: pubsub::Sink,
    ) -> Result<(), app::Error> {
        let mut session = self
            .sessions
            .with_session(session_id)
            .ok_or_else(|| app::Error::UnknownSession)?;

        session.add_subscription(subscription_id, sink);

        Ok(())
    }

    /// Remove a subscription and leave its corresponding slot free.
    pub fn unsubscribe(&mut self, id: &types::SubscriptionId) -> Result<pubsub::Sink, app::Error> {
        self.sessions
            .remove_subscription(&id)
            .ok_or_else(|| app::Error::UnknownSession)
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
                    .timeout(self.requests_timeout)
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
    fn get_wallet_infos(&self) -> ResponseFuture<Vec<types::WalletInfo>, app::Error> {
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
        params: types::CreateWallet,
    ) -> ResponseActFuture<Self, types::WalletId, app::Error> {
        let types::CreateWallet {
            name,
            caption,
            password,
            seed_source,
        } = params;
        let key_spec = types::Wip::Wip3;
        let fut = self
            .crypto
            .send(crypto::GenWalletKeys(seed_source))
            .map_err(app::Error::CryptoFailed)
            .and_then(|result| result.map_err(app::Error::Crypto))
            .into_actor(self)
            .and_then(move |(id, master_key), slf, _ctx| {
                // Keypath: m/3'/4919'/0'
                let keypath = types::KeyPath::master()
                    .hardened(3)
                    .hardened(4919)
                    .hardened(0);
                let keychains = types::KeyChains::new(keypath);
                let account = types::Account::new(keychains);
                let content = types::WalletContent::new(master_key, key_spec, vec![account]);
                let info = types::WalletInfo {
                    id: id.clone(),
                    name,
                    caption,
                };
                let wallet = types::Wallet::new(info, content);

                slf.storage
                    .send(storage::CreateWallet(slf.db.clone(), wallet, password))
                    .map_err(app::Error::StorageFailed)
                    .map(move |_| id)
                    .into_actor(slf)
            });

        Box::new(fut)
    }

    /// Lock a wallet, that is, remove its encryption/decryption key from the list of known keys and
    /// close the session.
    ///
    /// This means the state of this wallet won't be updated with information received from the
    /// node.
    fn lock_wallet(
        &mut self,
        session_id: types::SessionId,
        wallet_id: types::WalletId,
    ) -> Result<(), app::Error> {
        let mut session = self
            .sessions
            .with_session(session_id)
            .ok_or_else(|| app::Error::UnknownSession)?;
        let wallet = session
            .with_wallet(wallet_id)
            .ok_or_else(|| app::Error::WrongWallet)?;

        wallet.lock();

        Ok(())
    }

    /// Unlock a wallet, that is, add its encryption/decryption key to the list of known keys so
    /// further wallet operations can be performed.
    fn unlock_wallet(
        &mut self,
        wallet_id: types::WalletId,
        password: ProtectedString,
    ) -> ResponseActFuture<Self, types::SessionId, app::Error> {
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
            .and_then(move |unlocked_wallet, slf, _ctx| {
                slf.crypto
                    .send(crypto::GenSessionId(unlocked_wallet.key.clone()))
                    .map_err(app::Error::CryptoFailed)
                    .into_actor(slf)
                    .and_then(move |session_id, slf, _ctx| {
                        log::info!("Wallet {} unlocked", &wallet_id);
                        slf.sessions
                            .register(session_id.clone(), wallet_id, unlocked_wallet);

                        fut::ok(session_id)
                    })
            });

        Box::new(f)
    }

    /// Return a timer function that can be scheduled to expire the session after the configured time.
    fn set_session_to_expire(&self, session_id: types::SessionId) -> TimerFunc<Self> {
        log::debug!(
            "Session {} will expire in {} seconds.",
            session_id.as_ref(),
            self.session_expiration.as_secs()
        );
        TimerFunc::new(
            self.session_expiration,
            move |slf: &mut Self, _ctx| match slf.close_session(session_id.clone()) {
                Ok(_) => log::info!("Session {} closed", session_id),
                Err(err) => log::error!("Session {} couldn't be closed: {}", session_id, err),
            },
        )
    }

    /// Remove a session from the list of active sessions.
    fn close_session(&mut self, session_id: types::SessionId) -> Result<(), app::Error> {
        log::info!("Closing session {}.", session_id.as_ref());
        let session = self
            .sessions
            .with_session(session_id)
            .ok_or_else(|| app::Error::UnknownSession)?;

        session.close();

        Ok(())
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

    fn handle_block_notification(&mut self, value: serde_json::Value) -> Result<(), app::Error> {
        let block = serde_json::from_value::<Block>(value).map_err(app::Error::ParseNewBlock)?;

        Ok(())
    }
}

impl Actor for App {
    type Context = Context<Self>;

    fn started(&mut self, ctx: &mut Self::Context) {
        if let Some(ref client) = self.node_client {
            let recipient = ctx.address().recipient();
            let request = rpc_client::Request::method("witnet_subscribe")
                .timeout(self.requests_timeout)
                .value(json!(["newBlocks"]));
            client.do_send(rpc_client::SetSubscriber(recipient, request));
        }
    }
}

impl Supervised for App {}
