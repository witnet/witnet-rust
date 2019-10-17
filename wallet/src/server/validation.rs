use serde::Serialize;

use super::*;

#[derive(Debug, Serialize)]
pub struct ValidationErrors(Vec<(String, String)>);

impl From<Vec<(String, String)>> for ValidationErrors {
    fn from(errors: Vec<(String, String)>) -> Self {
        ValidationErrors(errors)
    }
}

impl requests::CreateMnemonics {
    pub fn validate(&self) -> Result<types::MnemonicLength, ValidationErrors> {
        match self.length {
            12 => Ok(types::MnemonicLength::Words12),
            15 => Ok(types::MnemonicLength::Words15),
            18 => Ok(types::MnemonicLength::Words18),
            21 => Ok(types::MnemonicLength::Words21),
            24 => Ok(types::MnemonicLength::Words24),
            _ => Err(vec![(
                "length".to_string(),
                "Invalid Mnemonics Length. Must be 12, 15, 18, 21 or 24".to_string(),
            )]
            .into()),
        }
    }
}
