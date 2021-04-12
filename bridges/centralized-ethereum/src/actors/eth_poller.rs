use actix::prelude::*;

/// EthPoller (TODO: Explanation)
#[derive(Default)]
pub struct EthPoller;

/// Make actor from EthPoller
impl Actor for EthPoller {
    /// Every actor has to provide execution Context in which it can run.
    type Context = Context<Self>;

    /// Method to be executed when the actor is started
    fn started(&mut self, _ctx: &mut Self::Context) {
        log::debug!("EthPoller actor has been started!");
    }
}

/// Required trait for being able to retrieve EthPoller address from system registry
impl actix::Supervised for EthPoller {}

/// Required trait for being able to retrieve EthPoller address from system registry
impl SystemService for EthPoller {}
