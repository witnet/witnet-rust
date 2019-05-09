//! # Application actor.
//!
//! See [`App`](App) actor for more information.

use std::path::PathBuf;

use actix::prelude::*;

use super::storage::Storage;

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
