use witnet_crypto::mnemonic::Mnemonic;

use super::*;
use crate::{app, validation};

/// Validate `CreateWalletRequest`.
///
/// To be valid it must pass these checks:
/// - password is at least 8 characters
/// - seed_sources has to be `mnemonics | xprv`
pub fn validate_create_wallet(
    req: CreateWalletRequest,
) -> Result<app::CreateWallet, validation::Error> {
    let name = req.name;
    let caption = req.caption;
    let seed_data = req.seed_data;
    let source = match req.seed_source.as_ref() {
        "xprv" => Ok(app::SeedSource::Xprv),
        "mnemonics" => Mnemonic::from_phrase(seed_data)
            .map_err(|err| validation::error("seed_data", format!("{}", err)))
            .map(app::SeedSource::Mnemonics),
        _ => Err(validation::error(
            "seed_source",
            "Seed source has to be mnemonics|xprv.",
        )),
    };
    let password = if <str>::len(req.password.as_ref()) < 8 {
        Err(validation::error(
            "password",
            "Password must be at least 8 characters.",
        ))
    } else {
        Ok(req.password)
    };

    validation::combine(source, password, move |seed_source, password| {
        app::CreateWallet {
            name,
            caption,
            password,
            seed_source,
        }
    })
}
