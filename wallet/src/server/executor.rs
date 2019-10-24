use std::path;

use actix::prelude::*;

use super::*;

pub struct Executor {
    state: types::State,
}

impl Actor for Executor {
    type Context = SyncContext<Self>;
}

impl Supervised for Executor {}

impl Executor {
    pub fn new(state: types::State) -> Self {
        Self { state }
    }

    pub fn state(&self) -> &types::State {
        &self.state
    }
}
