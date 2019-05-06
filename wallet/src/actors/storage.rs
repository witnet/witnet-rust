/// TODO: doc
use std::path::PathBuf;

use actix::prelude::*;

pub struct Storage {
    db_path: PathBuf,
}

impl Storage {
    /// TODO: doc
    pub fn new(db_path: PathBuf) -> Self {
        Self { db_path }
    }
}

impl Actor for Storage {
    type Context = SyncContext<Self>;
}
