use actix::prelude::*;
use serde::Deserialize;

use witnet_crypto::mnemonic::Mnemonic;

use crate::actors::app;
use crate::types;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateWalletRequest {
    pub name: Option<String>,
    pub caption: Option<String>,
    pub password: types::Password,
    pub seed_source: String,
    pub seed_data: types::Password,
}

impl Message for CreateWalletRequest {
    type Result = app::Result<()>;
}

impl Handler<CreateWalletRequest> for app::App {
    type Result = app::ResponseActFuture<()>;

    fn handle(&mut self, req: CreateWalletRequest, _ctx: &mut Self::Context) -> Self::Result {
        let validated_params = validate(req).map_err(app::validation_error);

        let f = fut::result(validated_params).and_then(|params, slf: &mut Self, _ctx| {
            slf.create_wallet(
                params.password,
                params.seed_source,
                params.name,
                params.caption,
            )
            .into_actor(slf)
        });

        Box::new(f)
    }
}

struct Validated {
    pub name: Option<String>,
    pub caption: Option<String>,
    pub password: types::Password,
    pub seed_source: types::SeedSource,
}

/// Validate `CreateWalletRequest`.
///
/// To be valid it must pass these checks:
/// - password is at least 8 characters
/// - seed_sources has to be `mnemonics | xprv`
fn validate(req: CreateWalletRequest) -> Result<Validated, app::ValidationErrors> {
    let name = req.name;
    let caption = req.caption;
    let seed_data = req.seed_data;
    let source = match req.seed_source.as_ref() {
        "xprv" => Ok(types::SeedSource::Xprv),
        "mnemonics" => Mnemonic::from_phrase(seed_data)
            .map_err(|err| app::field_error("seed_data", format!("{}", err)))
            .map(types::SeedSource::Mnemonics),
        _ => Err(app::field_error(
            "seed_source",
            "Seed source has to be mnemonics|xprv.",
        )),
    };
    let password = if <str>::len(req.password.as_ref()) < 8 {
        Err(app::field_error(
            "password",
            "Password must be at least 8 characters.",
        ))
    } else {
        Ok(req.password)
    };

    app::combine_field_errors(source, password, move |seed_source, password| Validated {
        name,
        caption,
        password,
        seed_source,
    })
}
