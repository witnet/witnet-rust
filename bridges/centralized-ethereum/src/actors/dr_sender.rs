use actix::prelude::*;

/// EthPoller (TODO: Explanation)
#[derive(Default)]
pub struct DrSender;

/// Make actor from DrSender
impl Actor for DrSender {
    /// Every actor has to provide execution Context in which it can run.
    type Context = Context<Self>;

    /// Method to be executed when the actor is started
    fn started(&mut self, _ctx: &mut Self::Context) {
        log::debug!("EthPoller actor has been started!");
    }
}

/// Required trait for being able to retrieve DrSender address from system registry
impl actix::Supervised for DrSender {}

/// Required trait for being able to retrieve DrSender address from system registry
impl SystemService for DrSender {}
