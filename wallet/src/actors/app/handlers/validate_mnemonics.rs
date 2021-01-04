use actix::prelude::*;
use serde::{Deserialize, Serialize};

use crate::actors::app;
use crate::types;

#[derive(Debug, Serialize, Deserialize)]
pub struct ValidateMnemonicsRequest {
    seed_source: String,
    seed_data: types::Password,
    backup_password: Option<types::Password>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ValidateMnemonicsResponse {
    pub exist: bool,
    pub wallet_id: String,
}

impl Message for ValidateMnemonicsRequest {
    type Result = app::Result<ValidateMnemonicsResponse>;
}

impl Handler<ValidateMnemonicsRequest> for app::App {
    type Result = app::ResponseActFuture<ValidateMnemonicsResponse>;

    fn handle(&mut self, req: ValidateMnemonicsRequest, _ctx: &mut Self::Context) -> Self::Result {
        let f = self.validate_seed(req.seed_source, req.seed_data, req.backup_password);

        Box::pin(f)
    }
}
