//! # Controller actor
//!
//! The Controller actor holds the address of the App actor and the instance of the Websockets
//! server, and is in charge of graceful shutdown of the entire system.  See `Controller` struct for
//! more info.

use actix::prelude::*;

use witnet_net::server::ws::Server;

use crate::actors::App;

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
    /// Start actor.
    pub fn start(server: Server, app: Addr<App>) -> Addr<Self> {
        let slf = Self {
            server: Some(server),
            app,
        };

        slf.start()
    }

    /// Stop websockets server.
    fn stop_server(&mut self) {
        drop(self.server.take())
    }
}

impl Actor for Controller {
    type Context = Context<Self>;
}
