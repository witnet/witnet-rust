use actix::prelude::*;

use crate::actors::{app, Controller};

/// Graceful shutdown of the wallet actor-system.
pub struct Shutdown;

impl Message for Shutdown {
    type Result = ();
}

impl Handler<Shutdown> for Controller {
    type Result = ();

    fn handle(&mut self, _msg: Shutdown, ctx: &mut Self::Context) -> Self::Result {
        self.stop_server();
        self.app
            .send(app::Stop)
            .map_err(|_| log::error!("couldn't stop application"))
            .and_then(|_| {
                log::info!("shutting down system!");
                System::current().stop();
                Ok(())
            })
            .into_actor(self)
            .spawn(ctx);
    }
}
