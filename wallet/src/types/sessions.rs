use std::{
    collections::{hash_map, HashMap},
    sync::Arc,
};

use jsonrpc_pubsub as pubsub;

use witnet_data_structures::chain;

use super::*;

#[derive(Default)]
pub struct Sessions {
    sessions: HashMap<SessionId, Session>,
    // utxos: HashSet<Arc<chain::OutputPointer>>,
    // wallet_utxos: HashMap<WalletId, Arc<chain::OutputPointer>>,
    // pkhs: HashMap<chain::PublicKeyHash, WalletId>,
}

#[derive(Default)]
pub struct Session {
    wallets: HashMap<WalletId, UnlockedWallet>,
    subscriptions: HashMap<SubscriptionId, pubsub::Sink>,
}

impl Sessions {
    pub fn with_session<'a>(&'a mut self, session_id: SessionId) -> Option<SessionEntry<'a>> {
        match self.sessions.entry(session_id) {
            hash_map::Entry::Occupied(entry) => Some(SessionEntry { entry }),
            hash_map::Entry::Vacant(_) => None,
        }
    }

    /// Check if a session with the given id exists.
    pub fn exists(&self, session_id: &SessionId) -> bool {
        self.sessions.contains_key(session_id)
    }

    /// Create a new subscription id for the session.
    pub fn new_subscription_id(&self, session_id: &SessionId) -> SubscriptionId {
        pubsub::SubscriptionId::String(session_id.as_ref().to_owned())
    }

    /// Remove the subcription id that belongs to a session.
    pub fn remove_subscription(
        &mut self,
        subscription_id: &SubscriptionId,
    ) -> Option<pubsub::Sink> {
        let session_id = match subscription_id {
            pubsub::SubscriptionId::String(id) => Some(Arc::new(id.clone())),
            _ => None,
        }?;
        let session = self.sessions.get_mut(&session_id)?;

        session.subscriptions.remove(subscription_id)
    }

    /// Register a new session with the given wallet id and key.
    pub fn register(&mut self, session_id: SessionId, wallet_id: WalletId, wallet: UnlockedWallet) {
        self.sessions
            .entry(session_id)
            .or_default()
            .wallets
            .insert(wallet_id, wallet);
    }
}

pub struct SessionEntry<'a> {
    entry: hash_map::OccupiedEntry<'a, SessionId, Session>,
}

impl<'a> SessionEntry<'a> {
    pub fn with_wallet(&'a mut self, wallet_id: WalletId) -> Option<WalletEntry<'a>> {
        match self.entry.get_mut().wallets.entry(wallet_id) {
            hash_map::Entry::Occupied(entry) => Some(WalletEntry { entry }),
            hash_map::Entry::Vacant(_) => None,
        }
    }

    /// Close the session but do not lock its wallets.
    pub fn close(self) {
        self.entry.remove();
    }

    /// Register a new subscription inside the session.
    pub fn add_subscription(&mut self, subscription_id: SubscriptionId, sink: pubsub::Sink) {
        self.entry
            .get_mut()
            .subscriptions
            .insert(subscription_id, sink);
    }
}

pub struct WalletEntry<'a> {
    entry: hash_map::OccupiedEntry<'a, WalletId, UnlockedWallet>,
}

impl<'a> WalletEntry<'a> {
    /// Lock a wallet and forget its storage secret key.
    pub fn lock(self) {
        self.entry.remove();
    }
}
