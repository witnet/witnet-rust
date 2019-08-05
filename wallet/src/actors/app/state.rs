use std::collections::{HashMap, HashSet};

use super::*;

/// Struct to manage the App actor state and its invariants.
#[derive(Default)]
pub struct State {
    sessions: HashMap<String, Session>,
    wallets: HashMap<String, types::Wallet>,
}

#[derive(Default)]
struct Session {
    wallets: HashSet<String>,
    subscriptions: HashMap<types::SubscriptionId, types::Sink>,
}

impl State {
    /// Get a copy of an unlocked wallet suitable for operations with the external keychain.
    pub fn external_wallet(
        &self,
        session_id: &str,
        wallet_id: &str,
    ) -> Result<types::ExternalWallet> {
        let session = self
            .sessions
            .get(session_id)
            .ok_or_else(|| Error::SessionNotFound)?;

        let wallet_id = session
            .wallets
            .get(wallet_id)
            .ok_or_else(|| Error::WalletNotFound)?;

        let wallet = self
            .wallets
            .get(wallet_id)
            .ok_or_else(|| Error::State("session wallet not found"))?;

        Ok(types::ExternalWallet {
            id: wallet.id.clone(),
            enc_key: wallet.enc_key.clone(),
            iv: wallet.iv.clone(),
            account_index: wallet.account_index,
            account_external: wallet.account_external.clone(),
            mutex: wallet.mutex.clone(),
        })
    }

    /// Check if the session is still active.
    pub fn is_session_active(&self, session_id: &str) -> bool {
        self.sessions.contains_key(session_id)
    }

    /// Add a sink and subscription id to a session.
    pub fn subscribe(
        &mut self,
        session_id: &str,
        subscription_id: types::SubscriptionId,
        sink: types::Sink,
    ) -> Result<()> {
        match self.sessions.get_mut(session_id) {
            Some(session) => {
                session.subscriptions.insert(subscription_id, sink);
                Ok(())
            }
            None => Err(Error::SessionNotFound),
        }
    }

    /// Remove a subscription sink from a session.
    pub fn unsubscribe(&mut self, subscription_id: &types::SubscriptionId) -> Result<()> {
        // Session id and subscription id are currently the same thing.
        let session_id_opt = match subscription_id {
            types::SubscriptionId::String(session_id) => Some(session_id),
            _ => None,
        };

        session_id_opt
            .and_then(|session_id| self.sessions.get_mut(session_id))
            .map(|session| {
                session.subscriptions.remove(subscription_id);
            })
            .ok_or_else(|| Error::SessionNotFound)
    }

    /// Remove a session but keep its wallets.
    pub fn remove_session(&mut self, session_id: &str) -> Result<()> {
        self.sessions
            .remove(session_id)
            .map(|_| ())
            .ok_or_else(|| Error::SessionNotFound)
    }

    /// Remove a wallet completely.
    pub fn remove_wallet(&mut self, session_id: &str, wallet_id: &str) -> Result<()> {
        let session = self
            .sessions
            .get_mut(session_id)
            .ok_or_else(|| Error::SessionNotFound)?;

        session.wallets.remove(wallet_id);
        self.wallets.remove(wallet_id);

        Ok(())
    }

    /// Insert a new wallet into the state if it is not already present.
    pub fn insert_wallet(&mut self, session_id: String, wallet: types::Wallet) -> Result<()> {
        let entry = self.sessions.entry(session_id.clone());
        let wallets = &mut entry.or_default().wallets;

        if wallets.contains(&wallet.id) {
            Err(Error::WalletAlreadyUnlocked)
        } else {
            wallets.insert(wallet.id.clone());
            self.wallets.insert(wallet.id.clone(), wallet);

            Ok(())
        }
    }

    // FIXME: Implement as Iterator
    /// Return an Iterator over the unlocked wallets.
    pub fn wallets(&self) -> Vec<types::SimpleWallet> {
        self.wallets.values().map(From::from).collect()
    }
}
