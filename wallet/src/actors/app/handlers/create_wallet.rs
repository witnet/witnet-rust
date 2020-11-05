use actix::prelude::*;
use serde::{Deserialize, Serialize};
use std::str;


use witnet_crypto::mnemonic::Mnemonic;

use crate::actors::app;
use crate::types;

#[derive(Debug, Serialize, Deserialize)]
pub struct CreateWalletRequest {
    name: Option<String>,
    description: Option<String>,
    password: types::Password,
    seed_source: String,
    seed_data: types::Password,
    overwrite: Option<bool>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct CreateWalletResponse {
    pub wallet_id: String,
}

impl Message for CreateWalletRequest {
    type Result = app::Result<CreateWalletResponse>;
}

impl Handler<CreateWalletRequest> for app::App {
    type Result = app::ResponseActFuture<CreateWalletResponse>;

    fn handle(&mut self, req: CreateWalletRequest, _ctx: &mut Self::Context) -> Self::Result {
        let validated_params = validate(req).map_err(app::validation_error);
        log::error!("I passed the validation");

        let f = fut::result(validated_params).and_then(|params, slf: &mut Self, _ctx| {
            slf.create_wallet(
                params.password,
                params.seed_source,
                params.name,
                params.description,
                params.overwrite,
            )
            .map(|wallet_id| CreateWalletResponse { wallet_id })
            .into_actor(slf)
        });

        Box::new(f)
    }
}

struct Validated {
    pub description: Option<String>,
    pub name: Option<String>,
    pub overwrite: bool,
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
    let description = req.description;
    let seed_data = req.seed_data;
    let source = match req.seed_source.as_ref() {
        "xprv" => {
                let ref_seed: &[u8] = seed_data.as_ref();
                let seed_data_string: String = str::from_utf8(ref_seed).expect("wrong").to_string();
                let ocurrences: Vec<(usize, &str)> = seed_data_string.match_indices("xprv").collect();
                log::error!("Here with ocurrences {:?}", ocurrences);

                match ocurrences.len() {
                    1 => Ok(types::SeedSource::Xprv(seed_data)),
                    2 => {
                        let (internal, external) = seed_data_string.split_at(ocurrences[1].0);
                        log::error!("Here with external {:?} and internal {:?}", external, internal);

                        Ok(types::SeedSource::XprvKeychain((internal.into(), external.into())))
                    },
                    _ => Ok(types::SeedSource::Xprv(seed_data)),
                }
        },
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
    let overwrite = req.overwrite.unwrap_or(false);

    app::combine_field_errors(source, password, move |seed_source, password| Validated {
        description,
        name,
        overwrite,
        password,
        seed_source,
    })
}
