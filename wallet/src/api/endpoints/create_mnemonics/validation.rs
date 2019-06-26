use witnet_crypto::mnemonic::Length;

use super::*;
use crate::{app, validation};

/// Validate `CreateMnemonics`.
///
/// To be valid it must pass these checks:
/// - length must be 12, 15, 18, 21 or 24
pub fn validate_create_mnemonics(
    req: CreateMnemonicsRequest,
) -> Result<app::CreateMnemonics, validation::Error> {
    let result = match req.length {
        12 => Ok(Length::Words12),
        15 => Ok(Length::Words15),
        18 => Ok(Length::Words18),
        21 => Ok(Length::Words21),
        24 => Ok(Length::Words24),
        _ => Err(validation::error(
            "length",
            "Invalid Mnemonics Length. Must be 12, 15, 18, 21 or 24",
        )),
    };

    result.map(|length| app::CreateMnemonics { length })
}
