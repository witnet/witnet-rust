use actix::prelude::*;
use serde::{Deserialize, Serialize};

use crate::actors::app;
use crate::types;

#[derive(Debug, Serialize, Deserialize)]
pub struct CreateMnemonicsRequest {
    pub length: u8,
}

#[derive(Debug, Serialize)]
pub struct CreateMnemonicsResponse {
    pub mnemonics: String,
}

impl Message for CreateMnemonicsRequest {
    type Result = app::Result<CreateMnemonicsResponse>;
}

impl Handler<CreateMnemonicsRequest> for app::App {
    type Result = app::ResponseActFuture<CreateMnemonicsResponse>;

    fn handle(&mut self, req: CreateMnemonicsRequest, _ctx: &mut Self::Context) -> Self::Result {
        let result = validate(req).map_err(app::validation_error);
        let f = fut::result(result).and_then(|length, slf: &mut Self, _| {
            slf.generate_mnemonics(length)
                .map(|mnemonics| CreateMnemonicsResponse { mnemonics })
                .into_actor(slf)
        });

        Box::new(f)
    }
}

/// Validate `CreateMnemonics`.
///
/// To be valid it must pass these checks:
/// - length must be 12, 15, 18, 21 or 24
fn validate(req: CreateMnemonicsRequest) -> Result<types::MnemonicLength, app::ValidationErrors> {
    match req.length {
        12 => Ok(types::MnemonicLength::Words12),
        15 => Ok(types::MnemonicLength::Words15),
        18 => Ok(types::MnemonicLength::Words18),
        21 => Ok(types::MnemonicLength::Words21),
        24 => Ok(types::MnemonicLength::Words24),
        _ => Err(app::field_error(
            "length",
            "Invalid Mnemonics Length. Must be 12, 15, 18, 21 or 24",
        )),
    }
}
