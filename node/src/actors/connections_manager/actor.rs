use actix::{Actor, Context};

use super::ConnectionsManager;

/// Make actor from ConnectionsManager
impl Actor for ConnectionsManager {
    /// Every actor has to provide execution `Context` in which it can run.
    type Context = Context<Self>;

    /// Method to be executed when the actor is started
    fn started(&mut self, ctx: &mut Self::Context) {
        log::debug!("Connections Manager actor has been started!");

        // Start server
        // FIXME(#72): decide what to do with actor when server cannot be started
        self.start_server(ctx);
    }
}
