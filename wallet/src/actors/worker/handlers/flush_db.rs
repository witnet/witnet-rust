use actix::prelude::*;

use crate::actors::worker;

pub struct FlushDb;

impl Message for FlushDb {
    type Result = worker::Result<()>;
}

impl Handler<FlushDb> for worker::Worker {
    type Result = <FlushDb as Message>::Result;

    fn handle(&mut self, _msg: FlushDb, _ctx: &mut Self::Context) -> Self::Result {
        self.flush_db()
    }
}
