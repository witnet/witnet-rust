use actix::prelude::*;

use crate::actors::controller;

pub struct Shutdown;

impl Message for Shutdown {
    type Result = ();
}

impl Handler<Shutdown> for controller::Controller {
    type Result = ();

    fn handle(&mut self, _msg: Shutdown, ctx: &mut Self::Context) -> Self::Result {
        self.shutdown(ctx)
    }
}
