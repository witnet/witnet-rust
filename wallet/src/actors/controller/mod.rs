use actix::prelude::*;

use witnet_net::server::ws::Server;

use crate::actors::{app, App};

mod handlers;

pub use handlers::*;

/// Controller actor.
///
/// When `Shutdown` message is received, it will first stop the websockets server, then send a
/// `Stop` message to the App actor and if successful it will then stop the actor system.
pub struct Controller {
    server: Option<Server>,
    app: Addr<App>,
}

impl Controller {
    pub fn start(server: Server, app: Addr<App>) -> Addr<Self> {
        let actor = Self {
            app,
            server: Some(server),
        };

        actor.start()
    }

    fn stop_server(&mut self) {
        drop(self.server.take())
    }

    fn shutdown(&mut self, ctx: &mut <Self as Actor>::Context) {
        self.stop_server();
        self.app
            .send(app::Stop)
            .map_err(|_| log::error!("Couldn't stop application!"))
            .and_then(|_| {
                log::info!("Application stopped. Shutting down system!");
                System::current().stop();
                Ok(())
            })
            .into_actor(self)
            .spawn(ctx);
    }
}

impl Actor for Controller {
    type Context = Context<Self>;
}
