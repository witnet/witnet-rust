use actix::prelude::*;
use serde::{Deserialize, Serialize};
use std::str;

use witnet_crypto::mnemonic::Mnemonic;

use crate::actors::app;
use crate::{crypto, types};

#[derive(Debug, Serialize, Deserialize)]
pub struct CreateWalletRequest {
    name: Option<String>,
    description: Option<String>,
    password: types::Password,
    seed_source: String,
    seed_data: types::Password,
    overwrite: Option<bool>,
    backup_password: Option<types::Password>,
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
    let backup_password = req.backup_password;
    let source = match req.seed_source.as_ref() {
        "xprv" => {
            let backup_password = backup_password.ok_or_else(|| {
                app::field_error("backup_password", "Backup password not found for XPRV key")
            })?;
            let seed_data_string = str::from_utf8(seed_data.as_ref()).map_err(|_| {
                app::field_error("seed_data", "Could not convert seed data to XPRV string")
            })?;
            log::error!("Before decoding");
            let (hrp, ciphertext) = bech32::decode(seed_data_string)
                .map_err(|_| app::field_error("seed_data", "Could not decode bench32 key"))?;

            let decrypted_key_string = bech32::FromBase32::from_base32(&ciphertext)
                .map_err(|_| {
                    app::field_error(
                        "seed_data",
                        "Could not convert bech 32 decoded key to u8 array",
                    )
                })
                .and_then(|res: Vec<u8>| {
                    crypto::decrypt_cbc(&res, backup_password.as_ref())
                        .map_err(|_| app::field_error("seed_data", "Could not decrypt seed data"))
                })
                .and_then(|decrypted: Vec<u8>| {
                    str::from_utf8(&decrypted)
                        .map(|str| str.to_string())
                        .map_err(|_| app::field_error("seed_data", "Could not decrypt seed data"))
                })?;
            match hrp.as_str() {
                "xprv" => Ok(types::SeedSource::Xprv(decrypted_key_string.into())),
                "xprvdouble" => {
                    let ocurrences: Vec<(usize, &str)> =
                        decrypted_key_string.match_indices("xprv").collect();
                    // xprvDouble should only have 2 ocurrences
                    if ocurrences.len() != 2 {
                        return Err(app::field_error(
                            "seed_data",
                            "Invalid number of XPRV keys found for xprvDouble type",
                        ));
                    }
                    let (internal, external) = decrypted_key_string.split_at(ocurrences[1].0);

                    Ok(types::SeedSource::XprvDouble((
                        internal.into(),
                        external.into(),
                    )))
                }
                _ => Ok(types::SeedSource::Xprv(seed_data)),
            }
        }
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
