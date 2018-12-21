use actix::{Actor, Context};

use crate::actors::config_manager::send_get_config_request;

use super::MiningManager;

/// Make actor from MiningManager
impl Actor for MiningManager {
    /// Every actor has to provide execution Context in which it can run.
    type Context = Context<Self>;

    /// Method to be executed when the actor is started
    fn started(&mut self, ctx: &mut Self::Context) {
        send_get_config_request(self, ctx, Self::process_config)
    }
}
