use super::ReputationManager;
use actix::{Actor, Context, Supervised, SystemService};
use log;

impl Actor for ReputationManager {
    type Context = Context<Self>;

    fn started(&mut self, _ctx: &mut Self::Context) {
        log::debug!("Reputation Manager actor has been started!");
    }
}

impl Supervised for ReputationManager {}

impl SystemService for ReputationManager {}
