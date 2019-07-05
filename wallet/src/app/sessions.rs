use std::time::Duration;

use jsonrpc_pubsub as pubsub;

use super::*;

#[derive(Default)]
pub struct Sessions {
    sessions: HashMap<app::SessionId, wallet::WalletId>,
    // wallet_keys: HashMap<wallet::WalletId, Arc<wallet::Key>>,
    last_subscription_id: u64,
}

impl Sessions {
    /// Return a fresh subscription id
    pub fn next_subscription_id(&mut self) -> pubsub::SubscriptionId {
        self.last_subscription_id = self.last_subscription_id.wrapping_add(1);

        pubsub::SubscriptionId::Number(self.last_subscription_id)
    }

    /// Remove a session id and return its associated wallet.
    pub fn lock(&mut self, wallet_id: &wallet::WalletId, session_id: &app::SessionId) {
        // self.sessions.remove(session_id)
        unimplemented!()
    }
}
