///! Crate level errors.
///
/// TODO: this can be refactored to separate CLI errors from library errors, which should make it
///  easier to convert the CLI part into a conditional compile.
use failure::Fail;

#[derive(Debug, Fail)]
#[fail(display = "error")]
pub enum Error {
    #[fail(display = "No bytes have been provided. Please use the --hex argument.")]
    DataRequestNoBytes,
    #[fail(
        display = "The string provided in the --hex field is not a valid hexadecimal byte string: {}",
        _0
    )]
    DataRequestHexNotValid(#[cause] hex::FromHexError),
    #[fail(
        display = "The string provided in the --hex field is not a valid Protocol Buffers byte string: {}",
        _0
    )]
    DataRequestProtoBufNotValid(#[cause] failure::Error),
    #[fail(display = "Error when serializing the result: {}", _0)]
    JsonSerialize(#[cause] serde_json::Error),
}
