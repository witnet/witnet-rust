use actix::prelude::*;

use crate::actors::app;

pub struct Stop;

impl Message for Stop {
    type Result = app::Result<()>;
}

impl Handler<Stop> for app::App {
    type Result = app::ResponseFuture<()>;

    fn handle(&mut self, _msg: Stop, _ctx: &mut Self::Context) -> Self::Result {
        log::info!("stopping application...");
        self.stop()
    }
}
