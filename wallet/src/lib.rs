//! Wallet implementation for Witnet

#![deny(rust_2018_idioms)]
#![deny(non_upper_case_globals)]
#![deny(non_camel_case_types)]
#![deny(non_snake_case)]
#![deny(unused_mut)]
#![deny(missing_docs)]

#[macro_use]
extern crate diesel;
#[macro_use]
extern crate diesel_migrations;

mod account;
mod constants;
mod crypto;
mod db;
mod error;
mod models;
mod radon;
mod result;
mod schema;
mod server;
mod types;
mod wallet;
mod wallets;

pub use server::run as run_server;
