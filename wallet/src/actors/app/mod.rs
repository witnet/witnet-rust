//! # Application actor.
//!
//! See [`App`](App) actor for more information.

use std::path::PathBuf;

use actix::prelude::*;

use super::storage::Storage;
use witnet_net::server::ws::actors::controller;

mod handlers;

pub use handlers::*;

/// Application actor.
///
/// The application actor is in charge of managing the state of the application and coordinating the
/// service actors, e.g.: storage, node client, and so on.
pub struct App {
    storage: Addr<Storage>,
}

impl App {
    /// Returns an [`AppBuilder`](AppBuilder) instance.
    ///
    /// Use this instance to start/configure the [`App`](App) actor.
    pub fn build() -> AppBuilder {
        AppBuilder::default()
    }
}

impl Actor for App {
    type Context = Context<Self>;

    fn started(&mut self, ctx: &mut Self::Context) {
        controller::Controller::from_registry()
            .do_send(controller::Subscribe(ctx.address().recipient()));
    }
}

/// [`App`](App) builder used to set optional parameters using the builder-pattern.
#[derive(Default)]
pub struct AppBuilder;

impl AppBuilder {
    /// Start the [`App`](App) actor and its service actors, e.g.: storage, node client, and so on.
    pub fn start(self, db_path: PathBuf) -> Addr<App> {
        let storage = Storage::start(db_path);

        let app = App { storage };

        app.start()
    }
}

impl Handler<controller::Shutdown> for App {
    type Result = <controller::Shutdown as Message>::Result;

    fn handle(&mut self, msg: controller::Shutdown, ctx: &mut Self::Context) -> Self::Result {
        if msg.timeout.is_some() {
            ctx.stop();
            log::debug!("App actor stopped.");
        } else {
            ctx.terminate();
            log::debug!("App actor terminated.");
        }
        Ok(())
    }
}
