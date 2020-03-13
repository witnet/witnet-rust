use actix::prelude::*;
use serde::{Deserialize, Serialize};

use witnet_crypto::mnemonic::Mnemonic;

use crate::actors::app;
use crate::types;

#[derive(Debug, Serialize, Deserialize)]
pub struct ValidateMnemonicsRequest {
    seed_source: String,
    seed_data: types::Password,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ValidateMnemonicsResponse {
    pub valid: bool,
}

impl Message for ValidateMnemonicsRequest {
    type Result = app::Result<ValidateMnemonicsResponse>;
}

impl Handler<ValidateMnemonicsRequest> for app::App {
    type Result = app::ResponseActFuture<ValidateMnemonicsResponse>;

    fn handle(&mut self, req: ValidateMnemonicsRequest, _ctx: &mut Self::Context) -> Self::Result {
        let validated_params = validate(req).is_ok();

        let f = fut::result(Ok(ValidateMnemonicsResponse {
            valid: validated_params,
        }));

        Box::new(f)
    }
}

struct Validated {
    pub seed_source: types::SeedSource,
}

/// Validate `ValidateMnemonicsRequest`.
///
/// To be valid it must pass these checks:
/// - seed_sources has to be `mnemonics | xprv`
fn validate(req: ValidateMnemonicsRequest) -> Result<Validated, app::ValidationErrors> {
    let seed_data = req.seed_data;

    match req.seed_source.as_ref() {
        "xprv" => Ok(Validated {
            seed_source: types::SeedSource::Xprv(seed_data),
        }),
        "mnemonics" => Mnemonic::from_phrase(seed_data)
            .map_err(|err| app::field_error("seed_data", format!("{}", err)))
            .map(|seed_data| Validated {
                seed_source: types::SeedSource::Mnemonics(seed_data),
            }),
        _ => Err(app::field_error(
            "seed_source",
            "Seed source has to be mnemonics|xprv.",
        )),
    }
}
