use std::collections::HashMap;

use super::*;

/// Struct to manage the App actor state and its invariants.
#[derive(Default)]
pub struct State {
    sessions: HashMap<types::SessionId, Session>,
    wallets: HashMap<String, types::SessionWallet>,
}

#[derive(Default)]
struct Session {
    wallets: HashMap<String, types::SessionWallet>,
    subscription: Option<types::Sink>,
}

impl State {
    /// Get the sink for a specific session
    pub fn get_sink(&self, session_id: &types::SessionId) -> Option<types::Sink> {
        self.sessions
            .get(session_id)
            .map(|session| session.subscription.clone())
            .flatten()
    }

    /// Get a reference to an unlocked wallet.
    pub fn wallet(
        &self,
        session_id: &types::SessionId,
        wallet_id: &str,
    ) -> Result<types::SessionWallet> {
        let session = self
            .sessions
            .get(session_id)
            .ok_or_else(|| Error::SessionNotFound)?;

        let wallet = session
            .wallets
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
    pub fn subscribe(&mut self, session_id: &types::SessionId, sink: types::Sink) -> Result<()> {
        match self.sessions.get_mut(session_id) {
            Some(session) => {
                session.subscription = Some(sink);
                Ok(())
            }
            None => Err(Error::SessionNotFound),
        }
    }

    /// Remove a subscription sink from a session.
    pub fn unsubscribe(&mut self, subscription_id: &types::SubscriptionId) -> Result<()> {
        // Session id and subscription id are currently the same thing.
        let session_id_opt = match subscription_id {
            types::SubscriptionId::String(id) => Some(types::SessionId::from(id.clone())),
            _ => None,
        };

        session_id_opt
            .and_then(|session_id| self.sessions.get_mut(&session_id))
            .map(|session| {
                session.subscription = None;
            })
            .ok_or_else(|| Error::SessionNotFound)
    }

    /// Remove a session but keep its wallets.
    pub fn remove_session(&mut self, session_id: &types::SessionId) -> Result<()> {
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

    /// Return an Iterator over the unlocked wallets.
    pub fn wallets(&self) -> impl Iterator<Item = (&String, &types::SessionWallet)> {
        self.wallets.iter()
    }
}
