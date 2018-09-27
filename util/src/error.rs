//! Convenient structs, implementations and types for nicer handling of our own custom error types.

use core::fmt::Display;
use std::fmt::Debug;

/// Generic structure for application errors that can be implemented over any error of kind `K`.
/// Namely, `K` is intended to be a simple, C-style enum.
///
/// # Examples
///
/// ```
/// #[derive(Debug)]
/// enum MyOwnErrorCodes {
///     AnError,
///     AnotherError
/// };
///
/// type MyErrorType = witnet_util::error::Error<MyOwnErrorCodes>;
/// ```
#[derive(Debug)]
pub struct Error<K: Debug> {
    kind: K,
    message: String
}

/// Implement the `::new()` constructor for the generic Error<K> type.
impl <K: Debug> Error<K> {

    /// Constructs a new `Error<K>` for a member of `K` and a `message` string.
    ///
    /// # Examples
    ///
    /// ```
    /// #[derive(Debug)]
    /// enum MyOwnErrorCodes {
    ///     AnError,
    ///     AnotherError
    /// };
    ///
    /// witnet_util::error::Error::new(
    ///     MyOwnErrorCodes::AnError,
    ///     String::from("This is a good example of an error")
    /// );
    /// ```
    pub fn new(kind: K, message: String) -> Self {
        Error { kind, message }
    }
}

/// Implement the `std::error::Error` trait for our custom `Error<K>`.
impl <K: Debug> std::error::Error for Error<K> {
    fn description(&self) -> &str {
        &self.message
    }
}

/// Implement the `std::fmt::Display::fmt` trait for our custom `Error<K>`.
impl <K: Debug> std::fmt::Display for Error<K> {

    fn fmt<'f>(&self, formatter: &mut std::fmt::Formatter<'f>) -> std::fmt::Result {
        Display::fmt(&self.message, formatter)
    }
}

/// Expose a generic `Result<T, K>` that can be used by other modules to define their own
/// `Result<T>` type by specifying `K`.
///
/// # Example
///
/// ```
/// #[derive(Debug)]
/// enum MyOwnErrorCodes {
///     AnError,
///     AnotherError
/// };
///
/// type Result<T> = witnet_util::error::Result<T, MyOwnErrorCodes>;
/// ```
pub type Result<T, K> = std::result::Result<T, Error<K>>;