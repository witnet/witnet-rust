//! # Protected
//!
//! Protected set of bytes that will be zeroed out when the value of
//! type [`Protected`](Protected) containing them is dropped.

#![deny(rust_2018_idioms)]
#![deny(non_upper_case_globals)]
#![deny(non_camel_case_types)]
#![deny(non_snake_case)]
#![deny(unused_mut)]
#![deny(missing_docs)]

use std::ops::{Deref, DerefMut};
use std::str;

use memzero::Memzero;

#[cfg(feature = "serde")]
mod serde;

/// Protected set of bytes
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Protected(Memzero<Vec<u8>>);

impl Protected {
    /// Create new protected set of bytes.
    pub fn new<T: Into<Vec<u8>>>(m: T) -> Self {
        Protected(m.into().into())
    }
}

impl<T: Into<Vec<u8>>> From<T> for Protected {
    fn from(x: T) -> Self {
        Self::new(x.into())
    }
}

impl AsRef<[u8]> for Protected {
    fn as_ref(&self) -> &[u8] {
        &*self.0
    }
}

impl AsMut<[u8]> for Protected {
    fn as_mut(&mut self) -> &mut [u8] {
        &mut *self.0
    }
}

impl Deref for Protected {
    type Target = [u8];

    fn deref(&self) -> &Self::Target {
        self.0.deref()
    }
}

impl DerefMut for Protected {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.0.deref_mut()
    }
}

impl std::fmt::Debug for Protected {
    fn fmt(&self, fmt: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(fmt, "Protected(***)")
    }
}

/// Protected string
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ProtectedString(Protected);

impl ProtectedString {
    const INVALID_STRING: &'static str = "ProtectedString does not contain a valid UTF-8 string";

    /// Create new protected string.
    pub fn new<T: Into<String>>(m: T) -> Self {
        ProtectedString(Protected::new(m.into().into_bytes()))
    }
}

impl<T: ToString> From<T> for ProtectedString {
    fn from(x: T) -> Self {
        Self::new(x.to_string())
    }
}

impl AsRef<str> for ProtectedString {
    fn as_ref(&self) -> &str {
        let bytes = self.0.as_ref();
        str::from_utf8(bytes).expect(Self::INVALID_STRING)
    }
}

impl AsRef<[u8]> for ProtectedString {
    fn as_ref(&self) -> &[u8] {
        self.0.as_ref()
    }
}

impl std::fmt::Debug for ProtectedString {
    fn fmt(&self, fmt: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(fmt, "ProtectedString(***)")
    }
}

impl Into<Protected> for ProtectedString {
    fn into(self) -> Protected {
        self.0
    }
}
