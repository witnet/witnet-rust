use actix::prelude::*;
use serde::{Deserialize, Serialize};

use crate::actors::app;

#[derive(Debug, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ImportSeedRequest {
    Mnemonics { mnemonics: String },
    Seed { seed: String },
}

impl Message for ImportSeedRequest {
    type Result = app::Result<()>;
}

impl Handler<ImportSeedRequest> for app::App {
    type Result = <ImportSeedRequest as Message>::Result;

    fn handle(&mut self, _msg: ImportSeedRequest, _ctx: &mut Self::Context) -> Self::Result {
        Ok(())
    }
}
