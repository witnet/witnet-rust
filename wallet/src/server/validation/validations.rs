use std::path;

use super::*;

impl requests::CreateMnemonics {
    pub fn validate(self) -> Result<types::MnemonicLength> {
        match self.length {
            12 => Ok(types::MnemonicLength::Words12),
            15 => Ok(types::MnemonicLength::Words15),
            18 => Ok(types::MnemonicLength::Words18),
            21 => Ok(types::MnemonicLength::Words21),
            24 => Ok(types::MnemonicLength::Words24),
            _ => Err(error(
                "length",
                "Invalid Mnemonics Length. Must be 12, 15, 18, 21 or 24",
            )),
        }
    }
}

impl requests::CreateWallet {
    pub fn validate(self, db_path: &path::Path) -> Result<types::CreateWallet> {
        let name = self.name;
        let caption = self.caption;
        let db_filename = db_path.join(&format!("{}.sqlite3", &name));
        let db_url = match db_filename.exists() {
            true => Err(error(
                "name",
                format!(
                    "A wallet database with that name '{}' already exists",
                    &name
                ),
            )),
            false => db_filename.to_str().map(|s| s.to_string()).ok_or_else(|| {
                error(
                    "name",
                    format!(
                        "Wallet database name and/or path '{}' contains unsupported characters.",
                        db_filename.to_string_lossy()
                    ),
                )
            }),
        };

        let seed_source = match self.seed_source.as_ref() {
            "xprv" => Ok(types::SeedSource::Xprv(self.seed_data)),
            "mnemonics" => types::Mnemonic::from_phrase_ref(self.seed_data.as_ref())
                .map_err(|err| error("seed_source", format!("{}", err)))
                .map(types::SeedSource::Mnemonic),
            _ => Err(error(
                "seed_source",
                "Seed source has to be 'mnemonics' or 'xprv'.",
            )),
        };

        let password = if <str>::len(self.password.as_ref()) < 8 {
            Err(error(
                "password",
                "Password must have at least 8 characters",
            ))
        } else {
            Ok(self.password)
        };

        join3_errors(
            db_url,
            seed_source,
            password,
            |db_url, seed_source, password| types::CreateWallet {
                name,
                caption,
                db_url,
                seed_source,
                password,
            },
        )
    }
}
