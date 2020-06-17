use actix::prelude::*;
use serde::{Deserialize, Serialize};

use crate::actors::app;
use crate::types;

pub struct Shutdown;

impl Message for Shutdown {
    type Result = ();
}

impl Handler<Shutdown> for app::App {
    type Result = ();

    fn handle(&mut self, _msg: Shutdown, ctx: &mut Self::Context) -> Self::Result {
        self.stop(ctx);
    }
}

