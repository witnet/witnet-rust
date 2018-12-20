use actix::{Actor, Context};

use crate::actors::config_manager::send_get_config_request;

use super::MiningManager;
use witnet_util::timestamp::get_timestamp;

/// Make actor from MiningManager
impl Actor for MiningManager {
    /// Every actor has to provide execution Context in which it can run.
    type Context = Context<Self>;

    /// Method to be executed when the actor is started
    fn started(&mut self, ctx: &mut Self::Context) {
        // Use the current timestamp as a random value to modify the signature
        // Wait at least 1 second before starting each node
        self.random = get_timestamp() as u64;
        send_get_config_request(self, ctx, Self::process_config)
    }
}
