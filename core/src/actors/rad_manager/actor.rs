use super::RadManager;
use actix::{Actor, Context};
use log;

/// Implement Actor trait for `RadManager`
impl Actor for RadManager {
    /// Every actor has to provide execution `Context` in which it can run
    type Context = Context<Self>;

    /// Method to be executed when the actor is started
    fn started(&mut self, _ctx: &mut Self::Context) {
        log::debug!("RadManager actor has been started!");
    }
}
