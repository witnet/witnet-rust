use actix::prelude::*;

use crate::actors::app::App;
use crate::app;

pub struct Stop;

impl Message for Stop {
    type Result = Result<(), app::Error>;
}

impl Handler<Stop> for App {
    type Result = ResponseFuture<(), app::Error>;

    fn handle(&mut self, _msg: Stop, _ctx: &mut Self::Context) -> Self::Result {
        log::info!("stopping application...");
        self.stop()
    }
}
