#![deny(rust_2018_idioms)]
#![deny(non_upper_case_globals)]
#![deny(non_camel_case_types)]
#![deny(non_snake_case)]
#![deny(unused_mut)]

use std::process::exit;
use std::result::Result;

use env_logger::Builder;
use failure;
use structopt::StructOpt;

use witnet_core as core;

mod cli;
mod json_rpc_client;

fn main() {
    // Init app logger
    Builder::from_default_env()
        // Remove comments to sprint demo
        //.default_format_timestamp(false)
        //.default_format_module_path(false)
        .init();

    if let Err(e) = run() {
        eprintln!("Error: {}", e);
        for cause in e.iter_causes() {
            eprintln!("Cause: {}", cause);
        }
        exit(1);
    }
}

fn run() -> Result<(), failure::Error> {
    let cli_args = cli::Cli::from_args();
    cli::exec(cli_args.cmd)?;
    Ok(())
}
