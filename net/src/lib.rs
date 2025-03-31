//! Servers and clients for creating services.

#![deny(rust_2018_idioms)]
#![deny(non_upper_case_globals)]
#![deny(non_camel_case_types)]
#![deny(non_snake_case)]
#![deny(unused_mut)]
#![deny(missing_docs)]

/// Wrapper around reqwest::Url to define our own URL type
pub type Uri = reqwest::Url;

pub mod client;
pub mod server;
