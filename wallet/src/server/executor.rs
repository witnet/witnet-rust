use actix::prelude::*;

use super::*;

pub struct Executor {
    state: state::State,
}

impl Actor for Executor {
    type Context = SyncContext<Self>;
}

impl Supervised for Executor {}

impl Executor {
    pub fn new(db: db::Database) -> Self {
        let state = state::State { db };

        Self { state }
    }

    pub fn state(&self) -> &state::State {
        &self.state
    }
}
