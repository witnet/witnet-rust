#![deny(rust_2018_idioms)]
#![deny(non_upper_case_globals)]
#![deny(non_camel_case_types)]
#![deny(non_snake_case)]
#![deny(unused_mut)]

use std::process::exit;

use env_logger;

use witnet_config as config;
use witnet_core as core;

mod cli;

use cli::{Cli, exec};

fn main() {
    // Init app logger
    env_logger::init();

    // Read configuration
    let configuration = config::Config::default();

    // Build default address from configuration
    let default_address = configuration.connections.server_addr;
    let default_address_cli = &format!("{}", default_address);

    // Get db root
    let default_db_root = configuration.storage.db_path;
}

fn run() -> Result<(), failure::Error> {
    let cli_args = Cli::from_args();
    cli::exec(args.cmd)?;
    Ok(())
}
