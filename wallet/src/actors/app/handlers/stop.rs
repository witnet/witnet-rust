use actix::prelude::*;

use crate::actors::app::App;

pub struct Stop;

impl Message for Stop {
    type Result = Result<(), failure::Error>;
}

impl Handler<Stop> for App {
    type Result = ResponseFuture<(), failure::Error>;

    fn handle(&mut self, _msg: Stop, _ctx: &mut Self::Context) -> Self::Result {
        log::info!("stopping application...");
        self.stop()
    }
}
