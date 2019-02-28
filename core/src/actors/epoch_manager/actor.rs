use log::debug;

use actix::{Actor, Context};

use super::EpochManager;

/// Make actor from EpochManager
impl Actor for EpochManager {
    /// Every actor has to provide execution Context in which it can run.
    type Context = Context<Self>;

    /// Method to be executed when the actor is started
    fn started(&mut self, ctx: &mut Self::Context) {
        debug!("Epoch Manager actor has been started!");

        self.process_config(ctx);
    }
}
