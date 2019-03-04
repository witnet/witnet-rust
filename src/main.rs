#![deny(rust_2018_idioms)]
#![deny(non_upper_case_globals)]
#![deny(non_camel_case_types)]
#![deny(non_snake_case)]
#![deny(unused_mut)]

use std::process::exit;
use std::result::Result;

use env_logger;
use failure;
use structopt::StructOpt;

use witnet_node as node;

mod cli;
mod json_rpc_client;

fn main() {
    init_logger();

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

fn init_logger() {
    let env = env_logger::Env::default().default_filter_or("info");
    let mut logger = env_logger::Builder::from_env(env);

    logger.init();
}
