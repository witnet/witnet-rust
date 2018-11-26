use actix::{Actor, Context, Supervised, SystemService};

use super::MempoolManager;

/// Implement Actor trait for `MempoolManager`
impl Actor for MempoolManager {
    /// Every actor has to provide execution `Context` in which it can run
    type Context = Context<Self>;
}

/// Make the MempoolManager a Supervisor, which provides the ability to be restarted
impl Supervised for MempoolManager {}

/// Required trait for being able to retrieve MempoolManager address from registry
impl SystemService for MempoolManager {}
