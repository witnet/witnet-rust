//! Crate level errors.
//
// TODO: this can be refactored to separate CLI errors from library errors, which should make it
//  easier to convert the CLI part into a conditional compile.
use thiserror::Error;

#[derive(Debug, Error)]
#[error("error")]
pub enum Error {
    #[error("No bytes have been provided. Please use the --hex or --from-solidity argument")]
    DataRequestNoBytes,
    #[error("The provided bytes are not a valid hexadecimal byte string: {0}")]
    DataRequestHexNotValid(hex::FromHexError),
    #[error("The provided bytes are not a valid Protocol Buffers byte string: {0}")]
    DataRequestProtoBufNotValid(anyhow::Error),
    #[error("Could not open Solidity file: {0}")]
    SolidityFileCantOpen(std::io::Error),
    #[error("Could not read Solidity file: {0}")]
    SolidityFileCantRead(std::io::Error),
    #[error(
        "Could not find constructor with request serialized in hex bytes inside the provided Solidity file"
    )]
    SolidityFileNoHexMatch(),
    #[error("Error when compiling regular expression: {0}")]
    RegularExpression(regex::Error),
    #[error("Error when serializing the result: {0}")]
    JsonSerialize(serde_json::Error),
}

/// Implicit, contextless wrapping of regular expression errors.
impl From<regex::Error> for Error {
    fn from(regex_error: regex::Error) -> Self {
        Self::RegularExpression(regex_error)
    }
}
