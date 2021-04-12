use actix::prelude::*;

/// EthPoller (TODO: Explanation)
#[derive(Default)]
pub struct DrDatabase;

/// Make actor from DrDatabase
impl Actor for DrDatabase {
    /// Every actor has to provide execution Context in which it can run.
    type Context = Context<Self>;

    /// Method to be executed when the actor is started
    fn started(&mut self, _ctx: &mut Self::Context) {
        log::debug!("DrReporter actor has been started!");
    }
}

/// Required trait for being able to retrieve DrDatabase address from system registry
impl actix::Supervised for DrDatabase {}

/// Required trait for being able to retrieve DrDatabase address from system registry
impl SystemService for DrDatabase {}
