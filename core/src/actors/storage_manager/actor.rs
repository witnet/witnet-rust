use actix::{Actor, ActorContext, Context};
use log::{debug, error};

use crate::actors::config_manager::send_get_config_request;

use super::StorageManager;

/// Make actor from `StorageManager`
impl Actor for StorageManager {
    /// Every actor has to provide execution `Context` in which it can run.
    type Context = Context<Self>;

    /// Method to be executed when the actor is started
    fn started(&mut self, ctx: &mut Self::Context) {
        debug!("Storage Manager actor has been started!");

        // Send message to config manager and process response
        send_get_config_request(self, ctx, |s, ctx, config| {
            // Get db path from configuration
            let db_path = &config.storage.db_path;

            // Override actor
            *s = Self::new(&db_path.to_string_lossy());

            // Stop context if the storage is not properly initialized
            // FIXME(#72): check error handling
            if s.storage.is_none() {
                error!("Error initializing storage");
                ctx.stop();
            }
        });
    }
}
