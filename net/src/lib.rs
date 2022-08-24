//! Servers and clients for creating services.

#![deny(rust_2018_idioms)]
#![deny(non_upper_case_globals)]
#![deny(non_camel_case_types)]
#![deny(non_snake_case)]
#![deny(unused_mut)]
#![deny(missing_docs)]

pub use isahc::http::Uri;
pub use surf;

pub mod client;
pub mod server;
