use std::convert::TryFrom;
use std::{
    collections::HashMap,
    sync::{Arc, Mutex, RwLock},
};

use witnet_net::client::tcp::jsonrpc::Subscribe;

use crate::types::SubscriptionId;

use super::*;

/// Struct to manage the App actor state and its invariants.
#[derive(Default)]
pub struct State {
    pub node_subscriptions: Arc<Mutex<HashMap<String, Subscribe>>>,
    pub client_subscriptions: HashMap<types::SessionId, types::DynamicSink>,
    pub sessions: HashMap<types::SessionId, Session>,
    pub wallets: HashMap<String, types::SessionWallet>,
}

#[derive(Default)]
pub struct Session {
    wallets: HashMap<String, types::SessionWallet>,
}

impl State {
    /// Get the subscription sink for a specific session
    pub fn get_sink(&mut self, session_id: &types::SessionId) -> types::DynamicSink {
        match self.client_subscriptions.get(session_id) {
            Some(sink) => sink.clone(),
            None => self.set_sink(session_id, None),
        }
    }

    /// Set the subscription sink for a specific session
    fn set_sink(
        &mut self,
        session_id: &types::SessionId,
        new_sink: Option<types::Sink>,
    ) -> types::DynamicSink {
        let sink = Arc::new(RwLock::new(new_sink));
        self.client_subscriptions
            .insert(session_id.clone(), sink.clone());

        sink
    }

    /// Updates the subscription sink for a specific session
    pub fn update_sink(
        &mut self,
        session_id: &types::SessionId,
        new_sink: Option<types::Sink>,
    ) -> types::DynamicSink {
        match self.client_subscriptions.get(session_id) {
            Some(sink) => {
                let mut lock = sink
                    .write()
                    .expect("Write locks should only fail if poisoned");
                *lock = new_sink;
                sink.clone()
            }
            None => self.set_sink(session_id, new_sink),
        }
    }

    /// Get all wallets for a session
    pub fn get_wallets_by_session(
        &self,
        session_id: &types::SessionId,
    ) -> Result<&HashMap<String, types::SessionWallet>> {
        Ok(&self
            .sessions
            .get(session_id)
            .ok_or_else(|| Error::SessionNotFound)?
            .wallets)
    }

    /// Get a reference to an unlocked wallet.
    pub fn get_wallet_by_session_and_id(
        &self,
        session_id: &types::SessionId,
        wallet_id: &str,
    ) -> Result<types::SessionWallet> {
        let wallets = self.get_wallets_by_session(&session_id)?;

        let wallet = wallets
            .get(wallet_id)
            .cloned()
            .ok_or_else(|| Error::WalletNotFound)?;

        Ok(wallet)
    }

    /// Check if the session is still active.
    pub fn is_session_active(&self, session_id: &types::SessionId) -> bool {
        self.sessions.contains_key(session_id)
    }

    /// Add a sink and subscription id to a session.
    pub fn subscribe(
        &mut self,
        session_id: &types::SessionId,
        sink: types::Sink,
    ) -> Result<types::DynamicSink> {
        match self.sessions.get_mut(session_id) {
            Some(_) => Ok(self.update_sink(session_id, Some(sink))),
            None => Err(Error::SessionNotFound),
        }
    }

    /// Remove a subscription sink from a session.
    pub fn unsubscribe(&mut self, subscription_id: &types::SubscriptionId) -> Result<()> {
        // Session id and subscription id are currently the same thing.
        let session_id = types::SessionId::try_from(subscription_id)?;
        match self.sessions.get_mut(&session_id) {
            Some(_) => {
                self.update_sink(&session_id, None);
                log::debug!("Desubscribed subscription {}", session_id);

                Ok(())
            }
            None => Err(Error::SessionNotFound),
        }
    }

    /// Remove a session but keep its wallets.
    pub fn remove_session(&mut self, session_id: &types::SessionId) -> Result<()> {
        let subscription_id = SubscriptionId::from(session_id);
        self.unsubscribe(&subscription_id).map(|_| ())?;
        self.sessions
            .remove(session_id)
            .map(|_| ())
            .ok_or_else(|| Error::SessionNotFound)
    }

    /// Remove a wallet completely.
    pub fn remove_wallet(&mut self, session_id: &types::SessionId, wallet_id: &str) -> Result<()> {
        let session = self
            .sessions
            .get_mut(session_id)
            .ok_or_else(|| Error::SessionNotFound)?;

        session.wallets.remove(wallet_id);
        self.wallets.remove(wallet_id);

        Ok(())
    }

    /// Insert a new wallet into the state of the session if it is not already present.
    pub fn create_session(
        &mut self,
        session_id: types::SessionId,
        wallet_id: String,
        wallet: types::SessionWallet,
    ) {
        let entry = self.sessions.entry(session_id);
        let wallets = &mut entry.or_default().wallets;

        wallets.insert(wallet_id.clone(), wallet.clone());

        self.wallets.insert(wallet_id, wallet);
    }
}
