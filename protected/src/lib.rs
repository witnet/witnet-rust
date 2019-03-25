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

use memzero::Memzero;

/// Protected set of bytes
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Protected(Memzero<Vec<u8>>);

impl<T: Into<Vec<u8>>> From<T> for Protected {
    fn from(x: T) -> Self {
        Protected::new(x.into())
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

impl Protected {
    /// Create new protected set of bytes.
    pub fn new<T: Into<Vec<u8>>>(m: T) -> Self {
        Protected(m.into().into())
    }
}

impl std::fmt::Debug for Protected {
    fn fmt(&self, fmt: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(fmt, "Protected(***)")
    }
}
