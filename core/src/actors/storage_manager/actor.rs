use super::StorageManager;
use crate::config_mngr;
use actix::prelude::*;
use log::{debug, error};

/// Make actor from `StorageManager`
impl Actor for StorageManager {
    /// Every actor has to provide execution `Context` in which it can run.
    type Context = Context<Self>;

    /// Method to be executed when the actor is started
    fn started(&mut self, ctx: &mut Self::Context) {
        debug!("Storage Manager actor has been started!");

        // Send message to config manager and process response
        config_mngr::get()
            .into_actor(self)
            .and_then(|config, actor, ctx| {
                // Get db path from configuration
                let db_path = &config.storage.db_path;

                // Override actor
                *actor = Self::new(&db_path.to_string_lossy());

                // Stop context if the storage is not properly initialized
                // FIXME(#72): check error handling
                if actor.storage.is_none() {
                    error!("Error initializing storage");
                    ctx.stop();
                }

                fut::ok(())
            })
            .map_err(|err, _, _| log::error!("Storage initialization failed: {}", err))
            .wait(ctx);
    }
}
