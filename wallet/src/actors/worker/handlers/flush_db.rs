use std::sync::Arc;

use actix::prelude::*;

use crate::actors::worker;

pub struct FlushDb(pub Arc<rocksdb::DB>);

impl Message for FlushDb {
    type Result = worker::Result<()>;
}

impl Handler<FlushDb> for worker::Worker {
    type Result = <FlushDb as Message>::Result;

    fn handle(&mut self, FlushDb(db): FlushDb, _ctx: &mut Self::Context) -> Self::Result {
        self.flush_db(&worker::Db::new(db.as_ref()))
    }
}
