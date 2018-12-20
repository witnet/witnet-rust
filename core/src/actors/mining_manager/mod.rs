use actix::{Actor, ActorContext, AsyncContext};

use log::debug;

use witnet_config::config::Config;

use crate::actors::epoch_manager::messages::Subscribe;
use crate::actors::epoch_manager::EpochManager;

use actix::System;

use self::handlers::EveryEpochPayload;

mod actor;
mod handlers;

////////////////////////////////////////////////////////////////////////////////////////
// ACTOR BASIC STRUCTURE
////////////////////////////////////////////////////////////////////////////////////////
/// MiningManager actor
#[derive(Default)]
pub struct MiningManager {
    // Random value to help with debugging because there is no signature
    // and all the mined blocks have the same hash.
    // This random value helps to distinguish blocks mined on different nodes
    random: u64,
}

/// Auxiliary methods for MiningManager actor
impl MiningManager {
    /// Method to process the configuration received from the config manager
    fn process_config(&mut self, ctx: &mut <Self as Actor>::Context, config: &Config) {
        let enabled = config.mining.enabled;

        // Do not start the MiningManager if the configuration disables it
        if !enabled {
            debug!("MiningManager explicitly disabled by configuration.");
            ctx.stop();
            return;
        }

        debug!("MiningManager actor has been started!");

        // Subscribe to epoch manager
        // Get EpochManager address from registry
        let epoch_manager_addr = System::current().registry().get::<EpochManager>();

        // Subscribe to all epochs with an EveryEpochPayload
        epoch_manager_addr.do_send(Subscribe::to_all(ctx.address(), EveryEpochPayload));
    }
}
