use std::sync::Arc;

use actix::prelude::*;

use crate::actors::storage::Storage;
use crate::storage::Error;

pub struct Flush(pub Arc<rocksdb::DB>);

impl Message for Flush {
    type Result = Result<(), Error>;
}

impl Handler<Flush> for Storage {
    type Result = <Flush as Message>::Result;

    fn handle(&mut self, Flush(db): Flush, _ctx: &mut Self::Context) -> Self::Result {
        log::info!("flushing storage");
        self.flush(db.as_ref())
    }
}
