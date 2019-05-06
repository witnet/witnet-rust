/// TODO: doc
use actix::prelude::*;

use super::storage::Storage;

mod handlers;

pub use handlers::*;

/// TODO: doc
pub struct App {
    storage: Addr<Storage>,
}

impl App {
    /// TODO: doc
    pub fn new(storage: Addr<Storage>) -> Self {
        Self { storage }
    }
}

impl Actor for App {
    type Context = Context<Self>;
}
