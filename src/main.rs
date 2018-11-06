#![deny(rust_2018_idioms)]
#![deny(non_upper_case_globals)]
#![deny(non_camel_case_types)]
#![deny(non_snake_case)]
#![deny(unused_mut)]

use std::process::exit;
use std::result::Result;

use log::error;
use env_logger;
use failure;
use structopt::StructOpt;

use witnet_core as core;

mod cli;

fn main() {
    // Init app logger
    env_logger::init();

    if let Err(e) = run() {
        error!("Error: {}", e);
        for cause in e.iter_causes() {
            error!("Cause: {}", cause);
        }
        exit(1);
    }
}

fn run() -> Result<(), failure::Error> {
    let cli_args = cli::Cli::from_args();
    cli::exec(cli_args.cmd)?;
    Ok(())
}
