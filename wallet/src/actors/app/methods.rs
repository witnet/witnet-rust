use actix::utils::TimerFunc;
use futures::future;

use super::*;
use crate::actors::*;
use crate::model;

impl App {
    pub fn start(db: rocksdb::DB, params: Params) -> Addr<Self> {
        let actor = Self {
            params,
            db: Arc::new(db),
            state: Default::default(),
        };

        actor.start()
    }

    /// Return a new subscription id for a session.
    pub fn next_subscription_id(&mut self, session_id: String) -> Result<types::SubscriptionId> {
        if self.state.is_session_active(&session_id) {
            // We are re-using the session id as the subscription id, this is because using a number
            // can let any client call the unsubscribe method for any other session.
            Ok(types::SubscriptionId::String(session_id.clone()))
        } else {
            Err(Error::SessionNotFound)
        }
    }

    /// Try to create a subscription and store it in the session. After subscribing, events related
    /// to wallets unlocked by this session will be sent to the client.
    pub fn subscribe(
        &mut self,
        session_id: String,
        subscription_id: types::SubscriptionId,
        sink: types::Sink,
    ) -> Result<()> {
        self.state.subscribe(&session_id, subscription_id, sink)
    }

    /// Remove a subscription.
    pub fn unsubscribe(&mut self, id: &types::SubscriptionId) -> Result<()> {
        self.state.unsubscribe(id)
    }

    /// Generate a receive address for the wallet's current account.
    pub fn generate_address(
        &mut self,
        session_id: String,
        wallet_id: String,
        label: Option<String>,
    ) -> ResponseActFuture<model::Address> {
        let f = fut::result(self.state.wallet(&session_id, &wallet_id)).and_then(
            move |wallet, slf: &mut Self, _| {
                slf.params
                    .worker
                    .send(worker::GenAddress(slf.db.clone(), wallet, label))
                    .flatten()
                    .map_err(From::from)
                    .into_actor(slf)
            },
        );

        Box::new(f)
    }

    /// Get a list of addresses generated by a wallet.
    pub fn get_addresses(
        &mut self,
        session_id: String,
        wallet_id: String,
        offset: u32,
        limit: u32,
    ) -> ResponseActFuture<model::Addresses> {
        let f = fut::result(self.state.wallet(&session_id, &wallet_id)).and_then(
            move |wallet, slf: &mut Self, _| {
                slf.params
                    .worker
                    .send(worker::GetAddresses(slf.db.clone(), wallet, offset, limit))
                    .flatten()
                    .map_err(From::from)
                    .into_actor(slf)
            },
        );

        Box::new(f)
    }

    /// Run a RADRequest and return the computed result.
    pub fn run_rad_request(&self, req: types::RADRequest) -> ResponseFuture<types::RadonTypes> {
        let f = self
            .params
            .worker
            .send(worker::RunRadRequest(req))
            .flatten()
            .map_err(From::from);

        Box::new(f)
    }

    /// Generate a random BIP39 mnemonics sentence
    pub fn generate_mnemonics(&self, length: types::MnemonicLength) -> ResponseFuture<String> {
        let f = self
            .params
            .worker
            .send(worker::GenMnemonic(length))
            .map_err(From::from);

        Box::new(f)
    }

    /// Forward a Json-RPC call to the node.
    pub fn forward(
        &mut self,
        method: String,
        params: types::RpcParams,
    ) -> ResponseFuture<types::Json> {
        match &self.params.client {
            Some(addr) => {
                let req = types::RpcRequest::method(method)
                    .timeout(self.params.requests_timeout)
                    .params(params)
                    .expect("params failed serialization");
                let f = addr.send(req).flatten().map_err(From::from);

                Box::new(f)
            }
            None => {
                let f = future::err(Error::NodeNotConnected);

                Box::new(f)
            }
        }
    }

    /// Get public info of all the wallets stored in the database.
    pub fn wallet_infos(&self) -> ResponseFuture<Vec<model::Wallet>> {
        let f = self
            .params
            .worker
            .send(worker::WalletInfos(self.db.clone()))
            .flatten()
            .map_err(From::from);

        Box::new(f)
    }

    /// Create an empty HD Wallet.
    pub fn create_wallet(
        &self,
        password: types::Password,
        seed_source: types::SeedSource,
        name: Option<String>,
        caption: Option<String>,
    ) -> ResponseFuture<String> {
        let f = self
            .params
            .worker
            .send(worker::CreateWallet(
                self.db.clone(),
                name,
                caption,
                password,
                seed_source,
            ))
            .flatten()
            .map_err(From::from);

        Box::new(f)
    }

    /// Lock a wallet, that is, remove its encryption/decryption key from the list of known keys and
    /// close the session.
    ///
    /// This means the state of this wallet won't be updated with information received from the
    /// node.
    pub fn lock_wallet(&mut self, session_id: String, wallet_id: String) -> Result<()> {
        self.state.remove_wallet(&session_id, &wallet_id)
    }

    /// Load a wallet's private information and keys in memory.
    pub fn unlock_wallet(
        &self,
        wallet_id: String,
        password: types::Password,
    ) -> ResponseActFuture<model::WalletUnlocked> {
        let f = self
            .params
            .worker
            .send(worker::UnlockWallet(self.db.clone(), wallet_id, password))
            .flatten()
            .map_err(|err| match err {
                worker::Error::DbKeyNotFound(_) => {
                    validation_error(field_error("wallet_id", "Wallet not found"))
                }
                worker::Error::Cipher(_) => {
                    validation_error(field_error("password", "Wrong password"))
                }
                err => From::from(err),
            })
            .into_actor(self)
            .and_then(|wallet: model::WalletUnlocked, slf: &mut Self, _| {
                fut::result(
                    slf.state
                        .insert_wallet(wallet.session_id.clone(), wallet.clone()),
                )
                .map(move |(), _, _| wallet)
            });

        Box::new(f)
    }

    /// Perform all the tasks needed to properly stop the application.
    pub fn stop(&self) -> ResponseFuture<()> {
        let fut = self
            .params
            .worker
            .send(worker::FlushDb(self.db.clone()))
            .map_err(internal_error)
            .and_then(|result| result.map_err(internal_error));

        Box::new(fut)
    }

    /// Return a timer function that can be scheduled to expire the session after the configured time.
    pub fn set_session_to_expire(&self, session_id: String) -> TimerFunc<Self> {
        log::debug!(
            "Session {} will expire in {} seconds.",
            &session_id,
            self.params.session_expires_in.as_secs()
        );

        TimerFunc::new(
            self.params.session_expires_in,
            move |slf: &mut Self, _ctx| match slf.close_session(session_id.clone()) {
                Ok(_) => log::info!("Session {} closed", session_id),
                Err(err) => log::error!("Session {} couldn't be closed: {}", session_id, err),
            },
        )
    }

    /// Remove a session from the list of active sessions.
    pub fn close_session(&mut self, session_id: String) -> Result<()> {
        self.state.remove_session(&session_id)
    }

    /// Handle notifications received from the node.
    pub fn handle_block_notification(&mut self, value: types::Json) {
        match serde_json::from_value::<types::ChainBlock>(value) {
            Ok(_block) => {
                // TODO: implement
            }
            Err(e) => log::error!("Couldn't parse received block: {}", e),
        }
    }
}
