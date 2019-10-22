use std::path;

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
    pub fn new(state: state::State) -> Self {
        Self { state }
    }

    pub fn state(&self) -> &state::State {
        &self.state
    }
}
