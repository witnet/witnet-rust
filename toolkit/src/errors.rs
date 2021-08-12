///! Crate level errors.
///
/// TODO: this can be refactored to separate CLI errors from library errors, which should make it
///  easier to convert the CLI part into a conditional compile.
use failure::Fail;

#[derive(Debug, Fail)]
#[fail(display = "error")]
pub enum Error {
    #[fail(
        display = "No bytes have been provided. Please use the --hex or --from-solidity argument"
    )]
    DataRequestNoBytes,
    #[fail(
        display = "The provided bytes are not a valid hexadecimal byte string: {}",
        _0
    )]
    DataRequestHexNotValid(#[cause] hex::FromHexError),
    #[fail(
        display = "The provided bytes are not a valid Protocol Buffers byte string: {}",
        _0
    )]
    DataRequestProtoBufNotValid(#[cause] failure::Error),
    #[fail(display = "Could not open Solidity file: {}", _0)]
    SolidityFileCantOpen(#[cause] std::io::Error),
    #[fail(display = "Could not read Solidity file: {}", _0)]
    SolidityFileCantRead(#[cause] std::io::Error),
    #[fail(
        display = "Could not find constructor with request serialized in hex bytes inside the provided Solidity file"
    )]
    SolidityFileNoHexMatch(),
    #[fail(display = "Error when compiling regular expression: {}", _0)]
    RegularExpression(#[cause] regex::Error),
    #[fail(display = "Error when serializing the result: {}", _0)]
    JsonSerialize(#[cause] serde_json::Error),
}

/// Implicit, contextless wrapping of regular expression errors.
impl From<regex::Error> for Error {
    fn from(regex_error: regex::Error) -> Self {
        Self::RegularExpression(regex_error)
    }
}
