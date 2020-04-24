use super::RadManager;
use actix::{Actor, Context, Supervised, SystemService};

/// Implement Actor trait for `RadManager`
impl Actor for RadManager {
    /// Every actor has to provide execution `Context` in which it can run
    type Context = Context<Self>;

    /// Method to be executed when the actor is started
    fn started(&mut self, _ctx: &mut Self::Context) {
        log::debug!("RadManager actor has been started!");
    }
}

impl Supervised for RadManager {}

impl SystemService for RadManager {}
