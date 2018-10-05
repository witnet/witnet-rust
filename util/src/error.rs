//! Convenient structs, implementations and types for nicer handling of our own custom error types.

use core::fmt::Display;
use std::fmt;

use failure::{Backtrace, Context, Fail};

/// Generic structure for witnet errors
#[derive(Debug)]
pub struct WitnetError<K: Fail> {
    inner: Context<K>,
}

impl<K: Fail> From<K> for WitnetError<K> {
    fn from(err: K) -> Self {
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
