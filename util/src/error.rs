//! Convenient structs, implementations and types for nicer handling of our own custom error types.

use core::fmt::Display;
use std::fmt;

use failure::{Backtrace, Context, Fail};

/// Generic structure for application errors that can be implemented over any error of kind `K`.
/// Namely, `K` is intended to be a simple, C-style enum.
#[derive(Debug)]
pub struct WitnetError<K: Fail> {
    inner: Context<ErrorKind<K>>,
}

/// ErrorKind
#[derive(Debug, Fail)]
pub enum ErrorKind<K: Fail> {
    /// storage error
    #[fail(display = "StorageError: {}", 0)]
    Storage(K),
}

impl<K: Fail> WitnetError<K> {
    /// create storage error
    pub fn storage_err(err: K) -> Self {
        Self {
            inner: Context::new(ErrorKind::Storage(err)),
        }
    }
}

impl<K: Fail> From<ErrorKind<K>> for WitnetError<K> {
    fn from(err: ErrorKind<K>) -> Self {
        Self {
            inner: Context::new(err),
        }
    }
}

impl<K: Fail> Display for WitnetError<K> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        Display::fmt(&self.inner, f)
    }
}

impl<K: Fail> Fail for WitnetError<K> {
    fn cause(&self) -> Option<&dyn Fail> {
        self.inner.cause()
    }

    fn backtrace(&self) -> Option<&Backtrace> {
        self.inner.backtrace()
    }
}

/// Result
pub type WitnetResult<T, K> = std::result::Result<T, WitnetError<K>>;
